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
use std::sync::{Arc, Mutex};

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

pub struct RichHtmlExport {
    pub link: WorkerLink,
    pub request: RichHtmlRequest,
    outcome: Arc<Mutex<Option<RichHtmlOutcome>>>,
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
        let outcome = Arc::new(Mutex::new(None));
        let worker_outcome = outcome.clone();
        let worker_request = request.clone();
        crate::thread_guard::spawn_guarded("quill-rich-html", move || {
            let slot = OutcomeSlot(worker_outcome);
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
            outcome,
        })
    }

    /// How it ended, once it has.
    pub fn outcome(&self) -> Option<RichHtmlOutcome> {
        self.outcome.lock_or_recover().clone()
    }

    /// Whether this export saves `message_id` of `chat_id`.
    pub fn saves(&self, chat_id: ChatId, message_id: MessageId) -> bool {
        self.request.chat_id == chat_id && self.request.message_id == message_id
    }
}

/// Where the worker leaves how the export ended. Dropped without an outcome,
/// which only a panic does (`spawn_guarded` catches it), it records
/// `Failed`, so the export never looks like it is still running and the
/// same message can be saved again.
struct OutcomeSlot(Arc<Mutex<Option<RichHtmlOutcome>>>);

impl OutcomeSlot {
    fn set(self, outcome: RichHtmlOutcome) {
        *self.0.lock_or_recover() = Some(outcome);
    }
}

impl Drop for OutcomeSlot {
    fn drop(&mut self) {
        self.0
            .lock_or_recover()
            .get_or_insert(RichHtmlOutcome::Failed);
        crate::wake::UPDATES.poke();
    }
}

#[cfg(test)]
mod tests {
    use super::{OutcomeSlot, RichHtmlOutcome};
    use std::sync::{Arc, Mutex};

    #[test]
    fn a_panicking_worker_still_ends_as_failed() {
        let outcome = Arc::new(Mutex::new(None));
        let slot = OutcomeSlot(outcome.clone());
        let joined = std::thread::spawn(move || {
            let _slot = slot;
            panic!("worker panic (expected by this test)");
        })
        .join();
        assert!(joined.is_err());
        assert_eq!(*outcome.lock().unwrap(), Some(RichHtmlOutcome::Failed));
    }

    #[test]
    fn a_set_outcome_is_kept() {
        let outcome = Arc::new(Mutex::new(None));
        OutcomeSlot(outcome.clone()).set(RichHtmlOutcome::Cancelled);
        assert_eq!(*outcome.lock().unwrap(), Some(RichHtmlOutcome::Cancelled));
    }
}
