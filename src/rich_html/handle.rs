//! A running "Save as HTML" as the session sees it: the worker thread, the
//! channel pair the driver pumps (the chat export's [`WorkerLink`]) and how
//! it ended.
use super::page::PageLabels;
use super::worker::run;
use crate::export::handle::WorkerLink;
use crate::export::worker::ExportError;
use crate::ids::{ChatId, MessageId};
use crate::locks::LockExt;
use std::io;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

/// What to save and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RichHtmlRequest {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    /// The folder the message's own folder is made in (the download folder,
    /// or the one the user picked).
    pub base_dir: PathBuf,
    /// The title when the page has no heading or paragraph (tdesktop uses
    /// the chat's name).
    pub fallback_title: String,
    pub labels: PageLabels,
}

/// How an export ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RichHtmlOutcome {
    /// The page was written here.
    Saved(PathBuf),
    Failed,
    /// Stopped by logout or quit; nothing is reported.
    Cancelled,
}

/// Where an export stands: the worker is running and will send how it ended
/// on `done`, or the outcome is kept. A worker that dies drops its sender
/// without sending, which reads as `Failed`.
#[derive(Debug)]
enum RichHtmlStage {
    Running { done: Receiver<RichHtmlOutcome> },
    Done(RichHtmlOutcome),
}

impl RichHtmlStage {
    /// How the worker ended, once it has; the outcome is kept after the
    /// first read.
    fn outcome(&mut self) -> Option<RichHtmlOutcome> {
        let ended = match self {
            Self::Done(outcome) => return Some(outcome.clone()),
            Self::Running { done } => match done.try_recv() {
                Ok(outcome) => outcome,
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Disconnected) => RichHtmlOutcome::Failed,
            },
        };
        *self = Self::Done(ended.clone());
        Some(ended)
    }
}

pub struct RichHtmlExport {
    pub link: WorkerLink,
    pub request: RichHtmlRequest,
    stage: Mutex<RichHtmlStage>,
}

impl RichHtmlExport {
    /// Start the worker thread. `files_root` is TDLib's files directory;
    /// only files under it are copied.
    pub fn start(
        request: RichHtmlRequest,
        files_root: PathBuf,
        leftovers: Option<crate::export::leftovers::Leftovers>,
    ) -> io::Result<Self> {
        let (link, mut tdlib) = WorkerLink::pair(leftovers);
        let (sender, done) = channel();
        let worker_request = request.clone();
        crate::thread_guard::spawn_guarded("quill-rich-html", move || {
            let slot = OutcomeSlot(Some(sender));
            let date = |unix: i64| {
                let time = crate::local_time::civil_local(unix);
                format!(
                    "{} {} {}",
                    time.day,
                    crate::local_time::month_name(time.month),
                    time.year
                )
            };
            let last = match run(&worker_request, &mut tdlib, &files_root, &date) {
                Ok(html) => RichHtmlOutcome::Saved(html),
                Err(ExportError::Cancelled) => RichHtmlOutcome::Cancelled,
                Err(_) => RichHtmlOutcome::Failed,
            };
            slot.set(last);
        })?;
        Ok(Self {
            link,
            request,
            stage: Mutex::new(RichHtmlStage::Running { done }),
        })
    }

    /// How it ended, once it has.
    pub fn outcome(&self) -> Option<RichHtmlOutcome> {
        self.stage.lock_or_recover().outcome()
    }

    /// Whether this export saves `message_id` of `chat_id`.
    pub fn saves(&self, chat_id: ChatId, message_id: MessageId) -> bool {
        self.request.chat_id == chat_id && self.request.message_id == message_id
    }
}

/// Where the worker leaves how the export ended. Dropped without an outcome,
/// which only a panic does (`spawn_guarded` catches it), its sender goes
/// away unsent and the export reads as `Failed`, so it never looks like it
/// is still running and the same message can be saved again.
struct OutcomeSlot(Option<Sender<RichHtmlOutcome>>);

impl OutcomeSlot {
    fn set(mut self, outcome: RichHtmlOutcome) {
        if let Some(sender) = self.0.take() {
            let _ = sender.send(outcome);
        }
    }
}

impl Drop for OutcomeSlot {
    fn drop(&mut self) {
        // The sender goes first: the woken reader must see the end.
        drop(self.0.take());
        crate::wake::UPDATES.poke();
    }
}

#[cfg(test)]
mod tests {
    use super::{OutcomeSlot, RichHtmlOutcome, RichHtmlStage};
    use std::sync::mpsc::channel;

    #[test]
    fn a_panicking_worker_still_ends_as_failed() {
        let (sender, done) = channel();
        let mut stage = RichHtmlStage::Running { done };
        let slot = OutcomeSlot(Some(sender));
        assert_eq!(stage.outcome(), None);
        let joined = std::thread::spawn(move || {
            let _slot = slot;
            panic!("worker panic (expected by this test)");
        })
        .join();
        assert!(joined.is_err());
        assert_eq!(stage.outcome(), Some(RichHtmlOutcome::Failed));
        assert_eq!(stage.outcome(), Some(RichHtmlOutcome::Failed));
    }

    #[test]
    fn a_set_outcome_is_kept() {
        let (sender, done) = channel();
        let mut stage = RichHtmlStage::Running { done };
        OutcomeSlot(Some(sender)).set(RichHtmlOutcome::Cancelled);
        assert_eq!(stage.outcome(), Some(RichHtmlOutcome::Cancelled));
        assert_eq!(stage.outcome(), Some(RichHtmlOutcome::Cancelled));
    }
}
