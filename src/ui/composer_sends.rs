//! The next send's choices, kept per account, chat and topic: the
//! attachment tray and its options, the schedule and the link preview
//! choice, plus each chat's "send silently" override. The composer reads
//! the entry for the open chat on every render, so opening another chat
//! (or account) by any path shows that chat's entry and nothing chosen
//! elsewhere goes with its messages
//! (docs/decisions/sweep-per-chat-composer.md).
//!
//! The store lives on `QuillApp`, outside the per-window slot
//! (`chat_window::slot`), so it is keyed by chat, never by window. Chat ids
//! repeat across accounts (a group both accounts are in, or a private chat
//! with the same user), so every entry sits under its account; an account
//! in the background keeps its entries, and logging out or removing it
//! drops them.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use quill::composer::{
    ComposerAttachment, ComposerScheduling, DraftKey, LinkPreviewChoice, PreviewMediaSize,
};
use quill::ids::{AccountKey, ChatId};
use quill::telegram::envelope::AuthorizationState;
use quill::telegram::requests::SelfDestructSend;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// What the next send from one slot carries besides its text.
#[derive(Debug, PartialEq)]
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
    /// The text of a Send pressed while a photo edit was rendering: it
    /// goes out when the last one lands.
    pub(super) held_send: Option<String>,
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
        held_send: None,
    };

    /// Drop the tray and the choices that only make sense with it.
    pub(super) fn clear_attachments(&mut self) {
        self.attachments.clear();
        self.self_destruct = None;
        self.caption_above = false;
        self.hd_photos = false;
        self.paid_stars = 0;
        self.held_send = None;
    }

    /// Some photo of the tray is still rendering its edit.
    pub(super) fn renders_an_edit(&self) -> bool {
        self.attachments
            .iter()
            .any(|attachment| attachment.extras.photo_render.is_some())
    }

    /// A held Send waits only on a render that will land.
    fn release_idle_hold(&mut self) {
        if !self.renders_an_edit() {
            self.held_send = None;
        }
    }

    fn is_empty(&self) -> bool {
        *self == Self::EMPTY
    }
}

impl Default for PendingSend {
    fn default() -> Self {
        Self::EMPTY
    }
}

static NOTHING_PENDING: PendingSend = PendingSend::EMPTY;

/// One composer's slot: the account, and the chat and topic in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SendSlot {
    pub(super) account: AccountKey,
    pub(super) draft: DraftKey,
}

/// A tray item as it was when a step that answers later started (a
/// picker, an editor). Its result lands only in that slot, and only while
/// the item at `index` is still the same file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TrayItem {
    pub(super) slot: SendSlot,
    pub(super) index: usize,
    pub(super) path: PathBuf,
}

/// One account's entries.
#[derive(Default)]
struct AccountSends {
    /// Keyed like the text draft (tdesktop's `Data::DraftKey`).
    slots: HashMap<DraftKey, PendingSend>,
    /// "Send silently" chosen against the chat's
    /// `default_disable_notification`, per chat (tdesktop's silent toggle
    /// is the channel's own setting).
    silent: HashMap<ChatId, bool>,
}

impl AccountSends {
    fn is_empty(&self) -> bool {
        self.slots.is_empty() && self.silent.is_empty()
    }
}

/// Per-chat send state for every chat and topic the user touched, per
/// account. An entry that is back to nothing chosen is dropped.
#[derive(Default)]
pub(crate) struct ComposerSends {
    accounts: HashMap<AccountKey, AccountSends>,
}

impl ComposerSends {
    pub(super) fn get(&self, slot: &SendSlot) -> &PendingSend {
        self.accounts
            .get(&slot.account)
            .and_then(|account| account.slots.get(&slot.draft))
            .unwrap_or(&NOTHING_PENDING)
    }

    /// Change the slot's entry. Every write goes through here (or
    /// [`Self::update_item`]), so an entry `f` leaves empty is dropped.
    pub(super) fn update<R>(
        &mut self,
        slot: &SendSlot,
        f: impl FnOnce(&mut PendingSend) -> R,
    ) -> R {
        let account = self.accounts.entry(slot.account.clone()).or_default();
        let send = account.slots.entry(slot.draft).or_default();
        let result = f(send);
        send.release_idle_hold();
        if send.is_empty() {
            account.slots.remove(&slot.draft);
        }
        self.drop_if_empty(&slot.account);
        result
    }

    /// Change the tray item `item` names, if it is still there: `None`
    /// when that slot's tray no longer holds the same file at that place.
    pub(super) fn update_item<R>(
        &mut self,
        item: &TrayItem,
        f: impl FnOnce(&mut ComposerAttachment) -> R,
    ) -> Option<R> {
        let send = self
            .accounts
            .get_mut(&item.slot.account)?
            .slots
            .get_mut(&item.slot.draft)?;
        let result = send
            .attachments
            .get_mut(item.index)
            .filter(|attachment| attachment.path == item.path)
            .map(f);
        send.release_idle_hold();
        result
    }

    /// The slot's send went: hand back what it held.
    pub(super) fn finish(&mut self, slot: &SendSlot) -> Option<PendingSend> {
        let sent = self
            .accounts
            .get_mut(&slot.account)?
            .slots
            .remove(&slot.draft);
        self.drop_if_empty(&slot.account);
        sent
    }

    /// Every tray entry for `path`, whichever account and chat it waits
    /// in: a file check that finishes after the user moved on still lands.
    pub(super) fn attachments_at_mut<'a>(
        &'a mut self,
        path: &'a Path,
    ) -> impl Iterator<Item = &'a mut ComposerAttachment> + 'a {
        self.accounts
            .values_mut()
            .flat_map(|account| account.slots.values_mut())
            .flat_map(|send| send.attachments.iter_mut())
            .filter(move |attachment| attachment.path == path)
    }

    /// Every attachment of every tray, in any account and chat.
    pub(super) fn attachments(&self) -> impl Iterator<Item = &ComposerAttachment> + '_ {
        self.accounts
            .values()
            .flat_map(|account| account.slots.values())
            .flat_map(|send| send.attachments.iter())
    }

    /// Some tray, in any account and chat, holds `path`.
    pub(super) fn holds(&self, path: &Path) -> bool {
        self.accounts
            .values()
            .flat_map(|account| account.slots.values())
            .flat_map(|send| send.attachments.iter())
            .any(|attachment| attachment.path == path)
    }

    pub(super) fn silent_override(&self, account: &AccountKey, chat_id: ChatId) -> Option<bool> {
        self.accounts.get(account)?.silent.get(&chat_id).copied()
    }

    pub(super) fn set_silent(
        &mut self,
        account: &AccountKey,
        chat_id: ChatId,
        silent: bool,
        chat_default: bool,
    ) {
        if silent == chat_default {
            if let Some(sends) = self.accounts.get_mut(account) {
                sends.silent.remove(&chat_id);
            }
            self.drop_if_empty(account);
        } else {
            self.accounts
                .entry(account.clone())
                .or_default()
                .silent
                .insert(chat_id, silent);
        }
    }

    /// The account logged out or was removed: its choices go with it.
    /// Hands back the trays it held.
    pub(super) fn forget_account(&mut self, account: &AccountKey) -> Vec<ComposerAttachment> {
        self.accounts
            .remove(account)
            .map(|sends| {
                sends
                    .slots
                    .into_values()
                    .flat_map(|send| send.attachments)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn drop_if_empty(&mut self, account: &AccountKey) {
        if self
            .accounts
            .get(account)
            .is_some_and(AccountSends::is_empty)
        {
            self.accounts.remove(account);
        }
    }

    /// How many slots hold something, over every account.
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn slot_count(&self) -> usize {
        self.accounts
            .values()
            .map(|account| account.slots.len())
            .sum()
    }
}

/// The auth state went from signed in (or on its way out) to signed out.
/// `Closing` counts as on the way out, so Ready then Closing then Closed
/// ends the session whether or not one poll saw all three.
fn session_ended(prev: &AuthorizationState, now: &AuthorizationState) -> bool {
    matches!(
        prev,
        AuthorizationState::Ready | AuthorizationState::LoggingOut | AuthorizationState::Closing
    ) && matches!(
        now,
        AuthorizationState::WaitPhoneNumber | AuthorizationState::Closed
    )
}

impl QuillApp {
    /// The slot the composer on screen sends from: the account on screen,
    /// the open chat (the window's own while a chat window is in scope)
    /// and its topic.
    pub(super) fn send_slot(&self) -> Option<SendSlot> {
        let account = self.session()?.account.clone();
        let chat_id = self.open_chat_id()?;
        Some(SendSlot {
            account,
            draft: self.open_draft_topic(chat_id).key(chat_id.0),
        })
    }

    /// The open slot's next send. Nothing pending when no chat is open.
    pub(super) fn pending_send(&self) -> &PendingSend {
        match self.send_slot() {
            Some(slot) => self.composer_sends.get(&slot),
            None => &NOTHING_PENDING,
        }
    }

    /// A photo of the open slot's tray is still rendering its edit.
    pub(super) fn tray_renders_an_edit(&self) -> bool {
        self.pending_send().renders_an_edit()
    }

    /// Change the open slot's next send. `None` when no chat is open.
    pub(super) fn update_pending_send<R>(
        &mut self,
        f: impl FnOnce(&mut PendingSend) -> R,
    ) -> Option<R> {
        let slot = self.send_slot()?;
        Some(self.composer_sends.update(&slot, f))
    }

    /// Change the open slot's attachment tray.
    pub(super) fn update_pending_attachments<R>(
        &mut self,
        f: impl FnOnce(&mut Vec<ComposerAttachment>) -> R,
    ) -> Option<R> {
        self.update_pending_send(|send| f(&mut send.attachments))
    }

    /// Change one attachment of the open slot's tray, now.
    pub(super) fn update_pending_attachment<R>(
        &mut self,
        index: usize,
        f: impl FnOnce(&mut ComposerAttachment) -> R,
    ) -> Option<R> {
        let item = self.tray_item(index)?;
        self.composer_sends.update_item(&item, f)
    }

    /// The open slot's tray item at `index`, for a step that answers later.
    pub(super) fn tray_item(&self, index: usize) -> Option<TrayItem> {
        let slot = self.send_slot()?;
        let path = self
            .composer_sends
            .get(&slot)
            .attachments
            .get(index)?
            .path
            .clone();
        Some(TrayItem { slot, index, path })
    }

    /// A screenshot demo starts with these in the open chat's tray.
    pub(super) fn seed_demo_attachments(&mut self, attachments: Vec<ComposerAttachment>) {
        if attachments.is_empty() {
            return;
        }
        self.update_pending_attachments(|tray| *tray = attachments);
    }

    /// The open slot's attachment tray.
    pub(super) fn pending_attachments(&self) -> &[ComposerAttachment] {
        &self.pending_send().attachments
    }

    /// The open slot's send went: it starts over (tdesktop clears the field
    /// and resets the preview draft after sending), and the tray's file
    /// checks forget the files it sent.
    pub(super) fn finish_pending_send(&mut self) {
        let Some(slot) = self.send_slot() else {
            return;
        };
        if let Some(sent) = self.composer_sends.finish(&slot) {
            self.forget_tray_checks(&sent.attachments);
        }
    }

    /// Drop the open slot's tray and the choices made for it; the schedule
    /// and link preview choice stay.
    pub(super) fn clear_pending_attachments(&mut self) {
        let dropped = self
            .update_pending_send(|send| {
                let dropped = std::mem::take(&mut send.attachments);
                send.clear_attachments();
                dropped
            })
            .unwrap_or_default();
        self.forget_tray_checks(&dropped);
    }

    /// Whether `slot` can still be sent from: its account is on screen
    /// with the chat in its list, or is signed in in the background.
    pub(super) fn send_slot_exists(&self, slot: &SendSlot) -> bool {
        match self.session() {
            Some(session) if session.account == slot.account => {
                session.chats.contains_key(&slot.draft.chat_id)
            }
            _ => self.background.client(&slot.account).is_some(),
        }
    }

    /// The account on screen left `Ready` for good (`logOut`, an expired
    /// or terminated session, TDLib closing the client): its
    /// choices go, whether or not `LoggingOut` was seen on the way.
    pub(in crate::ui) fn forget_sends_after_auth(
        &mut self,
        prev: &AuthorizationState,
        saw_logging_out: bool,
    ) {
        let Some(session) = self.session() else {
            return;
        };
        if saw_logging_out || session_ended(prev, &session.auth) {
            let account = session.account.clone();
            self.forget_account_sends(&account);
        }
    }

    /// `account` logged out or was removed: drop its choices.
    pub(in crate::ui) fn forget_account_sends(&mut self, account: &AccountKey) {
        self.composer_ui.send_box.prep.cancel_account(account);
        let dropped = self.composer_sends.forget_account(account);
        self.forget_tray_checks(&dropped);
    }

    /// These attachments left their tray: forget what this window read
    /// about their files, unless another tray still holds the same file.
    pub(super) fn forget_tray_checks(&mut self, dropped: &[ComposerAttachment]) {
        let sends = &self.composer_sends;
        self.composer_ui.send_box.forget(
            dropped
                .iter()
                .map(|attachment| attachment.path.as_path())
                .filter(|path| !sends.holds(path)),
        );
    }

    /// Whether the next send from the open chat goes without sound: the
    /// user's choice for this chat, else the chat's
    /// `default_disable_notification`.
    pub(super) fn composer_effective_silent(&self) -> bool {
        let Some(slot) = self.send_slot() else {
            return false;
        };
        let chat_id = ChatId(slot.draft.chat_id);
        self.composer_sends
            .silent_override(&slot.account, chat_id)
            .unwrap_or_else(|| self.chat_default_silent(chat_id))
    }

    /// Flip "send silently" for the open chat; stays until flipped back.
    pub(in crate::ui) fn toggle_composer_silent(&mut self) {
        let Some(slot) = self.send_slot() else {
            return;
        };
        let chat_id = ChatId(slot.draft.chat_id);
        let next = !self.composer_effective_silent();
        let chat_default = self.chat_default_silent(chat_id);
        self.composer_sends
            .set_silent(&slot.account, chat_id, next, chat_default);
    }

    fn chat_default_silent(&self, chat_id: ChatId) -> bool {
        self.session()
            .is_some_and(|session| session.sync.is_default_silent(chat_id.0))
    }
}
