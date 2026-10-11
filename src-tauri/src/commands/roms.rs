//! ROM manager commands: download plans, downloads and Download & Play.
//!
//! A plan is prepared for a launch request (installation, machine, software,
//! mounted parts, profile) and kept in memory with the remote connections it
//! was built with: downloading a plan uses exactly that installation, those
//! paths and those connections, whatever the active installation becomes.
//! Downloads run on a dedicated thread, one plan at a time and one file at
//! a time; the frontend polls [`get_download_status`] (in memory, cheap).
//! Network operations only run on explicit user actions.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use super::installations::{InstallationsState, ResolvedInstallation};
use super::launcher::{LauncherState, launch_blocking, launch_config, prepare_resolved};
use super::remote::RemoteState;
use super::{CommandError, run_blocking};
use crate::installations::new_id;
use crate::launcher::LaunchRequest;
use crate::launcher::config::{LaunchConfig, ResolvedOption};
use crate::launcher::options::OptionValue;
use crate::launcher::session::{LaunchTarget, SessionInfo};
use crate::mame::ini::read_search_paths;
use crate::remote::connections::ConnectionsConfig;
use crate::remote::path::RemotePath;
use crate::remote::sources::{RemoteSourcesConfig, SourceCategory};
use crate::roms::download::{self, DownloadTask, Progress, TransferState, TransferStatus};
use crate::roms::local::SearchDirs;
use crate::roms::plan::{
    DownloadPlan, PlanArtifact, PlanWarning, WarningKind, build_artifacts, check_remote, finish,
};
use crate::roms::planner::{self, Organizations, PlanTarget, PlannerError, SoftwareTarget};
use crate::roms::{Connector, ProviderConnector};

/// Starts MAME once the downloads of Download & Play succeeded.
type LaunchHook = Box<dyn FnOnce(&LaunchRequest) -> LaunchOutcome + Send>;

/// Plans kept in memory (the most recent ones).
const KEPT_PLANS: usize = 16;

/// Managed state of the ROM manager.
#[derive(Default)]
pub struct RomManagerState {
    plans: Mutex<Vec<Arc<StoredPlan>>>,
    /// The running or last download; shared with its thread.
    job: Arc<Mutex<Option<Job>>>,
}

/// A plan and the connections it was built with.
struct StoredPlan {
    plan: DownloadPlan,
    connections: ConnectionsConfig,
}

struct Job {
    plan: Arc<StoredPlan>,
    progress: Arc<Progress>,
    cancel: Arc<AtomicBool>,
    play: bool,
    started_at: u64,
    finished_at: Option<u64>,
    launch: Option<LaunchOutcome>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum JobState {
    Running,
    /// Every planned file was downloaded (or was already there), and the
    /// plan had no warnings.
    Completed,
    /// Every planned file was downloaded; the plan has warnings (for
    /// example candidates not found remotely).
    CompletedWithWarnings,
    /// At least one transfer failed.
    Failed,
    Cancelled,
}

/// What Download & Play did after the downloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum LaunchOutcome {
    Started {
        session_id: u64,
    },
    /// The downloads failed or were cancelled: MAME was not started.
    NotStarted,
    Failed {
        error: CommandError,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadJobStatus {
    pub plan_id: String,
    pub installation_id: String,
    pub installation_label: String,
    pub target: LaunchTarget,
    pub title: String,
    pub play: bool,
    pub state: JobState,
    pub cancel_requested: bool,
    pub transfers: Vec<TransferState>,
    pub bytes_transferred: u64,
    /// Sum of the sizes, when every size is known.
    pub bytes_total: Option<u64>,
    pub started_at: u64,
    pub finished_at: Option<u64>,
    pub launch: Option<LaunchOutcome>,
}

impl Job {
    fn status(&self) -> DownloadJobStatus {
        let transfers = self.progress.snapshot();
        let plan = &self.plan.plan;
        let state = if self.finished_at.is_none() {
            JobState::Running
        } else {
            outcome(&transfers, plan)
        };
        DownloadJobStatus {
            plan_id: plan.id.clone(),
            installation_id: plan.installation_id.clone(),
            installation_label: plan.installation_label.clone(),
            target: plan.target.clone(),
            title: plan.title.clone(),
            play: self.play,
            state,
            cancel_requested: self.cancel.load(Ordering::Relaxed),
            bytes_transferred: transfers.iter().map(|t| t.bytes_transferred).sum(),
            bytes_total: transfers.iter().map(|t| t.size).sum(),
            transfers,
            started_at: self.started_at,
            finished_at: self.finished_at,
            launch: self.launch.clone(),
        }
    }
}

fn outcome(transfers: &[TransferState], plan: &DownloadPlan) -> JobState {
    let any = |status| transfers.iter().any(|t| t.status == status);
    if any(TransferStatus::Cancelled) {
        JobState::Cancelled
    } else if any(TransferStatus::Failed) {
        JobState::Failed
    } else if plan.summary.warnings > 0 {
        JobState::CompletedWithWarnings
    } else {
        JobState::Completed
    }
}

impl RomManagerState {
    fn store(&self, plan: DownloadPlan, connections: ConnectionsConfig) {
        let mut plans = self.plans.lock().unwrap_or_else(PoisonError::into_inner);
        plans.push(Arc::new(StoredPlan { plan, connections }));
        let excess = plans.len().saturating_sub(KEPT_PLANS);
        plans.drain(..excess);
    }

    fn plan(&self, plan_id: &str) -> Result<Arc<StoredPlan>, CommandError> {
        self.plans
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .find(|stored| stored.plan.id == plan_id)
            .cloned()
            .ok_or_else(|| CommandError::NotFound {
                message: "the download plan is no longer available; prepare it again".to_owned(),
            })
    }

    pub fn status(&self) -> Option<DownloadJobStatus> {
        self.job
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(Job::status)
    }

    fn cancel(&self) {
        if let Some(job) = self
            .job
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .filter(|job| job.finished_at.is_none())
        {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Starts downloading the files of a plan on a dedicated thread. With
    /// `launch`, MAME is started afterwards unless a transfer failed or the
    /// downloads were cancelled.
    fn start<C>(
        &self,
        stored: Arc<StoredPlan>,
        connector: C,
        launch: Option<LaunchHook>,
    ) -> Result<DownloadJobStatus, CommandError>
    where
        C: Connector + Send + 'static,
    {
        let tasks = tasks(&stored)?;
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = {
            let mut slot = self.job.lock().unwrap_or_else(PoisonError::into_inner);
            if slot.as_ref().is_some_and(|job| job.finished_at.is_none()) {
                return Err(CommandError::DownloadInProgress);
            }
            // A retry of the same plan keeps the files already completed.
            let previous = slot
                .as_ref()
                .filter(|job| job.plan.plan.id == stored.plan.id)
                .map(|job| job.progress.snapshot())
                .unwrap_or_default();
            let progress = Arc::new(Progress::new(&tasks, &previous));
            *slot = Some(Job {
                plan: Arc::clone(&stored),
                progress: Arc::clone(&progress),
                cancel: Arc::clone(&cancel),
                play: launch.is_some(),
                started_at: now_millis(),
                finished_at: None,
                launch: None,
            });
            progress
        };

        let job = Arc::clone(&self.job);
        let thread_progress = Arc::clone(&progress);
        let request = stored.plan.request.clone();
        let spawned = thread::Builder::new()
            .name("rom-download".to_owned())
            .spawn(move || {
                match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime.block_on(download::run(
                        &tasks,
                        &connector,
                        &thread_progress,
                        &cancel,
                    )),
                    Err(error) => fail_all(&thread_progress, &error.to_string()),
                }
                let succeeded = !thread_progress.snapshot().iter().any(|transfer| {
                    matches!(
                        transfer.status,
                        TransferStatus::Failed | TransferStatus::Cancelled
                    )
                });
                let launched = launch.map(|launch| {
                    if succeeded {
                        launch(&request)
                    } else {
                        LaunchOutcome::NotStarted
                    }
                });
                finish_job(&job, launched);
            });
        if let Err(error) = spawned {
            fail_all(
                &progress,
                &format!("failed to start the download thread: {error}"),
            );
            finish_job(&self.job, None);
        }
        Ok(self.status().expect("the job was just recorded"))
    }
}

fn fail_all(progress: &Progress, message: &str) {
    progress.fail_unfinished(&download::TransferError::Local {
        message: message.to_owned(),
    });
}

fn finish_job(job: &Mutex<Option<Job>>, launch: Option<LaunchOutcome>) {
    if let Some(job) = job.lock().unwrap_or_else(PoisonError::into_inner).as_mut() {
        job.finished_at = Some(now_millis());
        job.launch = launch;
    }
}

/// The files a plan downloads, with the connections it was built with.
fn tasks(stored: &StoredPlan) -> Result<Vec<DownloadTask>, CommandError> {
    let tasks: Vec<DownloadTask> = stored
        .plan
        .artifacts
        .iter()
        .filter(|artifact| artifact.download)
        .filter_map(|artifact| task(artifact, &stored.connections))
        .collect();
    if tasks.is_empty() {
        return Err(CommandError::InvalidRequest {
            message: "the plan has no file to download".to_owned(),
        });
    }
    Ok(tasks)
}

fn task(artifact: &PlanArtifact, connections: &ConnectionsConfig) -> Option<DownloadTask> {
    Some(DownloadTask {
        artifact_id: artifact.id.clone(),
        connection: connections.get(artifact.connection_id.as_deref()?)?.clone(),
        remote_path: RemotePath::parse(artifact.remote_path.as_deref()?).ok()?,
        destination: artifact.destination.clone()?,
        expected_size: artifact.remote_size,
    })
}

/// What the local part of a plan needs from the launch preparation.
struct LocalPlan {
    target: LaunchTarget,
    title: String,
    artifacts: Vec<PlanArtifact>,
    warnings: Vec<PlanWarning>,
}

/// Validates the request as a launch would, lists the candidate files and
/// checks them locally. Blocking.
fn local_plan(
    resolved: &ResolvedInstallation,
    config: &LaunchConfig,
    request: &LaunchRequest,
    connections: &ConnectionsConfig,
) -> Result<LocalPlan, CommandError> {
    let prepared = prepare_resolved(resolved, config, request)?;
    let mut warnings = Vec::new();
    let sources = resolved.remote_sources.load().unwrap_or_else(|error| {
        warnings.push(PlanWarning {
            kind: WarningKind::Configuration,
            message: format!("The ROM Manager settings cannot be read: {error}"),
        });
        RemoteSourcesConfig::default()
    });
    let mame_dir = resolved
        .installation
        .executable_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let search = effective_search_dirs(&mame_dir, &prepared.options);
    let organization = |category| {
        sources
            .categories
            .get(&category)
            .filter(|source| source.enabled)
            .and_then(|source| source.organization)
    };
    let organizations = Organizations {
        machine_chds: organization(SourceCategory::MachineChds),
        samples: organization(SourceCategory::Samples),
    };
    let parts: Vec<String> = request
        .software
        .iter()
        .flat_map(|software| software.media.iter().map(|media| media.part.clone()))
        .collect();
    let target = PlanTarget {
        machine: &request.machine,
        software: request.software.as_ref().map(|software| SoftwareTarget {
            list: &software.list,
            name: &software.name,
            parts: &parts,
        }),
    };
    let candidates = resolved
        .catalog
        .with_reader(|connection| Ok(planner::plan(connection, &target, organizations)))?
        .ok_or(CommandError::CatalogNotImported)?
        .map_err(|error| match error {
            PlannerError::NotFound(message) => CommandError::NotFound { message },
            PlannerError::Database(error) => CommandError::Internal {
                message: error.to_string(),
            },
        })?;
    let artifacts = build_artifacts(candidates, &mame_dir, &search, &sources, connections);
    Ok(LocalPlan {
        target: prepared.target,
        title: prepared.title,
        artifacts,
        warnings,
    })
}

/// The directories MAME searches for the launch: the resolved `rompath` and
/// `samplepath` of the plan (which already include the ROM manager
/// destinations), else MAME's own values from `mame.ini`.
fn effective_search_dirs(mame_dir: &Path, options: &[ResolvedOption]) -> SearchDirs {
    let native = read_search_paths(mame_dir);
    let value = |name: &str, native: Vec<String>| match options
        .iter()
        .find(|option| option.name == name)
        .map(|option| &option.value)
    {
        Some(OptionValue::List(entries)) => entries.clone(),
        _ => native,
    };
    SearchDirs::new(
        mame_dir,
        &value("rompath", native.rompath),
        &value("samplepath", native.samplepath),
    )
}

fn connector(
    remote: &RemoteState,
    passphrases: Option<HashMap<String, String>>,
) -> ProviderConnector {
    ProviderConnector {
        defaults: remote.connect_options(None),
        per_connection: passphrases
            .unwrap_or_default()
            .into_iter()
            .map(|(id, passphrase)| (id, remote.connect_options(Some(passphrase))))
            .collect(),
    }
}

/// Builds the download plan of a launch: candidate files, local presence and
/// remote availability. Nothing is downloaded. Passphrases (by connection
/// id) are used for this lookup only.
#[tauri::command]
pub async fn prepare_download_plan(
    request: LaunchRequest,
    passphrases: Option<HashMap<String, String>>,
    app: AppHandle,
) -> Result<DownloadPlan, CommandError> {
    let installations = app.state::<InstallationsState>();
    let launcher = app.state::<LauncherState>();
    let remote = app.state::<RemoteState>();
    let resolved = installations.resolve(&request.installation_id)?;
    let config = launch_config(&launcher, &resolved)?;
    let installation_label = resolved.installation.label();
    let mut warnings = Vec::new();
    let connections = remote.load().unwrap_or_else(|error| {
        let message = match error {
            CommandError::Settings { message } => message,
            other => format!("{other:?}"),
        };
        warnings.push(PlanWarning {
            kind: WarningKind::Configuration,
            message: format!("The remote connections cannot be read: {message}"),
        });
        ConnectionsConfig::default()
    });

    let (local, request, connections) = run_blocking(move || {
        local_plan(&resolved, &config, &request, &connections)
            .map(|local| (local, request, connections))
    })
    .await??;
    let LocalPlan {
        target,
        title,
        mut artifacts,
        warnings: local_warnings,
    } = local;
    warnings.extend(local_warnings);

    let connector = connector(&remote, passphrases);
    let connection_issues = check_remote(&mut artifacts, &connections, &connector).await;
    for issue in &connection_issues {
        remote.remember_pending(&issue.connection_id, &issue.error);
    }
    let summary = finish(&mut artifacts, &warnings);
    let plan = DownloadPlan {
        id: new_id(),
        installation_id: request.installation_id.clone(),
        installation_label,
        request,
        target,
        title,
        created_at: now_millis(),
        artifacts,
        warnings,
        connection_issues,
        summary,
    };
    app.state::<RomManagerState>()
        .store(plan.clone(), connections);
    Ok(plan)
}

/// Downloads the available missing files of a plan (again for a retry: the
/// files already completed are kept). With `play`, MAME is started with the
/// plan's launch request once every transfer succeeded.
#[tauri::command]
pub fn start_download(
    plan_id: String,
    passphrases: Option<HashMap<String, String>>,
    play: bool,
    app: AppHandle,
    roms: State<'_, RomManagerState>,
    remote: State<'_, RemoteState>,
    installations: State<'_, InstallationsState>,
) -> Result<DownloadJobStatus, CommandError> {
    let stored = roms.plan(&plan_id)?;
    // The installation must still exist, and the connections must be those
    // the plan was built with.
    installations.resolve(&stored.plan.installation_id)?;
    let current = remote.load()?;
    for artifact in stored
        .plan
        .artifacts
        .iter()
        .filter(|artifact| artifact.download)
    {
        let id = artifact.connection_id.as_deref().unwrap_or_default();
        if current.get(id) != stored.connections.get(id) {
            return Err(CommandError::InvalidRequest {
                message: "a remote connection changed since the plan was prepared; prepare the download again".to_owned(),
            });
        }
    }
    let launch: Option<LaunchHook> = play.then(|| {
        let app = app.clone();
        Box::new(move |request: &LaunchRequest| {
            match launch_blocking(
                &app.state::<InstallationsState>(),
                &app.state::<LauncherState>(),
                request,
            ) {
                Ok(SessionInfo { id, .. }) => LaunchOutcome::Started { session_id: id },
                Err(error) => LaunchOutcome::Failed { error },
            }
        }) as LaunchHook
    });
    roms.start(stored, connector(&remote, passphrases), launch)
}

/// The running or last download, `null` when none ran since startup.
/// Cheap: suitable for polling.
#[tauri::command]
pub fn get_download_status(roms: State<'_, RomManagerState>) -> Option<DownloadJobStatus> {
    roms.status()
}

/// Requests cancellation of the running download: the current transfer
/// stops, its temporary file is removed and completed files are kept.
#[tauri::command]
pub fn cancel_download(roms: State<'_, RomManagerState>) -> Option<DownloadJobStatus> {
    roms.cancel();
    roms.status()
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
