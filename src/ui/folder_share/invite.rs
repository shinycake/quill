//! The "Add folder" box for an `addlist` link (tdesktop `ui/chatlist_box`):
//! its check and its add each wait for the answer to their own request.

use super::*;
use crate::ui::dialogs::InviteStage;
use quill::lang::{keys, tr, tr_with, trn};
use quill::state::DialogAnswer;

/// The toast of a shared folder that was added, with the chats joined.
pub(super) fn folder_added_note(folder: &str, joined: usize) -> StatusNote {
    let added = tr_with(keys::FILTERS_ADDED_TITLE, &[("folder", folder)]);
    match joined {
        0 => StatusNote::silent(added),
        n => StatusNote::silent(format!("{added} {}", trn(keys::FILTERS_ADDED_ALSO, n))),
    }
}

impl QuillApp {
    /// `addlist` link opened: show the box and check the link. A check that
    /// cannot be sent ends the box in the error, not in "Checking link...".
    pub(in crate::ui) fn open_folder_invite(&mut self, link: String, cx: &mut Context<Self>) {
        // An add in flight keeps its box: its answer is the only word on
        // whether that folder was added. Another link waits for it in the
        // stage and is checked when the add settles.
        if let Some(InviteStage::Adding { next, .. }) =
            self.folders.invite.as_mut().map(|d| &mut d.stage)
        {
            *next = Some(link);
            // box-host: re-raises the box the add already shows, where it is.
            self.open_box(DialogKind::FolderInvite);
            cx.notify();
            return;
        }
        self.drop_folder_invite_check();
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
        // box-host: the folder box is the account's one, drawn where it was
        // already shown; a link queued behind an add opens from the poll.
        self.open_box(DialogKind::FolderInvite);
        cx.notify();
    }

    /// Closing the box while the add is out hands that request to
    /// `closed_asks`: its answer still says whether the folder was added.
    pub(in crate::ui) fn close_folder_invite(&mut self, cx: &mut Context<Self>) {
        self.drop_folder_invite_check();
        let mut next = None;
        if let Some(dialog) = self.folders.invite.take()
            && let InviteStage::Adding {
                request,
                info,
                next: queued,
            } = dialog.stage
        {
            self.folders.closed_asks.insert(
                request,
                ClosedAsk::FolderAdded {
                    folder: info.folder.name.to_string(),
                    joined: dialog.selected.len(),
                },
            );
            next = queued;
        }
        // A link that came while the add was out is checked now.
        if let Some(link) = next {
            self.open_folder_invite(link, cx);
        }
        cx.notify();
    }

    /// A box still checking its link is going away or being replaced: its
    /// request leaves the registry, so a later answer is noted by nobody,
    /// and an answer that already arrived is taken and dropped.
    fn drop_folder_invite_check(&mut self) {
        let Some(InviteStage::Checking(old)) = self.folders.invite.as_ref().map(|d| &d.stage)
        else {
            return;
        };
        let old = *old;
        if let Some(session) = self.session_mut_any() {
            session.requests.take(old);
            session.chat_list.dialog_answers.take(old);
        }
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
                let (over, next) = self.settle_folder_invite(answer);
                // A link that came while the add was out is checked now.
                if let Some(link) = next {
                    self.open_folder_invite(link, cx);
                }
                if over {
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
    /// (the folder was added), and the link that waited behind an add.
    fn settle_folder_invite(&mut self, answer: DialogAnswer) -> (bool, Option<String>) {
        let Some(dialog) = self.folders.invite.as_mut() else {
            return (false, None);
        };
        let stage = std::mem::replace(&mut dialog.stage, InviteStage::Failed(String::new()));
        match (stage, answer) {
            (InviteStage::Checking(_), DialogAnswer::Checked(info)) => {
                dialog.selected = info.missing_chat_ids.iter().copied().collect();
                dialog.stage = InviteStage::Ready { info, error: None };
                (false, None)
            }
            (InviteStage::Checking(_), DialogAnswer::Failed(line)) => {
                dialog.stage = InviteStage::Failed(line);
                (false, None)
            }
            (InviteStage::Adding { info, next, .. }, DialogAnswer::Failed(line)) => {
                dialog.stage = InviteStage::Ready {
                    info,
                    error: Some(line),
                };
                (false, next)
            }
            (InviteStage::Adding { info, next, .. }, DialogAnswer::LimitHit) => {
                // The limit box is up; the form stays as it was.
                dialog.stage = InviteStage::Ready { info, error: None };
                (false, next)
            }
            (
                InviteStage::Adding { info, next, .. },
                DialogAnswer::Done | DialogAnswer::Checked(_),
            ) => {
                let joined = dialog.selected.len();
                let name = info.folder.name.clone();
                self.folders.invite = None;
                self.set_note(folder_added_note(&name.to_string(), joined));
                (true, next)
            }
            // An answer that is not for this stage changes nothing.
            (stage, _) => {
                dialog.stage = stage;
                (false, None)
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
                Ok(request) => InviteStage::Adding {
                    request,
                    info,
                    next: None,
                },
                Err(err) => InviteStage::Ready {
                    info,
                    error: Some(user_error_for(keys::Q_NOTES_FAILED_ADD_FOLDER, &err)),
                },
            };
        }
        cx.notify();
    }
}
