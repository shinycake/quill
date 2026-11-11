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

/// Where an export stands: the worker is running, or it left how it ended.
/// The worker publishes `Done` once; a worker that dies leaves `Done(Failed)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RichHtmlStage {
    Running,
    Done(RichHtmlOutcome),
}

pub struct RichHtmlExport {
    pub link: WorkerLink,
    pub request: RichHtmlRequest,
    stage: Arc<Mutex<RichHtmlStage>>,
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
        let stage = Arc::new(Mutex::new(RichHtmlStage::Running));
        let worker_stage = stage.clone();
        let worker_request = request.clone();
        crate::thread_guard::spawn_guarded("quill-rich-html", move || {
            let slot = OutcomeSlot(worker_stage);
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
            stage,
        })
    }

    /// How it ended, once it has.
    pub fn outcome(&self) -> Option<RichHtmlOutcome> {
        match &*self.stage.lock_or_recover() {
            RichHtmlStage::Running => None,
            RichHtmlStage::Done(outcome) => Some(outcome.clone()),
        }
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
struct OutcomeSlot(Arc<Mutex<RichHtmlStage>>);

impl OutcomeSlot {
    fn set(self, outcome: RichHtmlOutcome) {
        *self.0.lock_or_recover() = RichHtmlStage::Done(outcome);
    }
}

impl Drop for OutcomeSlot {
    fn drop(&mut self) {
        let mut stage = self.0.lock_or_recover();
        if *stage == RichHtmlStage::Running {
            *stage = RichHtmlStage::Done(RichHtmlOutcome::Failed);
        }
        drop(stage);
        crate::wake::UPDATES.poke();
    }
}

#[cfg(test)]
mod tests {
    use super::{OutcomeSlot, RichHtmlOutcome, RichHtmlStage};
    use std::sync::{Arc, Mutex};

    #[test]
    fn a_panicking_worker_still_ends_as_failed() {
        let outcome = Arc::new(Mutex::new(RichHtmlStage::Running));
        let slot = OutcomeSlot(outcome.clone());
        let joined = std::thread::spawn(move || {
            let _slot = slot;
            panic!("worker panic (expected by this test)");
        })
        .join();
        assert!(joined.is_err());
        assert_eq!(
            *outcome.lock().unwrap(),
            RichHtmlStage::Done(RichHtmlOutcome::Failed)
        );
    }

    #[test]
    fn a_set_outcome_is_kept() {
        let outcome = Arc::new(Mutex::new(RichHtmlStage::Running));
        OutcomeSlot(outcome.clone()).set(RichHtmlOutcome::Cancelled);
        assert_eq!(
            *outcome.lock().unwrap(),
            RichHtmlStage::Done(RichHtmlOutcome::Cancelled)
        );
    }
}
