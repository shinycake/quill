//! The "Add folder" box for an `addlist` link (tdesktop `ui/chatlist_box`):
//! its check and its add each wait for the answer to their own request.

use super::*;
use crate::ui::dialogs::InviteStage;
use quill::lang::{keys, tr, tr_with, trn};
use quill::state::DialogAnswer;

impl QuillApp {
    /// `addlist` link opened: show the box and check the link. A check that
    /// cannot be sent ends the box in the error, not in "Checking link...".
    pub(in crate::ui) fn open_folder_invite(&mut self, link: String, cx: &mut Context<Self>) {
        let stage = match self.live.as_mut() {
            Some(live) => match live.driver.check_folder_invite_link(&link) {
                Ok(request) => InviteStage::Checking(request),
                Err(err) => {
                    let line = user_error_for(keys::Q_NOTES_FAILED_CHECK_FOLDER_LINK, &err);
                    self.set_note(StatusNote::error(line.clone()));
                    InviteStage::Failed(line)
                }
            },
            None => InviteStage::Failed(tr(keys::Q_NOTES_NO_CONNECTION).to_string()),
        };
        self.folders.invite = Some(FolderInviteDialog::new(link, stage));
        self.open_box(DialogKind::FolderInvite);
        cx.notify();
    }

    pub(in crate::ui) fn close_folder_invite(&mut self, cx: &mut Context<Self>) {
        self.folders.invite = None;
        cx.notify();
    }

    /// Poll-loop half of the add-by-link box: take the answer of the check
    /// or the add it waits for, fetch the titles of chats we do not know
    /// yet, and finish when the add is confirmed.
    pub(in crate::ui) fn drive_folder_invite(&mut self, cx: &mut Context<Self>) -> bool {
        let waiting = match self.folders.invite.as_ref().map(|d| &d.stage) {
            Some(InviteStage::Checking(request)) => Some(*request),
            Some(InviteStage::Adding { request, .. }) => Some(*request),
            Some(_) => None,
            None => return false,
        };
        let mut changed = false;
        if let Some(request) = waiting {
            let answer = self
                .session_mut_any()
                .and_then(|session| session.chat_list.dialog_answers.take(request));
            if let Some(answer) = answer {
                changed = true;
                if self.settle_folder_invite(answer) {
                    cx.notify();
                    return true;
                }
            }
        }
        if let Some(dialog) = self.folders.invite.as_ref()
            && let Some(info) = dialog.info().cloned()
        {
            let unknown: Vec<i64> = info
                .missing_chat_ids
                .iter()
                .chain(info.added_chat_ids.iter())
                .copied()
                .filter(|id| !dialog.asked_chats.contains(id))
                .filter(|id| self.session().is_some_and(|s| !s.chats.contains_key(id)))
                .collect();
            // Asked once each, marked only once the asks went out: a send
            // that failed is tried again, an answer that failed is not.
            let sent = self
                .live
                .as_mut()
                .is_some_and(|live| live.driver.fetch_chats_for_folder_invite(&unknown).is_ok());
            if sent && let Some(dialog) = self.folders.invite.as_mut() {
                dialog.asked_chats.extend(unknown.iter().copied());
            }
        }
        if changed {
            cx.notify();
        }
        changed
    }

    /// The answer to the check or the add. `true` when the box is over
    /// (the folder was added).
    fn settle_folder_invite(&mut self, answer: DialogAnswer) -> bool {
        let Some(dialog) = self.folders.invite.as_mut() else {
            return false;
        };
        let stage = std::mem::replace(&mut dialog.stage, InviteStage::Failed(String::new()));
        match (stage, answer) {
            (InviteStage::Checking(_), DialogAnswer::Checked(info)) => {
                dialog.selected = info.missing_chat_ids.iter().copied().collect();
                dialog.stage = InviteStage::Ready { info, error: None };
                false
            }
            (InviteStage::Checking(_), DialogAnswer::Failed(line)) => {
                dialog.stage = InviteStage::Failed(line);
                false
            }
            (InviteStage::Adding { info, .. }, DialogAnswer::Failed(line)) => {
                dialog.stage = InviteStage::Ready {
                    info,
                    error: Some(line),
                };
                false
            }
            (InviteStage::Adding { info, .. }, _) => {
                let joined = dialog.selected.len();
                let name = info.folder.name.clone();
                self.folders.invite = None;
                self.set_note(match joined {
                    0 => StatusNote::silent(tr_with(
                        keys::FILTERS_ADDED_TITLE,
                        &[("folder", &name.to_string())],
                    )),
                    n => StatusNote::silent(format!(
                        "{} {}",
                        tr_with(keys::FILTERS_ADDED_TITLE, &[("folder", &name.to_string())]),
                        trn(keys::FILTERS_ADDED_ALSO, n)
                    )),
                });
                true
            }
            // An answer that is not for this stage changes nothing.
            (stage, _) => {
                dialog.stage = stage;
                false
            }
        }
    }

    pub(in crate::ui) fn confirm_folder_invite(&mut self, cx: &mut Context<Self>) {
        let Some((link, selected, info)) = self.folders.invite.as_ref().and_then(|d| {
            let mut ids: Vec<i64> = d.selected.iter().copied().collect();
            ids.sort_unstable();
            Some((d.link.clone(), ids, d.info()?.clone()))
        }) else {
            return;
        };
        // A new folder keeps every chat of the link; chats already joined
        // are part of it whatever was ticked.
        let mut chat_ids = selected;
        if info.folder.id == 0 {
            chat_ids.extend(info.added_chat_ids.iter().copied());
            chat_ids.sort_unstable();
            chat_ids.dedup();
        }
        let Some(live) = self.live.as_mut() else {
            // Screenshot demo: finish without a server.
            self.folders.invite = None;
            self.set_note(StatusNote::silent(tr(keys::Q_CHATLIST_FOLDER_ADDED)));
            cx.notify();
            return;
        };
        let sent = live.driver.add_folder_by_invite_link(&link, &chat_ids);
        if let Some(dialog) = self.folders.invite.as_mut() {
            let info = Box::new(info);
            dialog.stage = match sent {
                Ok(request) => InviteStage::Adding { request, info },
                Err(err) => InviteStage::Ready {
                    info,
                    error: Some(user_error_for(keys::Q_NOTES_FAILED_ADD_FOLDER, &err)),
                },
            };
        }
        cx.notify();
    }
}
