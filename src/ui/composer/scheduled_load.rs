//! The scheduled-messages dialog's load: one stage for the dialog and the
//! `getChatScheduledMessages` request that fills it.
use super::*;
use crate::ui::status_note::StatusNote;
use quill::ids::RequestId;
use quill::lang::{keys, tr};

/// The scheduled-messages dialog of one chat view. `Loading` leaves on the
/// request's answer (`Ready`, or `Failed` with the line the dialog draws and
/// a Retry); the dialog closing, or the chat view being replaced, drops it
/// from any stage.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::ui) enum ScheduledDialog {
    #[default]
    Closed,
    Loading(RequestId),
    Ready,
    Failed(String),
}

impl ScheduledDialog {
    pub(in crate::ui) fn is_open(&self) -> bool {
        !matches!(self, Self::Closed)
    }

    pub(in crate::ui) fn loading(&self) -> Option<RequestId> {
        match self {
            Self::Loading(request) => Some(*request),
            _ => None,
        }
    }
}

/// A scheduled message's delete that TDLib has not answered. Its row is
/// already gone from the list; a refusal puts the list back from TDLib. It
/// leaves with its answer, or with the chat view that sent it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) struct ScheduledDelete {
    pub(in crate::ui) request: RequestId,
    pub(in crate::ui) chat_id: quill::ids::ChatId,
}

/// Take the answered deletes out of `pending`; the chats whose delete was
/// refused come back, so their lists load again. An unanswered one stays.
fn take_refused(
    pending: &mut Vec<ScheduledDelete>,
    mut answer: impl FnMut(RequestId) -> Option<quill::state::Answer>,
) -> Vec<quill::ids::ChatId> {
    let mut refused = Vec::new();
    pending.retain(|delete| match answer(delete.request) {
        None => true,
        Some(quill::state::Answer::Done) => false,
        Some(quill::state::Answer::Failed(_)) => {
            refused.push(delete.chat_id);
            false
        }
    });
    refused
}

impl QuillApp {
    /// M1: load the chat's scheduled sends and open the dialog.
    pub(in crate::ui) fn open_scheduled_dialog(&mut self, cx: &mut Context<Self>) {
        self.chat_ui.hide_schedule_popup();
        if let Some(request) = self.chat_ui.scheduled_dialog.loading()
            && let Some(live) = self.live.as_mut()
        {
            live.driver.session.acks.forget(request);
        }
        self.chat_ui.scheduled_dialog = match self.live.as_mut() {
            Some(live) => match live
                .driver
                .session
                .open_chat_id()
                .map(|chat_id| live.driver.get_chat_scheduled_messages(chat_id))
            {
                Some(Ok(request)) => {
                    live.driver.session.acks.expect(request);
                    ScheduledDialog::Loading(request)
                }
                _ => ScheduledDialog::Failed(
                    tr(keys::Q_CHAT_COULD_NOT_LOAD_SCHEDULED_MESSAGES).to_string(),
                ),
            },
            // The demo holds its list already.
            None => ScheduledDialog::Ready,
        };
        cx.notify();
    }

    /// Frame upkeep: the scheduled list's request answered.
    pub(in crate::ui) fn settle_scheduled_load(&mut self, cx: &mut Context<Self>) {
        let Some(request) = self.chat_ui.scheduled_dialog.loading() else {
            return;
        };
        let Some(answer) = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.acks.take(request))
        else {
            return;
        };
        self.chat_ui.scheduled_dialog = match answer {
            quill::state::Answer::Done => ScheduledDialog::Ready,
            quill::state::Answer::Failed(_) => ScheduledDialog::Failed(
                tr(keys::Q_CHAT_COULD_NOT_LOAD_SCHEDULED_MESSAGES).to_string(),
            ),
        };
        cx.notify();
    }

    /// Frame upkeep: a scheduled delete TDLib answered. A refusal (offline,
    /// the message already went out) says so and loads the chat's list
    /// again, so a row that is still scheduled comes back.
    pub(in crate::ui) fn settle_scheduled_deletes(&mut self, cx: &mut Context<Self>) {
        if self.chat_ui.scheduled_deletes.is_empty() {
            return;
        }
        let mut pending = std::mem::take(&mut self.chat_ui.scheduled_deletes);
        let Some(live) = self.live.as_mut() else {
            self.chat_ui.scheduled_deletes = pending;
            return;
        };
        let refused = take_refused(&mut pending, |request| live.driver.session.acks.take(request));
        self.chat_ui.scheduled_deletes = pending;
        if refused.is_empty() {
            return;
        }
        for chat_id in refused {
            // A reload that cannot be sent leaves the list as it is; the
            // dialog's next open loads it again.
            if let Some(live) = self.live.as_mut() {
                let _ = live.driver.get_chat_scheduled_messages(chat_id);
            }
        }
        self.set_note(StatusNote::error(tr(
            keys::Q_CHAT_COULD_NOT_DELETE_SCHEDULED_MESSAGE,
        )));
        cx.notify();
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "scheduled_load_tests.rs"]
mod tests;
