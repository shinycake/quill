//! The running account export as the session sees it: the worker thread,
//! its [`WorkerLink`] for the driver to pump, and the progress the UI
//! polls. Replaces the earlier raw JSONL account export.
use super::settings::AccountSettings;
use super::worker::{AccountJob, AccountProgress, AccountStage, create_folder, run};
use crate::export::handle::{SettleOnPanic, WorkerLink, publish};
use crate::export::worker::ExportError;
use crate::locks::LockExt;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Whether the running export's progress box is on screen. It lives with
/// the export, which the account's session holds, so switching to another
/// account and back leaves a hidden export hidden.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ExportView {
    /// The box is open (a new export starts here).
    #[default]
    Shown,
    /// "Hide": the export runs on, its result shows as a note.
    Hidden,
}

pub struct AccountExport {
    pub link: WorkerLink,
    progress: Arc<Mutex<AccountProgress>>,
    pub folder: PathBuf,
    pub view: ExportView,
}

impl AccountExport {
    /// Create `DataExport_<date>` in the chosen folder and start the worker.
    pub fn start(
        settings: AccountSettings,
        files_root: PathBuf,
        leftovers: Option<crate::export::leftovers::Leftovers>,
    ) -> io::Result<Self> {
        let parent = settings.dir.canonicalize()?;
        let now = crate::local_time::civil_local(crate::local_time::now_unix());
        let stamp = format!("{:04}-{:02}-{:02}", now.year, now.month, now.day);
        let folder = create_folder(&parent, &stamp)?;
        let (link, mut tdlib) = WorkerLink::pair(leftovers);
        let progress = Arc::new(Mutex::new(AccountProgress::starting()));
        let worker_progress = progress.clone();
        let worker_folder = folder.clone();
        crate::thread_guard::spawn_guarded("quill-export-account", move || {
            let _settle = SettleOnPanic::new(
                &worker_progress,
                AccountProgress::is_settled,
                |progress, why| progress.stage = AccountStage::Failed(why),
            );
            let report = |progress: AccountProgress| {
                publish(&worker_progress, &crate::wake::UPDATES, |slot| {
                    *slot = progress;
                });
            };
            let job = AccountJob {
                settings: &settings,
                files_root: &files_root,
                offset_at: &crate::local_time::utc_offset_at,
                report: &report,
            };
            let last = match run(&job, &mut tdlib, &worker_folder) {
                Ok(summary) => AccountStage::Finished(summary),
                Err(ExportError::Cancelled) => AccountStage::Cancelled,
                Err(err) => AccountStage::Failed(err.to_string()),
            };
            publish(&worker_progress, &crate::wake::UPDATES, |slot| {
                slot.stage = last;
            });
        })?;
        Ok(Self {
            link,
            progress,
            folder,
            view: ExportView::Shown,
        })
    }

    pub fn progress(&self) -> AccountProgress {
        self.progress.lock_or_recover().clone()
    }

    /// Ask the worker to stop. It does so at its next step; what was
    /// written stays.
    pub fn cancel(&self) {
        self.link.cancel();
    }

    #[cfg(test)]
    pub(crate) fn scripted() -> (Self, crate::export::handle::ScriptedWorker) {
        let (link, worker) = WorkerLink::pair(None);
        let export = Self {
            link,
            progress: Arc::new(Mutex::new(AccountProgress::starting())),
            folder: PathBuf::new(),
            view: ExportView::Shown,
        };
        (export, worker.into_scripted())
    }
}
