//! Folder-link requests whose box closed before TDLib answered. The
//! answer is still the only word on whether the folder was added or the
//! link changed, so it ends in a toast instead of going unseen.

use super::invite::folder_added_note;
use super::*;
use quill::ids::RequestId;
use quill::state::DialogAnswer;

/// What a closed box's request was for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClosedAsk {
    /// The add of a shared folder: the toast says it was added, with how
    /// many chats were joined. `next` is an `addlist` link that came while
    /// the add was out: its box opens when the add is settled.
    FolderAdded {
        folder: String,
        joined: usize,
        next: Option<String>,
    },
    /// A link save or delete of the Share Folder box: the list is the
    /// session's, so only a failure has something to say; a success asks
    /// the list of a box reopened since (`refetch_open_folder_links`).
    LinkChange,
}

impl QuillApp {
    /// Poll-loop half: the answer of each request a closed box left.
    /// Returns whether anything changed.
    pub(super) fn settle_closed_folder_asks(&mut self, cx: &mut Context<Self>) -> bool {
        if self.folders.closed_asks.is_empty() {
            return false;
        }
        let waiting: Vec<RequestId> = self.folders.closed_asks.keys().copied().collect();
        let mut changed = false;
        let mut queued: Option<String> = None;
        for request in waiting {
            let Some(session) = self.session_mut_any() else {
                continue;
            };
            let Some(answer) = session.chat_list.dialog_answers.take(request) else {
                // Out of the session's requests with no answer kept (the
                // ring of answers dropped it): nothing will come, the
                // entry ends without a toast.
                if session.requests.get(request).is_none() {
                    self.folders.closed_asks.remove(&request);
                }
                continue;
            };
            let Some(ask) = self.folders.closed_asks.remove(&request) else {
                continue;
            };
            match (ask, answer) {
                (ClosedAsk::FolderAdded { next, .. }, DialogAnswer::Failed(line)) => {
                    self.set_note(StatusNote::error(line));
                    queued = queued.or(next);
                }
                (ClosedAsk::FolderAdded { next, .. }, DialogAnswer::LimitHit) => {
                    queued = queued.or(next);
                }
                (_, DialogAnswer::Failed(line)) => self.set_note(StatusNote::error(line)),
                // The limit box says it; no toast on top.
                (_, DialogAnswer::LimitHit) => {}
                (
                    ClosedAsk::FolderAdded {
                        folder,
                        joined,
                        next,
                    },
                    _,
                ) => {
                    self.set_note(folder_added_note(&folder, joined));
                    queued = queued.or(next);
                }
                // The delete or save landed: a box reopened meanwhile held its
                // list back for it (`fetch_folder_share`) and asks it now.
                (ClosedAsk::LinkChange, _) => self.refetch_open_folder_links(),
            }
            changed = true;
        }
        if let Some(link) = queued {
            self.open_folder_invite(link, cx);
        }
        if changed {
            cx.notify();
        }
        changed
    }
}
