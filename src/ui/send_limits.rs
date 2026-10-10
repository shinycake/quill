//! The viewer's own sending rights in the open group: the sentence that
//! replaces the composer when nothing can be sent, and the refusals for a
//! single recording, attachment, sticker, GIF or poll (`quill::send_rights`).

use super::app::QuillApp;
use gpui_kit::*;
use quill::composer::AttachmentKind;
use quill::local_time::{civil_local, full_stamp, now_unix};
use quill::send_rights::{SendKind, SendRights};

/// "{date}, {time}" for the end of a restriction.
fn until_text(unix: i32) -> String {
    full_stamp(&civil_local(i64::from(unix)))
}

impl QuillApp {
    /// The open chat's rights, `None` outside a group or without a chat.
    fn open_send_rights(&self) -> Option<SendRights> {
        let session = self.session()?;
        let chat = session.chats.get(&session.open_chat?.0)?;
        Some(SendRights::for_chat(session, chat, now_unix()))
    }

    /// The sentence that takes the composer's place when the viewer may
    /// send nothing here (tdesktop's `TextErrorSendRestriction`).
    pub(super) fn composer_restriction(&self) -> Option<String> {
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
        self.status_note = note;
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
        self.status_note = note;
        cx.notify();
        true
    }
}
