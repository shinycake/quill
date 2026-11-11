//! The scheduled-messages dialog's load: one stage for the dialog and the
//! `getChatScheduledMessages` request that fills it.
use super::*;
use quill::ids::RequestId;

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
}
