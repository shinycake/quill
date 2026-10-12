//! UI half of the voice chat, video chat and live stream links
//! (`parity:deeplink-voice-chat`, see `quill::call_link`).
use super::app::QuillApp;
use gpui_kit::*;
use quill::call_link::{BAD_LINK_NOTE, CallLinkStep, step_note};
use quill::ids::ChatId;

impl QuillApp {
    /// The link's chat is open: offer its call. The call screen's Join
    /// button joins, with the link's hash.
    pub(super) fn offer_call_link(
        &mut self,
        chat_id: ChatId,
        invite_hash: &str,
        live_stream: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.live.as_mut() else {
            self.connection.status_note = "Calls need a signed-in account.".into();
            cx.notify();
            return;
        };
        match live.driver.offer_call_link(chat_id, invite_hash) {
            Ok(step) => {
                self.connection.status_note = match step_note(step, live_stream) {
                    Some(note) => note,
                    None if matches!(step, CallLinkStep::Offer { .. }) => {
                        if live_stream {
                            "Opening the live stream…".into()
                        } else {
                            "Opening the voice chat…".into()
                        }
                    }
                    None => self.connection.status_note.clone(),
                };
            }
            Err(_) => self.connection.status_note = BAD_LINK_NOTE.into(),
        }
        cx.notify();
    }
}
