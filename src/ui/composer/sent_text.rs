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
use super::super::composer_sends::SendSlot;
use super::*;
use quill::ids::{ChatId, RequestId};

/// One chat view's last rich send and where it stands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::ui) enum SentText {
    #[default]
    None,
    /// The field was cleared for the send `request`; `markup` is what it held.
    Sent {
        request: RequestId,
        markup: String,
        slot: Option<SendSlot>,
    },
    /// The same for an inline result pick: the `@bot query` the pick took
    /// out of the field, given back when TDLib refuses the result (an
    /// expired query or result id).
    InlinePick {
        request: RequestId,
        markup: String,
        slot: Option<SendSlot>,
    },
}

impl SentText {
    pub(in crate::ui) fn request(&self) -> Option<RequestId> {
        match self {
            Self::None => None,
            Self::Sent { request, .. } | Self::InlinePick { request, .. } => Some(*request),
        }
    }
}

/// A rich send whose chat view was replaced before TDLib answered (another
/// chat opened, the account switched). It waits, with the slot the text was
/// typed in, so a refusal puts the text back in that slot's draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::ui) struct LeftSend {
    pub(in crate::ui) request: RequestId,
    pub(in crate::ui) slot: super::super::composer_sends::SendSlot,
    pub(in crate::ui) markup: String,
    /// A rich send (it set the slot's draft clear), not an inline pick.
    pub(in crate::ui) rich: bool,
}

/// The draft the last AI composer action was asked about.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::ui) enum AiAsk {
    #[default]
    Idle,
    /// `request` was asked about this markup; only its answer applies, and
    /// only while the field holds it. Its error ends the wait, and a newer
    /// ask replaces it (the older answer is then dropped as stale).
    Asked { request: RequestId, markup: String },
}

impl QuillApp {
    /// The rich send `request` took `markup` out of the field. A newer
    /// send replaces an older one (its answer no longer restores anything).
    pub(super) fn remember_sent_text(&mut self, request: RequestId, markup: String) {
        let slot = self.send_slot();
        self.remember_sent(SentText::Sent {
            request,
            markup,
            slot,
        });
    }

    /// An inline result pick took `markup` out of the field.
    pub(in crate::ui) fn remember_inline_pick(&mut self, request: RequestId, markup: String) {
        let slot = self.send_slot();
        self.remember_sent(SentText::InlinePick {
            request,
            markup,
            slot,
        });
    }

    /// The chat view in place is replaced while a rich send waits: the send
    /// goes on waiting on its own, off the view (`settle_left_sends`).
    pub(in crate::ui) fn leave_sent_text(&mut self, old: SentText) {
        let (request, markup, slot, rich) = match old {
            SentText::None => return,
            SentText::Sent {
                request,
                markup,
                slot,
            } => (request, markup, slot, true),
            SentText::InlinePick {
                request,
                markup,
                slot,
            } => (request, markup, slot, false),
        };
        let Some(slot) = slot else {
            if let Some(live) = self.live.as_mut() {
                live.driver.session.acks.forget(request);
            }
            return;
        };
        let overflow = self.composer_sends.keep_left_send(LeftSend {
            request,
            slot,
            markup,
            rich,
        });
        if let (Some(old), Some(live)) = (overflow, self.live.as_mut()) {
            live.driver.session.acks.forget(old.request);
        }
    }

    fn remember_sent(&mut self, sent: SentText) {
        let Some(request) = sent.request() else {
            return;
        };
        if let Some(old) = std::mem::replace(&mut self.chat_ui.sent_text, sent).request()
            && let Some(live) = self.live.as_mut()
        {
            live.driver.session.acks.forget(old);
        }
        if let Some(live) = self.live.as_mut() {
            live.driver.session.acks.expect(request);
        }
    }

    /// Frame upkeep: a rich send of a chat view since replaced was answered:
    /// a refusal puts its text in the draft of the slot it was typed in, an
    /// accepted one lets it go.
    pub(in crate::ui) fn settle_left_sends(&mut self, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let account = live.driver.session.account.clone();
        let answered = self
            .composer_sends
            .take_left_sends(&account, |request| live.driver.session.acks.take(request));
        for (send, answer) in answered {
            if matches!(answer, quill::state::Answer::Done) {
                continue;
            }
            // The refused send never became a message: the ack of a later
            // send of this chat must not clear the text given back here.
            if send.rich {
                self.composer_sends.set_draft_clear(&send.slot, None);
            }
            let chat_id = ChatId(send.slot.draft.chat_id);
            self.save_chat_draft(
                chat_id,
                send.slot.draft.topic,
                quill::composer::DraftContent::Text(send.markup),
                None,
                false,
                cx,
            );
        }
    }

    /// Frame upkeep: a refused rich send puts its text back in the field,
    /// when the user has typed nothing since; an accepted one lets it go.
    pub(in crate::ui) fn settle_sent_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(request) = self.chat_ui.sent_text.request() else {
            return;
        };
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let Some(answer) = live.driver.session.acks.take(request) else {
            return;
        };
        self.apply_sent_text_answer(&answer, window, cx);
    }

    /// TDLib answered the rich send `settle_sent_text` waits on.
    pub(in crate::ui) fn apply_sent_text_answer(
        &mut self,
        answer: &quill::state::Answer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (markup, rich) = match std::mem::take(&mut self.chat_ui.sent_text) {
            SentText::None => return,
            SentText::Sent { markup, .. } => (markup, true),
            SentText::InlinePick { markup, .. } => (markup, false),
        };
        if matches!(answer, quill::state::Answer::Done) {
            return;
        }
        // The refused send never became a message, so its draft stays: the
        // ack of a later send of this chat must not clear the text given
        // back here.
        if rich && let Some(slot) = self.send_slot() {
            self.composer_sends.set_draft_clear(&slot, None);
        }
        if self.composer.read(cx).value().is_empty() && !self.chat_ui.is_editing() {
            self.chat_ui.rich.set_editor(rich);
            self.set_composer_markup(&markup, window, cx);
            self.note_open_draft(false, cx);
            cx.notify();
        }
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "left_sends_tests.rs"]
mod tests;
