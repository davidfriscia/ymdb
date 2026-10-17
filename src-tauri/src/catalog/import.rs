//! Orchestration of catalog imports from `mame -listxml` (machines) and
//! `mame -listsoftware` (software lists).
//!
//! Steps: validate the configured executable (to record its version), start
//! MAME, stream its output through the parser into a new catalog database
//! file, check the process result, then commit and swap the catalog. Any
//! failure or cancellation before the swap discards the new database file and
//! keeps the previous catalog.

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use super::parser::{ListXmlParser, ParseError};
use super::software_parser::{SoftwareListItem, SoftwareListParser};
use super::store::{
    CatalogInfo, CatalogSource, CatalogStore, CompletedImport, CompletedSoftwareImport, StoreError,
};
use crate::mame::process::{self, ActivityReader, ProcessError, StreamingOutcome, Termination};
use crate::mame::validation::{ValidationStatus, validate_executable};

/// MAME is terminated if it produces no output for this long. Measured with
/// MAME 0.280: `-listxml` runs for 2-3 s and `-listsoftware` for 12-66 s
/// depending on system load, with pauses between output chunks below 2 s; the
/// limit only catches a hung process.
pub const LISTXML_INACTIVITY_TIMEOUT: Duration = Duration::from_secs(120);
/// Read buffer for the MAME output.
const READ_BUFFER_SIZE: usize = 256 * 1024;
/// Maximum length of the MAME standard error included in error messages.
const MAX_STDERR_IN_ERROR: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportPhase {
    /// Running `mame -version` to identify the build.
    Validating,
    /// Reading the MAME output and storing machines or software.
    Reading,
    /// Verifying, committing and replacing the catalog.
    Finalizing,
}

/// Progress shared between the import thread and status queries.
#[derive(Debug, Default)]
pub struct ImportProgress {
    phase: AtomicU8,
    /// Machines or software stored so far.
    items: AtomicU64,
}

impl ImportProgress {
    pub fn phase(&self) -> ImportPhase {
        match self.phase.load(Ordering::Relaxed) {
            0 => ImportPhase::Validating,
            1 => ImportPhase::Reading,
            _ => ImportPhase::Finalizing,
        }
    }

    pub fn items_processed(&self) -> u64 {
        self.items.load(Ordering::Relaxed)
    }

    fn set_items(&self, items: u64) {
        self.items.store(items, Ordering::Relaxed);
    }

    fn set_phase(&self, phase: ImportPhase) {
        let value = match phase {
            ImportPhase::Validating => 0,
            ImportPhase::Reading => 1,
            ImportPhase::Finalizing => 2,
        };
        self.phase.store(value, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ImportError {
    /// The user cancelled the import.
    Cancelled,
    /// The configured executable is not a valid MAME installation.
    InvalidMame { status: ValidationStatus },
    /// Software lists require an imported machine catalog.
    MachineCatalogRequired,
    /// The configured executable is not the one the machine catalog was
    /// imported from (another MAME build, another file, or a modified file).
    CatalogMismatch {
        catalog_release: String,
        catalog_revision: String,
        executable_release: String,
        executable_revision: String,
    },
    /// MAME could not be started or waited for.
    LaunchFailed { message: String },
    /// MAME exited with an error.
    MameFailed {
        exit_code: Option<i32>,
        stderr: String,
    },
    /// MAME stopped producing output and was terminated.
    Inactive { timeout_seconds: u64 },
    /// The output of MAME could not be parsed.
    Parse { message: String },
    /// The catalog database could not be written.
    Database { message: String },
}

impl From<ParseError> for ImportError {
    fn from(error: ParseError) -> Self {
        Self::Parse {
            message: error.to_string(),
        }
    }
}

impl From<StoreError> for ImportError {
    fn from(error: StoreError) -> Self {
        Self::Database {
            message: error.to_string(),
        }
    }
}

/// Imports the catalog from the MAME executable at `executable`.
///
/// Blocks until the import ends; run it on a background thread. Setting
/// `cancel` stops the import (and the MAME process) as soon as possible.
pub fn import_catalog(
    executable: &Path,
    store: &CatalogStore,
    cancel: &Arc<AtomicBool>,
    progress: &ImportProgress,
) -> Result<CatalogInfo, ImportError> {
    progress.set_phase(ImportPhase::Validating);
    let source = identify_source(executable)?;
    check_cancelled(cancel)?;

    progress.set_phase(ImportPhase::Reading);
    stream_mame_output(executable, "-listxml", cancel, |reader, finish_process| {
        import_from_reader(reader, store, source, cancel, progress, finish_process)
    })
}

/// Imports the software lists from the MAME executable at `executable` into
/// the current catalog, replacing its previous software lists.
///
/// The executable must be the one the machine catalog was imported from
/// (same MAME release and revision, same file, unchanged); otherwise
/// [`ImportError::CatalogMismatch`] is returned and nothing is changed.
/// Blocks until the import ends; run it on a background thread.
pub fn import_software_lists(
    executable: &Path,
    store: &CatalogStore,
    cancel: &Arc<AtomicBool>,
    progress: &ImportProgress,
) -> Result<CatalogInfo, ImportError> {
    progress.set_phase(ImportPhase::Validating);
    let source = identify_source(executable)?;
    let catalog = store
        .read_info()?
        .ok_or(ImportError::MachineCatalogRequired)?;
    check_same_build(&catalog, &source)?;
    check_cancelled(cancel)?;

    progress.set_phase(ImportPhase::Reading);
    stream_mame_output(
        executable,
        "-listsoftware",
        cancel,
        |reader, finish_process| {
            import_software_from_reader(reader, store, source, cancel, progress, finish_process)
        },
    )
}

/// Software lists may only be added to a catalog built from the same MAME
/// build, read from the same unchanged executable.
fn check_same_build(catalog: &CatalogInfo, source: &CatalogSource) -> Result<(), ImportError> {
    let same_build = catalog.mame_release == source.mame_release
        && catalog.mame_revision == source.mame_revision;
    let same_executable = catalog.executable_path == source.executable_path
        && catalog.executable_size == source.executable_size
        && catalog.executable_modified_at == source.executable_modified_at;
    if same_build && same_executable {
        Ok(())
    } else {
        Err(ImportError::CatalogMismatch {
            catalog_release: catalog.mame_release.clone(),
            catalog_revision: catalog.mame_revision.clone(),
            executable_release: source.mame_release.clone(),
            executable_revision: source.mame_revision.clone(),
        })
    }
}

/// Runs `<executable> <argument>` and lets `consume` read its standard
/// output. `consume` must call the provided function once the whole output
/// has been read and before committing: it waits for MAME and fails unless
/// MAME exited successfully.
fn stream_mame_output<T>(
    executable: &Path,
    argument: &str,
    cancel: &Arc<AtomicBool>,
    consume: impl FnOnce(
        BufReader<ActivityReader>,
        &mut dyn FnMut() -> Result<(), ImportError>,
    ) -> Result<T, ImportError>,
) -> Result<T, ImportError> {
    let mut command = Command::new(executable);
    command.arg(argument);
    if let Some(dir) = executable.parent() {
        command.current_dir(dir);
    }
    let mut process =
        process::spawn_streaming(command, LISTXML_INACTIVITY_TIMEOUT, Arc::clone(cancel))
            .map_err(launch_error)?;
    let stdout = BufReader::with_capacity(
        READ_BUFFER_SIZE,
        process.take_stdout().expect("stdout is piped"),
    );

    let mut process = Some(process);
    let result = {
        let mut finish_process = || {
            let outcome = process
                .take()
                .ok_or_else(|| ImportError::LaunchFailed {
                    message: "MAME process already finished".to_owned(),
                })?
                .finish()
                .map_err(launch_error)?;
            check_outcome(outcome)
        };
        consume(stdout, &mut finish_process)
    };

    match (result, process) {
        (Ok(info), _) => Ok(info),
        (Err(error), None) => Err(error),
        // The import stopped before the output was fully read. A cancellation,
        // an inactivity timeout or a MAME failure that ended the output
        // explains the error better than the truncated document; an exit code
        // caused by killing the process here does not.
        (Err(error), Some(process)) => {
            let exited_on_its_own = process.has_exited();
            process.kill();
            match process.finish() {
                Ok(outcome) if outcome.termination.is_some() || exited_on_its_own => {
                    Err(check_outcome(outcome).err().unwrap_or(error))
                }
                _ => Err(error),
            }
        }
    }
}

/// Builds a new catalog from a `-listxml` document and replaces the current
/// catalog with it. `before_commit` runs after the whole document has been
/// read; an error from it aborts the import.
pub fn import_from_reader<R: BufRead>(
    reader: R,
    store: &CatalogStore,
    source: CatalogSource,
    cancel: &AtomicBool,
    progress: &ImportProgress,
    before_commit: impl FnOnce() -> Result<(), ImportError>,
) -> Result<CatalogInfo, ImportError> {
    let started = Instant::now();
    let mut writer = store.begin_import()?;
    let mut parser = ListXmlParser::new(reader);
    while let Some(machine) = parser.next_machine()? {
        check_cancelled(cancel)?;
        writer.insert_machine(&machine)?;
        progress.set_items(writer.machine_count());
    }
    before_commit()?;
    check_cancelled(cancel)?;

    progress.set_phase(ImportPhase::Finalizing);
    let header = parser.header().cloned().unwrap_or_default();
    let completed = CompletedImport {
        source,
        mame_build: header.build,
        mame_config: header.mame_config,
        mame_debug: header.debug,
        imported_at: now_millis(),
        import_duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    };
    Ok(writer.finish(&completed)?)
}

/// Replaces the software lists of the current catalog with those of a
/// `-listsoftware` document. `before_commit` runs after the whole document
/// has been read; an error from it aborts the import.
pub fn import_software_from_reader<R: BufRead>(
    reader: R,
    store: &CatalogStore,
    source: CatalogSource,
    cancel: &AtomicBool,
    progress: &ImportProgress,
    before_commit: impl FnOnce() -> Result<(), ImportError>,
) -> Result<CatalogInfo, ImportError> {
    let started = Instant::now();
    let mut writer = store.begin_software_import()?;
    let mut parser = SoftwareListParser::new(reader);
    while let Some(item) = parser.next_item()? {
        check_cancelled(cancel)?;
        match item {
            SoftwareListItem::List(list) => writer.insert_software_list(&list)?,
            SoftwareListItem::Software(software) => {
                writer.insert_software(&software)?;
                progress.set_items(writer.software_count());
            }
        }
    }
    before_commit()?;
    check_cancelled(cancel)?;

    progress.set_phase(ImportPhase::Finalizing);
    let completed = CompletedSoftwareImport {
        source,
        imported_at: now_millis(),
        import_duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    };
    Ok(writer.finish_software(&completed)?)
}

/// Validates the executable and collects the information recorded with the catalog.
fn identify_source(executable: &Path) -> Result<CatalogSource, ImportError> {
    let report = validate_executable(executable);
    let Some(version) = report
        .version
        .filter(|_| report.status == ValidationStatus::Valid)
    else {
        return Err(ImportError::InvalidMame {
            status: report.status,
        });
    };
    let metadata = fs::metadata(executable).ok();
    Ok(CatalogSource {
        mame_release: version.release,
        mame_revision: version.revision,
        executable_path: executable.to_path_buf(),
        executable_size: metadata.as_ref().map(fs::Metadata::len),
        executable_modified_at: metadata
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .and_then(|elapsed| u64::try_from(elapsed.as_millis()).ok()),
    })
}

fn check_outcome(outcome: StreamingOutcome) -> Result<(), ImportError> {
    match outcome.termination {
        Some(Termination::Cancelled) => Err(ImportError::Cancelled),
        Some(Termination::Inactive) => Err(ImportError::Inactive {
            timeout_seconds: LISTXML_INACTIVITY_TIMEOUT.as_secs(),
        }),
        None if outcome.exit_code == Some(0) => Ok(()),
        None => Err(ImportError::MameFailed {
            exit_code: outcome.exit_code,
            stderr: truncate(outcome.stderr, MAX_STDERR_IN_ERROR),
        }),
    }
}

fn check_cancelled(cancel: &AtomicBool) -> Result<(), ImportError> {
    if cancel.load(Ordering::Relaxed) {
        Err(ImportError::Cancelled)
    } else {
        Ok(())
    }
}

fn launch_error(error: ProcessError) -> ImportError {
    let message = match error {
        ProcessError::Spawn(error) => format!("failed to start MAME: {error}"),
        ProcessError::Wait(error) => format!("failed to wait for MAME: {error}"),
        ProcessError::TimedOut => "MAME did not exit after its output ended".to_owned(),
    };
    ImportError::LaunchFailed { message }
}

fn truncate(mut text: String, max_len: usize) -> String {
    if text.len() > max_len {
        let mut cut = max_len;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
        text.push('…');
    }
    text
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use rusqlite::Connection;

    use super::*;
    use crate::catalog::classification::CLASSIFICATIONS_FILE_NAME;
    use crate::catalog::store::CATALOG_FILE_NAME;

    const SAMPLE: &str = include_str!("testdata/listxml-sample.xml");

    fn source(release: &str) -> CatalogSource {
        CatalogSource {
            mame_release: release.to_owned(),
            mame_revision: format!("mame{}", release.replace('.', "")),
            executable_path: PathBuf::from("/opt/mame/mame"),
            executable_size: Some(1234),
            executable_modified_at: Some(5678),
        }
    }

    fn import(
        store: &CatalogStore,
        xml: &str,
        release: &str,
        cancel: &AtomicBool,
    ) -> Result<CatalogInfo, ImportError> {
        import_from_reader(
            xml.as_bytes(),
            store,
            source(release),
            cancel,
            &ImportProgress::default(),
            || Ok(()),
        )
    }

    fn not_cancelled() -> AtomicBool {
        AtomicBool::new(false)
    }

    fn open(store: &CatalogStore) -> Connection {
        Connection::open(store.catalog_path()).expect("open catalog")
    }

    fn assert_no_partial_file(dir: &Path) {
        let names: Vec<_> = fs::read_dir(dir)
            .expect("read dir")
            .map(|entry| entry.expect("entry").file_name())
            .collect();
        // The classification projection is created next to the catalog when it is read.
        assert!(
            names
                .iter()
                .all(|name| name == CATALOG_FILE_NAME || name == CLASSIFICATIONS_FILE_NAME),
            "unexpected files: {names:?}"
        );
    }

    fn count(db: &Connection, sql: &str) -> i64 {
        db.query_row(sql, [], |row| row.get(0)).expect(sql)
    }

    #[test]
    fn imports_synthetic_dataset_with_summary_and_metadata() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = CatalogStore::in_dir(dir.path());
        let progress = ImportProgress::default();

        let info = import_from_reader(
            SAMPLE.as_bytes(),
            &store,
            source("0.280"),
            &not_cancelled(),
            &progress,
            || Ok(()),
        )
        .expect("import");

        assert_eq!(info.mame_release, "0.280");
        assert_eq!(info.mame_revision, "mame0280");
        assert_eq!(info.mame_build.as_deref(), Some("0.280 (mame0280)"));
        assert_eq!(info.executable_path, PathBuf::from("/opt/mame/mame"));
        assert!(info.imported_at > 0);
        assert!(info.database_size > 0);
        let stats = &info.statistics;
        assert_eq!(stats.machines, 8);
        assert_eq!(stats.devices, 1);
        assert_eq!(stats.bios, 1);
        assert_eq!(stats.mechanical, 1);
        assert_eq!(stats.clones, 1);
        assert_eq!(stats.runnable_systems, 6);
        assert_eq!(stats.roms, 8);
        assert_eq!(stats.disks, 1);
        assert_eq!(stats.software_lists, 2);
        assert_eq!(progress.items_processed(), 8);
        assert_eq!(progress.phase(), ImportPhase::Finalizing);
        assert_eq!(store.read_info().expect("read"), Some(info));
        assert_no_partial_file(dir.path());
    }

    #[test]
    fn persists_parent_clone_rom_bios_and_device_relations() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = CatalogStore::in_dir(dir.path());
        import(&store, SAMPLE, "0.280", &not_cancelled()).expect("import");
        let db = open(&store);

        let relation = |name: &str| -> (Option<String>, Option<String>) {
            db.query_row(
                "SELECT clone_of, rom_of FROM machines WHERE name = ?1",
                [name],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("machine")
        };
        assert_eq!(
            relation("pacman"),
            (Some("puckman".into()), Some("puckman".into()))
        );
        assert_eq!(relation("kof98"), (None, Some("neogeo".into())));

        let bios_rom: (Option<String>, Option<String>) = db
            .query_row(
                "SELECT bios, merge_name FROM machine_roms
                 WHERE machine_name = 'kof98' AND name = 'sp-s2.sp1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("rom");
        assert_eq!(bios_rom, (Some("euro".into()), Some("sp-s2.sp1".into())));

        let default_bios: String = db
            .query_row(
                "SELECT name FROM machine_bios_sets WHERE machine_name = 'neogeo' AND is_default = 1",
                [],
                |row| row.get(0),
            )
            .expect("bios set");
        assert_eq!(default_bios, "japan");

        assert_eq!(
            count(
                &db,
                "SELECT instance_count FROM machine_device_refs
                 WHERE machine_name = 'neogeo' AND device_name = 'z80'"
            ),
            2
        );

        let extensions: String = db
            .query_row(
                "SELECT group_concat(e.extension, ',') FROM machine_media_devices d
                 JOIN machine_media_device_extensions e ON e.media_device_id = d.id
                 WHERE d.machine_name = 'c64' AND d.device_type = 'cartridge'",
                [],
                |row| row.get(0),
            )
            .expect("extensions");
        assert_eq!(extensions, "80,crt");

        let (sample_of, mechanical, requires_artwork): (Option<String>, bool, bool) = db
            .query_row(
                "SELECT sample_of, is_mechanical, driver_requires_artwork
                 FROM machines WHERE name = 'fruitbank'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("fruitbank");
        assert_eq!(sample_of.as_deref(), Some("fruitsamples"));
        assert!(mechanical && requires_artwork);

        assert_eq!(
            count(&db, "SELECT COUNT(*) FROM pragma_foreign_key_check"),
            0
        );
    }

    #[test]
    fn rejects_dangling_references_and_keeps_no_catalog() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = CatalogStore::in_dir(dir.path());
        let xml = concat!(
            r#"<mame build="x"><machine name="clone" cloneof="missing">"#,
            r#"<description>Clone</description></machine></mame>"#,
        );

        let error = import(&store, xml, "0.280", &not_cancelled()).expect_err("dangling reference");

        assert!(
            matches!(&error, ImportError::Database { message }
                if message.contains("references a missing machines")),
            "{error:?}"
        );
        assert_eq!(store.read_info().expect("read"), None);
        assert!(fs::read_dir(dir.path()).expect("read dir").next().is_none());
    }

    #[test]
    fn parse_error_keeps_previous_catalog() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = CatalogStore::in_dir(dir.path());
        let previous = import(&store, SAMPLE, "0.279", &not_cancelled()).expect("first import");
        let truncated = &SAMPLE[..SAMPLE.len() / 2];

        let error = import(&store, truncated, "0.280", &not_cancelled()).expect_err("truncated");

        assert!(matches!(error, ImportError::Parse { .. }), "{error:?}");
        assert_eq!(store.read_info().expect("read"), Some(previous));
        assert_no_partial_file(dir.path());
    }

    #[test]
    fn cancellation_keeps_previous_catalog() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = CatalogStore::in_dir(dir.path());
        let previous = import(&store, SAMPLE, "0.279", &not_cancelled()).expect("first import");

        let error = import(&store, SAMPLE, "0.280", &AtomicBool::new(true)).expect_err("cancelled");

        assert_eq!(error, ImportError::Cancelled);
        assert_eq!(store.read_info().expect("read"), Some(previous));
        assert_no_partial_file(dir.path());
    }

    #[test]
    fn process_failure_before_commit_keeps_previous_catalog() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = CatalogStore::in_dir(dir.path());
        let previous = import(&store, SAMPLE, "0.279", &not_cancelled()).expect("first import");
        let failure = ImportError::MameFailed {
            exit_code: Some(1),
            stderr: "fatal error".to_owned(),
        };

        let error = import_from_reader(
            SAMPLE.as_bytes(),
            &store,
            source("0.280"),
            &not_cancelled(),
            &ImportProgress::default(),
            || Err(failure.clone()),
        )
        .expect_err("process failed");

        assert_eq!(error, failure);
        assert_eq!(store.read_info().expect("read"), Some(previous));
        assert_no_partial_file(dir.path());
    }

    #[test]
    fn reimport_replaces_previous_catalog() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = CatalogStore::in_dir(dir.path());
        import(&store, SAMPLE, "0.279", &not_cancelled()).expect("first import");
        let smaller = concat!(
            r#"<mame build="0.280 (mame0280)"><machine name="solo">"#,
            r#"<description>Solo</description></machine></mame>"#,
        );

        let info = import(&store, smaller, "0.280", &not_cancelled()).expect("reimport");

        assert_eq!(info.mame_release, "0.280");
        assert_eq!(info.statistics.machines, 1);
        assert_eq!(store.read_info().expect("read"), Some(info));
        assert_eq!(count(&open(&store), "SELECT COUNT(*) FROM machine_roms"), 0);
        assert_no_partial_file(dir.path());
    }

    #[test]
    fn invalid_executable_is_rejected_before_import() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = CatalogStore::in_dir(dir.path());

        let error = import_catalog(
            &dir.path().join("missing-mame"),
            &store,
            &Arc::new(AtomicBool::new(false)),
            &ImportProgress::default(),
        )
        .expect_err("invalid executable");

        assert_eq!(
            error,
            ImportError::InvalidMame {
                status: ValidationStatus::NotFound
            }
        );
        assert_eq!(store.read_info().expect("read"), None);
    }

    #[test]
    fn serializes_errors_to_frontend_contract() {
        let error = ImportError::MameFailed {
            exit_code: Some(2),
            stderr: "boom".to_owned(),
        };

        assert_eq!(
            serde_json::to_value(error).expect("serialize"),
            serde_json::json!({ "kind": "mameFailed", "exitCode": 2, "stderr": "boom" })
        );
        assert_eq!(
            serde_json::to_value(ImportError::Cancelled).expect("serialize"),
            serde_json::json!({ "kind": "cancelled" })
        );
    }

    const SOFTWARE_SAMPLE: &str = include_str!("testdata/listsoftware-sample.xml");

    fn import_software(
        store: &CatalogStore,
        xml: &str,
        source: CatalogSource,
        cancel: &AtomicBool,
    ) -> Result<CatalogInfo, ImportError> {
        import_software_from_reader(
            xml.as_bytes(),
            store,
            source,
            cancel,
            &ImportProgress::default(),
            || Ok(()),
        )
    }

    /// A store with the machine fixture imported from MAME "0.280".
    fn store_with_machines(dir: &Path) -> CatalogStore {
        let store = CatalogStore::in_dir(dir);
        import(&store, SAMPLE, "0.280", &not_cancelled()).expect("machine import");
        store
    }

    #[test]
    fn imports_software_lists_into_the_machine_catalog() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = store_with_machines(dir.path());
        let machines_before = store
            .read_info()
            .expect("read")
            .expect("catalog")
            .statistics;
        let progress = ImportProgress::default();

        let info = import_software_from_reader(
            SOFTWARE_SAMPLE.as_bytes(),
            &store,
            source("0.280"),
            &not_cancelled(),
            &progress,
            || Ok(()),
        )
        .expect("software import");

        assert_eq!(
            info.statistics, machines_before,
            "machine data is unchanged"
        );
        let software = info.software.expect("software info");
        assert_eq!(software.mame_release, "0.280");
        assert!(software.matches_machine_catalog);
        let stats = software.statistics;
        assert_eq!((stats.lists, stats.software), (4, 7));
        assert_eq!(
            (stats.clones, stats.partially_supported, stats.unsupported),
            (1, 1, 1)
        );
        assert_eq!((stats.parts, stats.data_areas, stats.roms), (9, 8, 9));
        assert_eq!((stats.disk_areas, stats.disks), (2, 2));
        // The c64 machine references `vic10`, which the sample does not provide.
        assert_eq!(stats.missing_referenced_lists, 1);
        assert_eq!(progress.items_processed(), 7);
        assert_no_partial_file(dir.path());
    }

    #[test]
    fn links_machines_to_software_lists_without_duplicating_them() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = store_with_machines(dir.path());
        import_software(&store, SOFTWARE_SAMPLE, source("0.280"), &not_cancelled())
            .expect("software import");
        let db = open(&store);

        // c64 -> c64_cart (original, NTSC filter) -> 3 software.
        let (status, filter, software): (String, Option<String>, i64) = db
            .query_row(
                "SELECT m.status, m.filter, COUNT(s.id)
                 FROM machine_software_lists m
                 JOIN software_lists l ON l.name = m.list_name
                 JOIN software s ON s.list_name = l.name
                 WHERE m.machine_name = 'c64' AND m.list_name = 'c64_cart'
                 GROUP BY m.id",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("relation");
        assert_eq!(
            (status.as_str(), filter.as_deref(), software),
            ("original", Some("NTSC"), 3)
        );
        // A list is stored once, whatever the number of machines using it.
        assert_eq!(
            count(
                &db,
                "SELECT COUNT(*) FROM software_lists WHERE name = 'c64_cart'"
            ),
            1
        );
        // The compatibility metadata is kept verbatim for later evaluation.
        let compatibility: String = db
            .query_row(
                "SELECT f.value FROM software_shared_features f
                 JOIN software s ON s.id = f.software_id
                 WHERE s.list_name = 'c64_cart' AND s.name = 'gameb2' AND f.name = 'compatibility'",
                [],
                |row| row.get(0),
            )
            .expect("compatibility");
        assert_eq!(compatibility, "NTSC");
    }

    #[test]
    fn persists_software_relations_parts_and_media() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = store_with_machines(dir.path());
        import_software(&store, SOFTWARE_SAMPLE, source("0.280"), &not_cancelled())
            .expect("software import");
        let db = open(&store);

        // Same short name in two lists.
        assert_eq!(
            count(&db, "SELECT COUNT(*) FROM software WHERE name = 'tetris'"),
            2
        );
        // Clone listed before its parent.
        let parent: (String, String) = db
            .query_row(
                "SELECT p.name, p.description FROM software c
                 JOIN software p ON p.list_name = c.list_name AND p.name = c.clone_of
                 WHERE c.list_name = 'c64_cart' AND c.name = 'gameb2'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("parent");
        assert_eq!(parent, ("gamebase".to_owned(), "Game Base".to_owned()));
        // Several parts with their interfaces.
        let parts: String = db
            .query_row(
                "SELECT group_concat(p.name || ':' || p.interface, ',') FROM software_parts p
                 JOIN software s ON s.id = p.software_id
                 WHERE s.list_name = 'pcecd' AND s.name = 'ys12'",
                [],
                |row| row.get(0),
            )
            .expect("parts");
        assert_eq!(parts, "cdrom:pce_cdrom,save:pce_hdd");
        // Several data areas with their ROM checksums.
        let areas: String = db
            .query_row(
                "SELECT group_concat(a.name || '=' || r.crc, ',') FROM software_data_areas a
                 JOIN software_roms r ON r.data_area_id = a.id
                 JOIN software_parts p ON p.id = a.part_id
                 JOIN software s ON s.id = p.software_id
                 WHERE s.list_name = 'nes' AND s.name = 'smb'",
                [],
                |row| row.get(0),
            )
            .expect("areas");
        assert_eq!(areas, "prg=967a605f,chr=867b51ad");
        // Loading instruction without ROM name.
        assert_eq!(
            count(
                &db,
                "SELECT COUNT(*) FROM software_roms WHERE name IS NULL AND load_flag = 'reload'"
            ),
            1
        );
        assert_eq!(
            count(
                &db,
                "SELECT COUNT(*) FROM software_disks WHERE is_writeable = 1"
            ),
            1
        );
        assert_eq!(
            count(&db, "SELECT COUNT(*) FROM pragma_foreign_key_check"),
            0
        );
        assert_eq!(
            db.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
                .expect("integrity"),
            "ok"
        );
    }

    #[test]
    fn rejects_software_lists_from_another_mame_build() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = store_with_machines(dir.path());
        let before = store.read_info().expect("read");

        let error = import_software(&store, SOFTWARE_SAMPLE, source("0.281"), &not_cancelled())
            .expect_err("different build");

        assert!(
            matches!(&error, ImportError::Database { message } if message.contains("MAME build")),
            "{error:?}"
        );
        assert_eq!(store.read_info().expect("read"), before);
        assert_no_partial_file(dir.path());
    }

    #[test]
    fn checks_build_and_executable_before_importing_software() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = store_with_machines(dir.path());
        let catalog = store.read_info().expect("read").expect("catalog");

        assert_eq!(check_same_build(&catalog, &source("0.280")), Ok(()));
        let mismatch = check_same_build(&catalog, &source("0.281")).expect_err("release");
        assert_eq!(
            mismatch,
            ImportError::CatalogMismatch {
                catalog_release: "0.280".to_owned(),
                catalog_revision: "mame0280".to_owned(),
                executable_release: "0.281".to_owned(),
                executable_revision: "mame0281".to_owned(),
            }
        );
        // Same build, but the executable file was modified since the import.
        let mut modified = source("0.280");
        modified.executable_modified_at = Some(9999);
        assert!(check_same_build(&catalog, &modified).is_err());
    }

    #[test]
    fn failed_or_cancelled_software_import_keeps_previous_software_lists() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = store_with_machines(dir.path());
        let previous = import_software(&store, SOFTWARE_SAMPLE, source("0.280"), &not_cancelled())
            .expect("first software import");

        let cancelled = import_software(
            &store,
            SOFTWARE_SAMPLE,
            source("0.280"),
            &AtomicBool::new(true),
        );
        let truncated = import_software(
            &store,
            &SOFTWARE_SAMPLE[..SOFTWARE_SAMPLE.len() / 2],
            source("0.280"),
            &not_cancelled(),
        );
        let process_failed = import_software_from_reader(
            SOFTWARE_SAMPLE.as_bytes(),
            &store,
            source("0.280"),
            &not_cancelled(),
            &ImportProgress::default(),
            || {
                Err(ImportError::MameFailed {
                    exit_code: Some(1),
                    stderr: String::new(),
                })
            },
        );

        assert_eq!(cancelled.expect_err("cancelled"), ImportError::Cancelled);
        assert!(matches!(truncated, Err(ImportError::Parse { .. })));
        assert!(matches!(
            process_failed,
            Err(ImportError::MameFailed { .. })
        ));
        assert_eq!(store.read_info().expect("read"), Some(previous));
        assert_no_partial_file(dir.path());
    }

    #[test]
    fn software_reimport_replaces_previous_software_lists() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = store_with_machines(dir.path());
        import_software(&store, SOFTWARE_SAMPLE, source("0.280"), &not_cancelled())
            .expect("first software import");
        let smaller = concat!(
            "<softwarelists><softwarelist name=\"c64_cart\">",
            "<software name=\"solo\"><description>Solo</description></software>",
            "</softwarelist></softwarelists>"
        );

        let info =
            import_software(&store, smaller, source("0.280"), &not_cancelled()).expect("reimport");

        let software = info.software.expect("software");
        assert_eq!(
            (software.statistics.lists, software.statistics.software),
            (1, 1)
        );
        assert_eq!(software.statistics.roms, 0);
        assert_eq!(info.statistics.machines, 8);
        assert_eq!(
            count(&open(&store), "SELECT COUNT(*) FROM software_info"),
            0
        );
        assert_no_partial_file(dir.path());
    }

    #[test]
    fn machine_reimport_drops_software_lists_of_the_previous_build() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = store_with_machines(dir.path());
        import_software(&store, SOFTWARE_SAMPLE, source("0.280"), &not_cancelled())
            .expect("software import");

        let info = import(&store, SAMPLE, "0.281", &not_cancelled()).expect("machine reimport");

        assert_eq!(info.mame_release, "0.281");
        assert_eq!(info.software, None);
        assert_eq!(count(&open(&store), "SELECT COUNT(*) FROM software"), 0);
    }

    #[test]
    fn upgrades_a_version_1_catalog_and_imports_software_lists() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = store_with_machines(dir.path());
        // Turn the catalog into what version 1 of the application wrote.
        open(&store)
            .execute_batch(
                "DROP TABLE machines_fts; DROP TABLE software_fts; DROP INDEX machines_description;
                 DROP TABLE software_disks; DROP TABLE software_disk_areas;
                 DROP TABLE software_roms; DROP TABLE software_data_areas;
                 DROP TABLE software_part_features; DROP TABLE software_parts;
                 DROP TABLE software_shared_features; DROP TABLE software_info;
                 DROP TABLE software; DROP TABLE software_lists;
                 DROP TABLE software_catalog_metadata;
                 PRAGMA user_version = 1;",
            )
            .expect("downgrade");

        let upgraded = store.read_info().expect("read").expect("catalog");
        let info = import_software(&store, SOFTWARE_SAMPLE, source("0.280"), &not_cancelled())
            .expect("software import");

        assert_eq!(upgraded.statistics.machines, 8);
        assert_eq!(upgraded.software, None);
        assert_eq!(info.software.map(|s| s.statistics.software), Some(7));
        assert_eq!(
            count(&open(&store), "PRAGMA user_version"),
            crate::catalog::schema::SCHEMA_VERSION
        );
    }

    #[test]
    fn replacement_with_an_externally_open_catalog_keeps_it_on_windows() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = store_with_machines(dir.path());
        let before = store.read_info().expect("read");
        // Another program (outside the store's lock) keeps the catalog open.
        let external = open(&store);
        count(&external, "SELECT COUNT(*) FROM machines");

        let result = import_software(&store, SOFTWARE_SAMPLE, source("0.280"), &not_cancelled());

        if cfg!(windows) {
            // Windows refuses to rename over an open file: the import fails
            // with an explanation and the previous catalog is kept.
            let error = result.expect_err("catalog in use");
            assert!(
                matches!(&error, ImportError::Database { message }
                    if message.contains("Close any other program")),
                "{error:?}"
            );
            drop(external);
            assert_eq!(store.read_info().expect("read"), before);
        } else {
            // Unix replaces the directory entry; the open file stays valid.
            assert!(result.is_ok(), "{result:?}");
            assert_eq!(count(&external, "SELECT COUNT(*) FROM machines"), 8);
        }
        assert_no_partial_file(dir.path());
    }

    #[test]
    fn concurrent_status_reads_do_not_block_catalog_replacement() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = store_with_machines(dir.path());
        let stop = Arc::new(AtomicBool::new(false));
        let reader = {
            let store = store.clone();
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                let mut reads = 0;
                while !stop.load(Ordering::Relaxed) {
                    store
                        .read_info()
                        .expect("read during replacement")
                        .expect("catalog");
                    reads += 1;
                }
                reads
            })
        };

        for round in 0..5 {
            import(&store, SAMPLE, "0.280", &not_cancelled()).expect("machine import");
            import_software(&store, SOFTWARE_SAMPLE, source("0.280"), &not_cancelled())
                .unwrap_or_else(|error| panic!("software import {round}: {error:?}"));
        }
        stop.store(true, Ordering::Relaxed);

        assert!(reader.join().expect("reader thread") > 0);
        assert_no_partial_file(dir.path());
    }

    /// Imports the catalog and the software lists from a real MAME executable
    /// and prints measurements. Run with:
    /// `YMDB_TEST_MAME_EXECUTABLE=/path/to/mame cargo test --release
    /// real_mame_catalog_import -- --ignored --nocapture`
    #[test]
    #[ignore = "requires a local MAME executable in YMDB_TEST_MAME_EXECUTABLE"]
    fn real_mame_catalog_import() {
        let executable = std::env::var_os("YMDB_TEST_MAME_EXECUTABLE")
            .expect("YMDB_TEST_MAME_EXECUTABLE must be set");
        // YMDB_TEST_CATALOG_DIR keeps the catalog, for example for benchmarks.
        let temp = tempfile::tempdir().expect("temp dir");
        let dir = std::env::var_os("YMDB_TEST_CATALOG_DIR")
            .map_or_else(|| temp.path().to_path_buf(), std::path::PathBuf::from);
        let store = CatalogStore::in_dir(&dir);
        let cancel = Arc::new(AtomicBool::new(false));

        let started = Instant::now();
        let info = import_catalog(
            Path::new(&executable),
            &store,
            &cancel,
            &ImportProgress::default(),
        )
        .expect("machine import");
        println!("machine import took {:?}", started.elapsed());
        assert!(info.statistics.machines > 0);

        let started = Instant::now();
        let info = import_software_lists(
            Path::new(&executable),
            &store,
            &cancel,
            &ImportProgress::default(),
        )
        .expect("software list import");
        println!("software list import took {:?}", started.elapsed());
        println!("{info:#?}");
        assert!(info.software.is_some_and(|s| s.statistics.software > 0));
    }
}
