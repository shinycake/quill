//! A running export as the session sees it: a worker thread plus the two
//! channels the connect driver pumps (requests out to TDLib, answers back).
//! [`WorkerLink`] is the session's end, shared by the chat export here and
//! the account export (`account/handle.rs`).
use super::loaded_files::LoadedFiles;
use super::settings::ExportSettings;
use super::worker::{ExportError, Job, Progress, Stage, Tdlib, create_folder, run};
use crate::ids::RequestId;
use crate::json_access::JsonExt;
use crate::locks::LockExt;
use crate::wake::Signal;
use serde_json::Value;
use std::collections::HashSet;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

/// The worker's end: blocks on channels the driver services from the poll
/// loop.
pub(crate) struct ChannelTdlib {
    requests: mpsc::Sender<Value>,
    replies: mpsc::Receiver<Value>,
    cancelled: Arc<AtomicBool>,
}

impl ChannelTdlib {
    #[cfg(test)]
    pub(crate) fn into_requests(self) -> mpsc::Sender<Value> {
        self.requests
    }
}

impl Tdlib for ChannelTdlib {
    fn call(&mut self, request: Value, timeout: Duration) -> Result<Value, ExportError> {
        let cancelled = || self.cancelled.load(Ordering::Acquire);
        if cancelled() {
            return Err(ExportError::Cancelled);
        }
        self.requests
            .send(request)
            .map_err(|_| ExportError::Cancelled)?;
        // The poll loop forwards the request; do not make it wait out a sleep.
        crate::wake::UPDATES.poke();
        let started = Instant::now();
        loop {
            if cancelled() {
                return Err(ExportError::Cancelled);
            }
            match self.replies.recv_timeout(Duration::from_millis(100)) {
                Ok(mut value) => {
                    if let Some(object) = value.as_object_mut() {
                        object.remove("@extra");
                        object.remove("@client_id");
                    }
                    return Ok(value);
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err(ExportError::Cancelled),
                Err(mpsc::RecvTimeoutError::Timeout) if started.elapsed() > timeout => {
                    return Err(ExportError::Tdlib("the request timed out".into()));
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }

    fn post_unanswered(&mut self, request: Value) {
        if self.requests.send(request).is_ok() {
            crate::wake::UPDATES.poke();
        }
    }
}

/// The session's end of a worker: its next request, the one the driver
/// sent and awaits, and the cancel flag.
pub struct WorkerLink {
    requests: mpsc::Receiver<Value>,
    replies: mpsc::Sender<Value>,
    /// The request the driver sent and awaits an answer for.
    pub pending: Option<RequestId>,
    cancelled: Arc<AtomicBool>,
    /// Files the view asked for while the export ran. The export leaves
    /// them in TDLib's cache even when it downloaded them first.
    view_files: HashSet<i32>,
}

impl WorkerLink {
    /// A connected pair: the session's end and the worker's.
    pub(crate) fn pair() -> (Self, ChannelTdlib) {
        let (request_tx, requests) = mpsc::channel();
        let (replies, reply_rx) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker = ChannelTdlib {
            requests: request_tx,
            replies: reply_rx,
            cancelled: cancelled.clone(),
        };
        let link = Self {
            requests,
            replies,
            pending: None,
            cancelled,
            view_files: HashSet::new(),
        };
        (link, worker)
    }

    pub fn idle_or_cancelled(&self) -> bool {
        self.pending.is_none() || self.cancelled.load(Ordering::Acquire)
    }

    /// The next request for TDLib, if the worker has one waiting.
    pub fn next_request(&self) -> Option<Value> {
        self.requests.try_recv().ok()
    }

    pub fn note_view_file(&mut self, file_id: i32) {
        self.view_files.insert(file_id);
    }

    /// Answer the worker's `deleteFile` for a file the view asked for
    /// without sending it. Returns whether `request` was one.
    pub fn keep_view_file(&mut self, request: &Value) -> bool {
        let kept = request.at("@type") == "deleteFile"
            && request
                .at("file_id")
                .as_i64()
                .and_then(|id| i32::try_from(id).ok())
                .is_some_and(|id| self.view_files.contains(&id));
        if kept {
            let _ = self.replies.send(serde_json::json!({"@type": "ok"}));
        }
        kept
    }

    /// Hand TDLib's answer to the worker.
    pub fn reply(&mut self, id: RequestId, value: Value) {
        if self.pending == Some(id) {
            self.pending = None;
            let _ = self.replies.send(value);
        }
    }

    /// The request could not be sent: tell the worker so it fails cleanly.
    pub fn fail_send(&mut self, id: RequestId) {
        self.reply(
            id,
            serde_json::json!({"@type": "error", "code": 0, "message": "could not send"}),
        );
    }

    /// Ask the worker to stop. It does so at its next step.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

impl Drop for WorkerLink {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Change the slot a poll pass reads, and wake that pass: stretches of the
/// Writing stage send no request, so nothing else would.
pub(crate) fn publish<P>(slot: &Mutex<P>, wake: &Signal, change: impl FnOnce(&mut P)) {
    change(&mut slot.lock_or_recover());
    wake.poke();
}

pub struct ChatExport {
    pub link: WorkerLink,
    progress: Arc<Mutex<Progress>>,
    pub folder: PathBuf,
    pub title: String,
}

impl ChatExport {
    /// Create the output folder and start the worker thread.
    pub fn start(settings: ExportSettings, files_root: PathBuf) -> io::Result<Self> {
        let parent = settings.dir.canonicalize()?;
        let now = crate::local_time::civil_local(crate::local_time::now_unix());
        let stamp = format!("{:04}-{:02}-{:02}", now.year, now.month, now.day);
        let folder = create_folder(&parent, &settings.title, &stamp)?;
        let (link, mut tdlib) = WorkerLink::pair();
        let progress = Arc::new(Mutex::new(Progress::starting()));
        let title = settings.title.clone();
        let worker_progress = progress.clone();
        let worker_folder = folder.clone();
        crate::thread_guard::spawn_guarded("quill-export", move || {
            let report = |progress: Progress| {
                publish(&worker_progress, &crate::wake::UPDATES, |slot| {
                    *slot = progress;
                });
            };
            let loaded = LoadedFiles::default();
            let job = Job {
                settings: &settings,
                files_root: &files_root,
                offset_at: &crate::local_time::utc_offset_at,
                report: &report,
                me: None,
                loaded: &loaded,
            };
            let last = match run(&job, &mut tdlib, &worker_folder) {
                Ok(summary) => Stage::Finished(summary),
                Err(ExportError::Cancelled) => Stage::Cancelled,
                Err(err) => Stage::Failed(err.to_string()),
            };
            publish(&worker_progress, &crate::wake::UPDATES, |slot| {
                slot.stage = last;
            });
        })?;
        Ok(Self {
            link,
            progress,
            folder,
            title,
        })
    }

    pub fn progress(&self) -> Progress {
        self.progress.lock_or_recover().clone()
    }

    /// Ask the worker to stop. It does so at its next step.
    pub fn cancel(&self) {
        self.link.cancel();
    }

    #[cfg(test)]
    pub(crate) fn scripted() -> (Self, mpsc::Sender<Value>) {
        let (link, worker) = WorkerLink::pair();
        let export = Self {
            link,
            progress: Arc::new(Mutex::new(Progress::starting())),
            folder: PathBuf::new(),
            title: String::new(),
        };
        (export, worker.into_requests())
    }
}
