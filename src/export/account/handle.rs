//! The running account export as the session sees it: the worker thread,
//! its [`WorkerLink`] for the driver to pump, and the progress the UI
//! polls. Replaces the earlier raw JSONL account export.
use super::settings::AccountSettings;
use super::worker::{AccountJob, AccountProgress, AccountStage, create_folder, run};
use crate::export::handle::WorkerLink;
use crate::export::worker::ExportError;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub struct AccountExport {
    pub link: WorkerLink,
    progress: Arc<Mutex<AccountProgress>>,
    /// Set once the worker has stopped, however it ended.
    pub finished: Arc<AtomicBool>,
    pub folder: PathBuf,
}

impl AccountExport {
    /// Create `DataExport_<date>` in the chosen folder and start the worker.
    pub fn start(settings: AccountSettings, files_root: PathBuf) -> io::Result<Self> {
        let parent = settings.dir.canonicalize()?;
        let now = crate::local_time::civil_local(crate::local_time::now_unix());
        let stamp = format!("{:04}-{:02}-{:02}", now.year, now.month, now.day);
        let folder = create_folder(&parent, &stamp)?;
        let (link, mut tdlib) = WorkerLink::pair();
        let progress = Arc::new(Mutex::new(AccountProgress::starting()));
        let finished = Arc::new(AtomicBool::new(false));
        let worker_progress = progress.clone();
        let worker_finished = finished.clone();
        let worker_folder = folder.clone();
        std::thread::spawn(move || {
            let report = |progress: AccountProgress| {
                if let Ok(mut slot) = worker_progress.lock() {
                    *slot = progress;
                }
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
            if let Ok(mut slot) = worker_progress.lock() {
                slot.stage = last;
            }
            worker_finished.store(true, Ordering::Release);
        });
        Ok(Self {
            link,
            progress,
            finished,
            folder,
        })
    }

    pub fn progress(&self) -> AccountProgress {
        self.progress
            .lock()
            .map(|p| p.clone())
            .unwrap_or_else(|_| AccountProgress::starting())
    }

    /// Ask the worker to stop. It does so at its next step; what was
    /// written stays.
    pub fn cancel(&self) {
        self.link.cancel();
    }
}
