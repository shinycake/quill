//! The viewer's own sending rights in the open chat: the sentence that
//! replaces a group's composer when nothing can be sent, and the refusals
//! for a single recording, attachment, sticker, GIF or poll
//! (`quill::send_rights`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::composer::AttachmentKind;
use quill::local_time::{civil_local, full_stamp, now_unix};
use quill::send_rights::{SendKind, SendRights};

register_demos![
    // Demo chat A's user accepts no voice or video messages: the record
    // button is dimmed, and clicking it says why.
    DemoSpec::chats(
        "ready-voice-restricted",
        "screenshot demo — the user accepts no voice messages"
    )
    .setup(QuillApp::demo_voice_restricted),
];

/// The user behind the demo's open private chat ("Demo chat A").
const DEMO_RESTRICTED_USER: i64 = 11;

/// "{date}, {time}" for the end of a restriction.
fn until_text(unix: i32) -> String {
    full_stamp(&civil_local(i64::from(unix)))
}

impl QuillApp {
    /// The open chat's rights, `None` without a chat.
    fn open_send_rights(&self) -> Option<SendRights> {
        let session = self.session()?;
        let chat = session.chats.get(&session.open_chat?.0)?;
        Some(SendRights::for_chat(session, chat, now_unix()))
    }

    /// The sentence that takes the composer's place when the viewer may
    /// send nothing here (tdesktop's `TextErrorSendRestriction`).
    pub(super) fn composer_restriction(&self) -> Option<String> {
        let session = self.session()?;
        let chat_id = session.open_chat?;
        let replying = self
            .chat_ui
            .reply()
            .filter(|reply| reply.chat_id == chat_id)
            .map(|reply| reply.message_id);
        if session.direct_reply_locked(chat_id, replying) {
            return Some(quill::lang::tr(quill::lang::keys::MONOFORUM_CHOOSE_TO_REPLY).to_string());
        }
        self.open_send_rights()?.composer_block(&until_text)
    }

    /// Why `kind` is refused in the open chat, if it is.
    pub(super) fn send_denial(&self, kind: SendKind) -> Option<String> {
        self.open_send_rights()?.denial(kind, &until_text)
    }

    /// Show the refusal for `kind` as the status note. True when refused.
    pub(super) fn deny_send(&mut self, kind: SendKind, cx: &mut Context<Self>) -> bool {
        let Some(note) = self.send_denial(kind) else {
            return false;
        };
        self.set_note(StatusNote::error(note));
        cx.notify();
        true
    }

    /// Show the first refusal among the kinds `attachments` are sent as.
    pub(super) fn deny_attachments(
        &mut self,
        kinds: impl IntoIterator<Item = AttachmentKind>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(note) = self
            .open_send_rights()
            .and_then(|rights| rights.attachment_denial(kinds, &until_text))
        else {
            return false;
        };
        self.set_note(StatusNote::error(note));
        cx.notify();
        true
    }

    fn demo_voice_restricted(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            let infos = &mut session.users_state.user_full_infos;
            let mut info = infos
                .get(&DEMO_RESTRICTED_USER)
                .cloned()
                .unwrap_or_default();
            info.voice_notes_restricted = true;
            infos.insert(DEMO_RESTRICTED_USER, info);
        }
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "send_limits_tests.rs"]
mod tests;
