//! A running export as the session sees it: a worker thread plus the two
//! channels the connect driver pumps (requests out to TDLib, answers back).
//! [`WorkerLink`] is the session's end, shared by the chat export here and
//! the account export (`account/handle.rs`).
use super::leftovers::Leftovers;
use super::loaded_files::LoadedFiles;
use super::settings::ExportSettings;
use super::worker::{ExportError, Job, Progress, Stage, Tdlib, create_folder, run};
use crate::ids::RequestId;
use crate::locks::LockExt;
use crate::wake::Signal;
use serde_json::Value;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

/// Which call of the worker a request or an answer belongs to. Calls count
/// up from 1; [`UNANSWERED`] marks a request nobody waits on.
type CallId = u64;

const UNANSWERED: CallId = 0;

/// The worker's end: blocks on channels the driver services from the poll
/// loop. Every request goes out with the number of its call and every answer
/// comes back with it, so a call never takes the answer to another one (a
/// call that gave up, or a request nobody waited on).
pub(crate) struct ChannelTdlib {
    requests: mpsc::Sender<(CallId, Value)>,
    replies: mpsc::Receiver<(CallId, Value)>,
    cancelled: Arc<AtomicBool>,
    last_call: CallId,
    leftovers: Option<Leftovers>,
    /// Dropped with the worker, which tells the link it ended.
    _alive: mpsc::Sender<()>,
}

impl ChannelTdlib {
    #[cfg(test)]
    pub(crate) fn into_scripted(self) -> ScriptedWorker {
        ScriptedWorker {
            requests: self.requests,
            replies: self.replies,
            last_call: std::cell::Cell::new(self.last_call),
            _alive: self._alive,
        }
    }
}

/// A test's stand-in for the worker thread: it sends the requests, and the
/// worker counts as running until it is dropped.
#[cfg(test)]
pub(crate) struct ScriptedWorker {
    requests: mpsc::Sender<(CallId, Value)>,
    replies: mpsc::Receiver<(CallId, Value)>,
    last_call: std::cell::Cell<CallId>,
    _alive: mpsc::Sender<()>,
}

#[cfg(test)]
impl ScriptedWorker {
    /// A new call: the request goes out under the next call number.
    pub(crate) fn send(&self, request: Value) -> Result<(), mpsc::SendError<(CallId, Value)>> {
        let call = self.last_call.get() + 1;
        self.last_call.set(call);
        self.requests.send((call, request))
    }

    /// The call number the last `send` used.
    pub(crate) fn last_call(&self) -> CallId {
        self.last_call.get()
    }

    /// An answer the link handed the worker, if one is waiting, with the
    /// call it answers.
    pub(crate) fn answer(&self) -> Option<(CallId, Value)> {
        self.replies.try_recv().ok()
    }
}

impl Tdlib for ChannelTdlib {
    fn call(&mut self, request: Value, timeout: Duration) -> Result<Value, ExportError> {
        let cancelled = || self.cancelled.load(Ordering::Acquire);
        if cancelled() {
            return Err(ExportError::Cancelled);
        }
        self.last_call += 1;
        let call = self.last_call;
        self.requests
            .send((call, request))
            .map_err(|_| ExportError::Cancelled)?;
        // The poll loop forwards the request; do not make it wait out a sleep.
        crate::wake::UPDATES.poke();
        let started = Instant::now();
        loop {
            if cancelled() {
                return Err(ExportError::Cancelled);
            }
            if started.elapsed() > timeout {
                return Err(ExportError::Tdlib("the request timed out".into()));
            }
            match self.replies.recv_timeout(Duration::from_millis(100)) {
                Ok((answered, mut value)) if answered == call => {
                    if let Some(object) = value.as_object_mut() {
                        object.remove("@extra");
                        object.remove("@client_id");
                    }
                    return Ok(value);
                }
                // The answer to a call that gave up, or to a request nobody
                // waited on: it is not this call's.
                Ok(_) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err(ExportError::Cancelled),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }

    fn post_unanswered(&mut self, request: Value) {
        if self.requests.send((UNANSWERED, request)).is_ok() {
            crate::wake::UPDATES.poke();
        }
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    fn leftovers(&self) -> Option<&Leftovers> {
        self.leftovers.as_ref()
    }
}

/// Which export a [`WorkerLink`] serves, unique for the process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExportId(u64);

/// The session's end of a worker: its next request, the one the driver
/// sent and awaits, and the cancel flag.
pub struct WorkerLink {
    id: ExportId,
    requests: mpsc::Receiver<(CallId, Value)>,
    replies: mpsc::Sender<(CallId, Value)>,
    /// The request the driver sent and awaits an answer for.
    pub pending: Option<RequestId>,
    /// The worker call of the request handed out last: its answer goes
    /// back under that number.
    asked: CallId,
    cancelled: Arc<AtomicBool>,
    /// Disconnected once the worker ended.
    ended: mpsc::Receiver<()>,
}

impl WorkerLink {
    /// A connected pair: the session's end and the worker's.
    pub(crate) fn pair(leftovers: Option<Leftovers>) -> (Self, ChannelTdlib) {
        let (request_tx, requests) = mpsc::channel();
        let (replies, reply_rx) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let (alive, ended) = mpsc::channel();
        let worker = ChannelTdlib {
            requests: request_tx,
            replies: reply_rx,
            cancelled: cancelled.clone(),
            last_call: UNANSWERED,
            leftovers,
            _alive: alive,
        };
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let link = Self {
            id: ExportId(NEXT.fetch_add(1, Ordering::Relaxed)),
            requests,
            replies,
            pending: None,
            asked: UNANSWERED,
            cancelled,
            ended,
        };
        (link, worker)
    }

    pub fn id(&self) -> ExportId {
        self.id
    }

    /// Neither stopped nor ended.
    pub fn running(&self) -> bool {
        !self.is_cancelled() && !self.worker_ended()
    }

    pub fn idle_or_cancelled(&self) -> bool {
        self.pending.is_none() || self.is_cancelled()
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    /// The next request for TDLib, if the worker has one waiting.
    pub fn next_request(&mut self) -> Option<Value> {
        let (call, request) = self.requests.try_recv().ok()?;
        self.asked = call;
        Some(request)
    }

    /// Answer the worker's request at once: the driver takes it from here.
    pub fn answer_ok(&self) {
        let _ = self
            .replies
            .send((self.asked, serde_json::json!({"@type": "ok"})));
    }

    /// Hand TDLib's answer to the worker.
    pub fn reply(&mut self, id: RequestId, value: Value) {
        if self.pending == Some(id) {
            self.pending = None;
            let _ = self.replies.send((self.asked, value));
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

    pub fn worker_ended(&self) -> bool {
        matches!(self.ended.try_recv(), Err(mpsc::TryRecvError::Disconnected))
    }

    /// Block until the worker ended or `until` passed. Returns whether it
    /// ended.
    pub fn wait_ended(&self, until: Instant) -> bool {
        let left = until.saturating_duration_since(Instant::now());
        matches!(
            self.ended.recv_timeout(left),
            Err(mpsc::RecvTimeoutError::Disconnected)
        )
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

/// Settles an export whose worker thread panicked: the thread body holds
/// one, and when it unwinds before the final stage was published, this
/// publishes `Failed`, so the progress box gets its Done button, a new
/// export is not refused as "already running", and Quit stops asking.
pub(crate) struct SettleOnPanic<P> {
    slot: Arc<Mutex<P>>,
    is_settled: fn(&P) -> bool,
    fail: fn(&mut P, String),
}

impl<P> SettleOnPanic<P> {
    pub(crate) fn new(
        slot: &Arc<Mutex<P>>,
        is_settled: fn(&P) -> bool,
        fail: fn(&mut P, String),
    ) -> Self {
        Self {
            slot: slot.clone(),
            is_settled,
            fail,
        }
    }
}

impl<P> Drop for SettleOnPanic<P> {
    fn drop(&mut self) {
        if (self.is_settled)(&self.slot.lock_or_recover()) {
            return;
        }
        let fail = self.fail;
        publish(&self.slot, &crate::wake::UPDATES, |slot| {
            fail(slot, "The export stopped unexpectedly.".into())
        });
    }
}

pub struct ChatExport {
    pub link: WorkerLink,
    progress: Arc<Mutex<Progress>>,
    pub folder: PathBuf,
    pub title: String,
}

impl ChatExport {
    /// Create the output folder and start the worker thread.
    pub fn start(
        settings: ExportSettings,
        files_root: PathBuf,
        leftovers: Option<Leftovers>,
    ) -> io::Result<Self> {
        let parent = settings.dir.canonicalize()?;
        let now = crate::local_time::civil_local(crate::local_time::now_unix());
        let stamp = format!("{:04}-{:02}-{:02}", now.year, now.month, now.day);
        let folder = create_folder(&parent, &settings.title, &stamp)?;
        let (link, mut tdlib) = WorkerLink::pair(leftovers);
        let progress = Arc::new(Mutex::new(Progress::starting()));
        let title = settings.title.clone();
        let worker_progress = progress.clone();
        let worker_folder = folder.clone();
        crate::thread_guard::spawn_guarded("quill-export", move || {
            let _settle =
                SettleOnPanic::new(&worker_progress, Progress::is_settled, |progress, why| {
                    progress.stage = Stage::Failed(why)
                });
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
    pub(crate) fn scripted() -> (Self, ScriptedWorker) {
        let (link, worker) = WorkerLink::pair(None);
        let export = Self {
            link,
            progress: Arc::new(Mutex::new(Progress::starting())),
            folder: PathBuf::new(),
            title: String::new(),
        };
        (export, worker.into_scripted())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_goes_back_under_the_call_it_answers() {
        let (mut export, worker) = ChatExport::scripted();
        worker
            .send(serde_json::json!({"@type": "getChat"}))
            .unwrap();
        assert!(export.link.next_request().is_some());
        export.link.pending = Some(RequestId(1));
        export
            .link
            .reply(RequestId(1), serde_json::json!({"@type": "chat"}));
        let (call, _) = worker.answer().expect("answered");
        assert_eq!(call, worker.last_call());
        assert!(export.link.pending.is_none());

        export.link.pending = Some(RequestId(2));
        export
            .link
            .reply(RequestId(9), serde_json::json!({"@type": "chat"}));
        assert!(
            worker.answer().is_none(),
            "another request's answer is ignored"
        );
    }

    #[test]
    fn a_call_ignores_the_late_answer_to_an_earlier_one() {
        let (link, mut tdlib) = WorkerLink::pair(None);
        // Call 1 gave up waiting; its answer is still on the way.
        tdlib.last_call = 1;
        link.replies
            .send((1, serde_json::json!({"@type": "messages", "stale": true})))
            .unwrap();
        link.replies
            .send((2, serde_json::json!({"@type": "messages"})))
            .unwrap();
        let answer = tdlib
            .call(
                serde_json::json!({"@type": "getChatHistory"}),
                Duration::from_secs(3600),
            )
            .expect("the real answer");
        assert_eq!(answer, serde_json::json!({"@type": "messages"}));
    }

    #[test]
    fn a_request_nobody_waits_on_gets_no_call_a_later_one_could_take() {
        let (mut link, mut tdlib) = WorkerLink::pair(None);
        tdlib.post_unanswered(serde_json::json!({"@type": "deleteFile"}));
        assert!(link.next_request().is_some());
        link.pending = Some(RequestId(1));
        link.reply(RequestId(1), serde_json::json!({"@type": "ok"}));
        link.replies
            .send((1, serde_json::json!({"@type": "messages"})))
            .unwrap();
        let answer = tdlib
            .call(
                serde_json::json!({"@type": "getChatHistory"}),
                Duration::from_secs(3600),
            )
            .expect("the call's own answer");
        assert_eq!(answer, serde_json::json!({"@type": "messages"}));
    }
}
