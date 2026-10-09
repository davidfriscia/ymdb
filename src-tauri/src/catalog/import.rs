//! Orchestration of a catalog import from `mame -listxml`.
//!
//! Steps: validate the configured executable (to record its version), start
//! `mame -listxml`, stream its output through the parser into a new catalog
//! database, check the process result, then commit and swap the catalog.
//! Any failure or cancellation before the swap discards the new database and
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
use super::store::{CatalogInfo, CatalogSource, CatalogStore, CompletedImport, StoreError};
use crate::mame::process::{self, ProcessError, StreamingOutcome, Termination};
use crate::mame::validation::{ValidationStatus, validate_executable};

/// `-listxml` is terminated if it produces no output for this long. The whole
/// run usually takes seconds; the limit only catches a hung process.
pub const LISTXML_INACTIVITY_TIMEOUT: Duration = Duration::from_secs(120);
/// Read buffer for the `-listxml` output.
const READ_BUFFER_SIZE: usize = 256 * 1024;
/// Maximum length of the MAME standard error included in error messages.
const MAX_STDERR_IN_ERROR: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportPhase {
    /// Running `mame -version` to identify the build.
    Validating,
    /// Reading `-listxml` and storing machines.
    Reading,
    /// Verifying, committing and replacing the catalog.
    Finalizing,
}

/// Progress shared between the import thread and status queries.
#[derive(Debug, Default)]
pub struct ImportProgress {
    phase: AtomicU8,
    machines: AtomicU64,
}

impl ImportProgress {
    pub fn phase(&self) -> ImportPhase {
        match self.phase.load(Ordering::Relaxed) {
            0 => ImportPhase::Validating,
            1 => ImportPhase::Reading,
            _ => ImportPhase::Finalizing,
        }
    }

    pub fn machines_processed(&self) -> u64 {
        self.machines.load(Ordering::Relaxed)
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
    /// `mame -listxml` could not be started or waited for.
    LaunchFailed { message: String },
    /// `mame -listxml` exited with an error.
    MameFailed {
        exit_code: Option<i32>,
        stderr: String,
    },
    /// `mame -listxml` stopped producing output and was terminated.
    Inactive { timeout_seconds: u64 },
    /// The output of `mame -listxml` could not be parsed.
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
    let mut command = Command::new(executable);
    command.arg("-listxml");
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
    let result = import_from_reader(stdout, store, source, cancel, progress, || {
        let outcome = process
            .take()
            .expect("process is finished once")
            .finish()
            .map_err(launch_error)?;
        check_outcome(outcome)
    });

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
        progress
            .machines
            .store(writer.machine_count(), Ordering::Relaxed);
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
        assert!(
            names.iter().all(|name| name == CATALOG_FILE_NAME),
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
        assert_eq!(progress.machines_processed(), 8);
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

    /// Imports the catalog from a real MAME executable and prints measurements.
    /// Run with: `YMDB_TEST_MAME_EXECUTABLE=/path/to/mame cargo test --release
    /// real_mame_catalog_import -- --ignored --nocapture`
    #[test]
    #[ignore = "requires a local MAME executable in YMDB_TEST_MAME_EXECUTABLE"]
    fn real_mame_catalog_import() {
        let executable = std::env::var_os("YMDB_TEST_MAME_EXECUTABLE")
            .expect("YMDB_TEST_MAME_EXECUTABLE must be set");
        let dir = tempfile::tempdir().expect("temp dir");
        let store = CatalogStore::in_dir(dir.path());
        let started = Instant::now();

        let info = import_catalog(
            Path::new(&executable),
            &store,
            &Arc::new(AtomicBool::new(false)),
            &ImportProgress::default(),
        )
        .expect("import");

        println!("import took {:?}", started.elapsed());
        println!("{info:#?}");
        assert!(info.statistics.machines > 0);
    }
}
