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
    pub fn start(request: RichHtmlRequest, files_root: PathBuf) -> io::Result<Self> {
        let (link, mut tdlib) = WorkerLink::pair();
        let outcome = Arc::new(Mutex::new(None));
        let worker_outcome = outcome.clone();
        let worker_request = request.clone();
        crate::thread_guard::spawn_guarded("quill-rich-html", move || {
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
            *worker_outcome.lock_or_recover() = Some(last);
            crate::wake::UPDATES.poke();
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
