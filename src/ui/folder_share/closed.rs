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
    /// many chats were joined.
    FolderAdded { folder: String, joined: usize },
    /// A link save or delete of the Share Folder box: the list is the
    /// session's, so only a failure has something to say.
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
        for request in waiting {
            let Some(answer) = self
                .session_mut_any()
                .and_then(|session| session.chat_list.dialog_answers.take(request))
            else {
                continue;
            };
            let Some(ask) = self.folders.closed_asks.remove(&request) else {
                continue;
            };
            match (ask, answer) {
                (_, DialogAnswer::Failed(line)) => self.set_note(StatusNote::error(line)),
                // The limit box says it; no toast on top.
                (_, DialogAnswer::LimitHit) => {}
                (ClosedAsk::FolderAdded { folder, joined }, _) => {
                    self.set_note(folder_added_note(&folder, joined));
                }
                (ClosedAsk::LinkChange, _) => {}
            }
            changed = true;
        }
        if changed {
            cx.notify();
        }
        changed
    }
}
