//! Text the composer handed to a request that answers later, kept with the
//! request so the answer is matched to what asked:
//!
//! - [`SentText`]: what a rich send took out of the field, kept until TDLib
//!   answers so a refusal (a flood wait, a lapsed Premium, a closed chat)
//!   gives it back. tdesktop clears the field as the send starts and so
//!   does Quill; a send that never became a message must not lose what was
//!   typed.
//! - [`AiAsk`]: the draft an AI action (Fix, Rewrite) was asked about, so
//!   its answer replaces that draft only while the field still holds it.
use super::*;
use quill::ids::RequestId;

/// One chat view's last rich send and where it stands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::ui) enum SentText {
    #[default]
    None,
    /// The field was cleared for the send `request`; `markup` is what it held.
    Sent { request: RequestId, markup: String },
}

/// The draft the last AI composer action was asked about.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::ui) enum AiAsk {
    #[default]
    Idle,
    /// Asked about this markup; the answer applies while the field holds it.
    Asked { markup: String },
}

impl QuillApp {
    /// The rich send `request` took `markup` out of the field. A newer
    /// send replaces an older one (its answer no longer restores anything).
    pub(super) fn remember_sent_text(&mut self, request: RequestId, markup: String) {
        if let SentText::Sent { request: old, .. } =
            std::mem::replace(&mut self.chat_ui.sent_text, SentText::Sent { request, markup })
            && let Some(live) = self.live.as_mut()
        {
            live.driver.session.acks.forget(old);
        }
        if let Some(live) = self.live.as_mut() {
            live.driver.session.acks.expect(request);
        }
    }

    /// Frame upkeep: a refused rich send puts its text back in the field,
    /// when the user has typed nothing since; an accepted one lets it go.
    pub(in crate::ui) fn settle_sent_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let SentText::Sent { request, .. } = &self.chat_ui.sent_text else {
            return;
        };
        let request = *request;
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let Some(answer) = live.driver.session.acks.take(request) else {
            return;
        };
        let SentText::Sent { markup, .. } = std::mem::take(&mut self.chat_ui.sent_text) else {
            return;
        };
        if matches!(answer, quill::state::Answer::Done) {
            return;
        }
        if self.composer.read(cx).value().is_empty() && !self.chat_ui.is_editing() {
            self.chat_ui.rich_editor_open = true;
            self.set_composer_markup(&markup, window, cx);
            self.note_open_draft(false, cx);
            cx.notify();
        }
    }
}
