//! The next send's choices, kept per chat and topic: the attachment tray
//! and its options, the schedule and the link preview choice, plus each
//! chat's "send silently" override. The composer reads the entry for the
//! open chat on every render, so opening another chat by any path shows
//! that chat's entry and nothing chosen elsewhere goes with its messages
//! (docs/decisions/sweep-per-chat-composer.md).
//!
//! The store lives on `QuillApp`, outside the per-window slot
//! (`chat_window::slot`), so it is keyed by chat, never by window.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use quill::composer::{
    ComposerAttachment, ComposerScheduling, DraftKey, LinkPreviewChoice, PreviewMediaSize,
};
use quill::ids::ChatId;
use quill::telegram::requests::SelfDestructSend;
use std::collections::HashMap;
use std::path::Path;

/// What the next send from one slot carries besides its text.
pub(crate) struct PendingSend {
    /// Local files the user attached (canonical paths via `pick`). One item
    /// sends with `sendMessage`; two or more photos/videos send as an album.
    pub(super) attachments: Vec<ComposerAttachment>,
    /// Self-destruct timer for photo/video sends (`self_destruct_type`,
    /// private chats only).
    pub(super) self_destruct: Option<SelfDestructSend>,
    /// `show_caption_above_media` for photo/video sends.
    pub(super) caption_above: bool,
    /// "High quality": photos keep up to 2560 px instead of 1280 px.
    pub(super) hd_photos: bool,
    /// Stars to unlock the media; 0 sends a normal message.
    pub(super) paid_stars: i64,
    /// `messageSchedulingState*` for the next send.
    pub(super) scheduling: ComposerScheduling,
    /// The repeat period (seconds) chosen with the schedule; zero for never.
    pub(super) schedule_repeat: i32,
    /// The link preview choice (tdesktop keeps it in the draft as
    /// `WebPageDraft`).
    pub(super) preview: LinkPreviewChoice,
}

impl PendingSend {
    const EMPTY: Self = Self {
        attachments: Vec::new(),
        self_destruct: None,
        caption_above: false,
        hd_photos: false,
        paid_stars: 0,
        scheduling: ComposerScheduling::None,
        schedule_repeat: 0,
        preview: LinkPreviewChoice {
            disabled: false,
            above_text: false,
            media: PreviewMediaSize::Auto,
            link_index: 0,
        },
    };

    /// Drop the tray and the choices that only make sense with it.
    pub(super) fn clear_attachments(&mut self) {
        self.attachments.clear();
        self.self_destruct = None;
        self.caption_above = false;
        self.hd_photos = false;
        self.paid_stars = 0;
    }
}

impl Default for PendingSend {
    fn default() -> Self {
        Self::EMPTY
    }
}

static NOTHING_PENDING: PendingSend = PendingSend::EMPTY;

/// Per-chat send state for every chat and topic the user touched.
#[derive(Default)]
pub(crate) struct ComposerSends {
    /// Keyed like the text draft (tdesktop's `Data::DraftKey`).
    slots: HashMap<DraftKey, PendingSend>,
    /// "Send silently" chosen against the chat's
    /// `default_disable_notification`, per chat (tdesktop's silent toggle
    /// is the channel's own setting).
    silent: HashMap<ChatId, bool>,
}

impl ComposerSends {
    pub(super) fn get(&self, slot: DraftKey) -> &PendingSend {
        self.slots.get(&slot).unwrap_or(&NOTHING_PENDING)
    }

    pub(super) fn get_mut(&mut self, slot: DraftKey) -> &mut PendingSend {
        self.slots.entry(slot).or_default()
    }

    /// The slot's send went: hand back what it held.
    pub(super) fn finish(&mut self, slot: DraftKey) -> Option<PendingSend> {
        self.slots.remove(&slot)
    }

    /// Every tray entry for `path`, whichever chat it waits in: a file
    /// check that finishes after the user moved on still lands.
    pub(super) fn attachments_at_mut<'a>(
        &'a mut self,
        path: &'a Path,
    ) -> impl Iterator<Item = &'a mut ComposerAttachment> + 'a {
        self.slots
            .values_mut()
            .flat_map(|send| send.attachments.iter_mut())
            .filter(move |attachment| attachment.path == path)
    }

    pub(super) fn silent_override(&self, chat_id: ChatId) -> Option<bool> {
        self.silent.get(&chat_id).copied()
    }

    pub(super) fn set_silent(&mut self, chat_id: ChatId, silent: bool, chat_default: bool) {
        if silent == chat_default {
            self.silent.remove(&chat_id);
        } else {
            self.silent.insert(chat_id, silent);
        }
    }
}

impl QuillApp {
    /// The slot the composer on screen sends from: the open chat (the
    /// window's own while a chat window is in scope) and its topic.
    pub(super) fn send_slot(&self) -> Option<DraftKey> {
        let chat_id = self.open_chat_id()?;
        Some(self.open_draft_topic(chat_id).key(chat_id.0))
    }

    /// The open slot's next send. Nothing pending when no chat is open.
    pub(super) fn pending_send(&self) -> &PendingSend {
        match self.send_slot() {
            Some(slot) => self.composer_sends.get(slot),
            None => &NOTHING_PENDING,
        }
    }

    /// The open slot's next send, to change. `None` when no chat is open.
    pub(super) fn pending_send_mut(&mut self) -> Option<&mut PendingSend> {
        let slot = self.send_slot()?;
        Some(self.composer_sends.get_mut(slot))
    }

    /// A screenshot demo starts with these in the open chat's tray.
    pub(super) fn seed_demo_attachments(&mut self, attachments: Vec<ComposerAttachment>) {
        if attachments.is_empty() {
            return;
        }
        if let Some(tray) = self.pending_attachments_mut() {
            *tray = attachments;
        }
    }

    /// The open slot's attachment tray.
    pub(super) fn pending_attachments(&self) -> &[ComposerAttachment] {
        &self.pending_send().attachments
    }

    /// The open slot's attachment tray, to change.
    pub(super) fn pending_attachments_mut(&mut self) -> Option<&mut Vec<ComposerAttachment>> {
        self.pending_send_mut().map(|send| &mut send.attachments)
    }

    pub(super) fn pending_attachment_mut(
        &mut self,
        index: usize,
    ) -> Option<&mut ComposerAttachment> {
        self.pending_attachments_mut()?.get_mut(index)
    }

    /// The open slot's send went: it starts over (tdesktop clears the field
    /// and resets the preview draft after sending), and the tray's file
    /// checks forget the files it sent.
    pub(super) fn finish_pending_send(&mut self) {
        let Some(slot) = self.send_slot() else {
            return;
        };
        if let Some(sent) = self.composer_sends.finish(slot) {
            self.composer_ui.send_box.forget(
                sent.attachments
                    .iter()
                    .map(|attachment| attachment.path.as_path()),
            );
        }
    }

    /// Drop the open slot's tray and the choices made for it; the schedule
    /// and link preview choice stay.
    pub(super) fn clear_pending_attachments(&mut self) {
        let Some(send) = self.pending_send_mut() else {
            return;
        };
        let dropped = std::mem::take(&mut send.attachments);
        send.clear_attachments();
        self.composer_ui
            .send_box
            .forget(dropped.iter().map(|attachment| attachment.path.as_path()));
    }

    /// Whether the next send from the open chat goes without sound: the
    /// user's choice for this chat, else the chat's
    /// `default_disable_notification`.
    pub(super) fn composer_effective_silent(&self) -> bool {
        let Some(chat_id) = self.open_chat_id() else {
            return false;
        };
        self.composer_sends
            .silent_override(chat_id)
            .unwrap_or_else(|| self.chat_default_silent(chat_id))
    }

    /// Flip "send silently" for the open chat; stays until flipped back.
    pub(in crate::ui) fn toggle_composer_silent(&mut self) {
        let Some(chat_id) = self.open_chat_id() else {
            return;
        };
        let next = !self.composer_effective_silent();
        let chat_default = self.chat_default_silent(chat_id);
        self.composer_sends.set_silent(chat_id, next, chat_default);
    }

    fn chat_default_silent(&self, chat_id: ChatId) -> bool {
        self.session()
            .is_some_and(|session| session.sync.is_default_silent(chat_id.0))
    }
}
