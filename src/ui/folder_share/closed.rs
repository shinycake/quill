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
    /// The folder editor's create or edit: the box closed while it was out,
    /// so the toast says whether the folder was saved or why not.
    FolderSaved { edited: bool },
}

impl QuillApp {
    /// The toast for a saved folder: edited, or created.
    pub(in crate::ui) fn folder_saved_note(edited: bool) -> StatusNote {
        StatusNote::silent(tr(if edited {
            keys::Q_CHATLIST_FOLDER_UPDATED
        } else {
            keys::Q_CHATLIST_FOLDER_CREATED
        }))
    }

    /// Every way the folder editor closes (Cancel, Esc, the backdrop, a
    /// replacing page): a save still out is handed to `closed_asks`, so
    /// its answer is still told. A fetch for a merge is not a save; it ends
    /// with the box.
    pub(in crate::ui) fn close_folder_editor(&mut self, cx: &mut Context<Self>) {
        if let Some(dialog) = self.folders.editor.take()
            && let FolderLoad::Saving(request) = dialog.load
        {
            self.folders.closed_asks.insert(
                request,
                ClosedAsk::FolderSaved {
                    edited: dialog.folder_id.is_some(),
                },
            );
        }
        cx.notify();
    }

    /// Poll-loop half: the answer of each request a closed box left.
    /// Returns whether anything changed.
    pub(super) fn settle_closed_folder_asks(&mut self, cx: &mut Context<Self>) -> bool {
        if self.folders.closed_asks.is_empty() {
            return false;
        }
        let mut waiting: Vec<RequestId> = self.folders.closed_asks.keys().copied().collect();
        // Request order: the newest queued link wins below.
        waiting.sort_by_key(|request| request.0);
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
                if session.requests.get(request).is_none()
                    && let Some(ClosedAsk::FolderAdded { next, .. }) =
                        self.folders.closed_asks.remove(&request)
                {
                    // The queued link still opens.
                    queued = next.or(queued);
                }
                continue;
            };
            let Some(ask) = self.folders.closed_asks.remove(&request) else {
                continue;
            };
            match (ask, answer) {
                (ClosedAsk::FolderAdded { next, .. }, DialogAnswer::Failed(line)) => {
                    self.set_note(StatusNote::error(line));
                    queued = next.or(queued);
                }
                (ClosedAsk::FolderAdded { next, .. }, DialogAnswer::LimitHit) => {
                    queued = next.or(queued);
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
                    queued = next.or(queued);
                }
                // The delete or save landed: a box reopened meanwhile held its
                // list back for it (`fetch_folder_share`) and asks it now.
                (ClosedAsk::LinkChange, _) => self.refetch_open_folder_links(),
                (ClosedAsk::FolderSaved { edited }, _) => {
                    self.set_note(Self::folder_saved_note(edited));
                }
            }
            changed = true;
        }
        // A link queued behind a closed add gives way to anything newer: a
        // box the user opened since stays as it is.
        if let Some(link) = queued
            && self.folders.invite.is_none()
        {
            self.open_folder_invite(link, cx);
        }
        if changed {
            cx.notify();
        }
        changed
    }
}
