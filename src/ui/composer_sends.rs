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
use super::composer::LeftSend;
use super::composer_state::{DraftClear, Reservation};
use quill::composer::{
    ComposerAttachment, ComposerScheduling, DraftKey, LinkPreviewChoice, PreviewMediaSize,
    RenderToken,
};
use quill::ids::{AccountKey, ChatId, MessageId, RequestId};
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
    /// MED1: the "group media" choice for this tray's 2+ files; `None`
    /// follows `media_prefs.default_grouping()`. It belongs to the tray, so
    /// a choice made for one chat does not group another chat's files.
    pub(super) group_media: Option<bool>,
    /// Stars to unlock the media; 0 sends a normal message.
    pub(super) paid_stars: i64,
    /// `messageSchedulingState*` for the next send.
    pub(super) scheduling: ComposerScheduling,
    /// The link preview choice (tdesktop keeps it in the draft as
    /// `WebPageDraft`).
    pub(super) preview: LinkPreviewChoice,
    /// A Send pressed while a photo edit was rendering.
    pub(super) send: TraySend,
}

/// Whether the tray's Send waits for the photo edits still rendering.
#[derive(Debug, PartialEq)]
pub(super) enum TraySend {
    Idle,
    /// The text of the Send pressed while photo edits were rendering, with
    /// that send's one-time options and the renders it waits on: it goes
    /// out when the last of them lands. It holds only while one of them
    /// still renders in the tray ([`PendingSend::send_is_held`]), so a
    /// tray that lost them (a photo removed, the tray cleared) leaves an
    /// inert value that nothing reads and no later render revives.
    Held {
        text: String,
        once: SendOnce,
        waiting: Vec<RenderToken>,
    },
}

/// What a finished render left in its tray.
#[derive(Default)]
pub(super) struct Landed {
    /// The held Send and its one-time options, once nothing in that tray
    /// renders.
    pub(super) held: Option<(String, SendOnce)>,
    /// The photo's file now, while it is still in the tray.
    pub(super) path: Option<PathBuf>,
}

/// The options a single send carries and the next does not: "Send without
/// sound" and "Send with Effect". They are an argument of the send path
/// (`submit_composer`), never a field. A send that waits (a paid-message
/// confirm, a photo edit still rendering) keeps its own copy, so it goes
/// out with them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct SendOnce {
    pub(super) silent: bool,
    pub(super) effect: i64,
}

impl PendingSend {
    const EMPTY: Self = Self {
        attachments: Vec::new(),
        self_destruct: None,
        caption_above: false,
        hd_photos: false,
        group_media: None,
        paid_stars: 0,
        scheduling: ComposerScheduling::None,
        preview: LinkPreviewChoice {
            disabled: false,
            above_text: false,
            media: PreviewMediaSize::Auto,
            link_index: 0,
        },
        send: TraySend::Idle,
    };

    /// Drop the tray and the choices that only make sense with it.
    pub(super) fn clear_attachments(&mut self) {
        self.attachments.clear();
        self.self_destruct = None;
        self.caption_above = false;
        self.hd_photos = false;
        self.group_media = None;
        self.paid_stars = 0;
    }

    /// Some photo of the tray is still rendering its edit.
    pub(super) fn renders_an_edit(&self) -> bool {
        self.attachments
            .iter()
            .any(|attachment| attachment.extras.photo_render().is_some())
    }

    /// The renders in flight in this tray.
    fn renders(&self) -> Vec<RenderToken> {
        self.attachments
            .iter()
            .filter_map(|attachment| attachment.extras.photo_render().cloned())
            .collect()
    }

    /// Hold a Send until the renders land. Without a render there is
    /// nothing to wait for: it is refused, and the caller sends at once.
    pub(super) fn hold_send(&mut self, text: String, once: SendOnce) -> bool {
        let waiting = self.renders();
        if waiting.is_empty() {
            return false;
        }
        self.send = TraySend::Held {
            text,
            once,
            waiting,
        };
        true
    }

    /// The render `render` of a tray photo ended: `saved` is the new file,
    /// `None` when it failed. The held Send goes out once no render is
    /// left, and a failed render of one it waits on drops it (it would
    /// carry the unedited photo).
    pub(super) fn end_render(
        &mut self,
        render: &RenderToken,
        saved: Option<&Path>,
    ) -> Option<Landed> {
        let waited = self.send_is_held();
        let attachment = self
            .attachments
            .iter_mut()
            .find(|attachment| attachment.extras.photo_render() == Some(render))?;
        let Some(saved) = saved else {
            attachment.extras.abandon_render();
            let path = attachment.path.clone();
            if self.waits_on(render) {
                self.send = TraySend::Idle;
            }
            return Some(Landed {
                held: None,
                path: Some(path),
            });
        };
        let stem = attachment
            .path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("photo")
            .to_string();
        attachment.path = saved.to_path_buf();
        attachment.extras.land_render();
        attachment.file_name = format!("{stem}.png");
        attachment.kind = quill::composer::AttachmentKind::Photo;
        let held = (waited && !self.renders_an_edit())
            .then(|| self.take_held_send())
            .flatten();
        Some(Landed {
            held,
            path: Some(saved.to_path_buf()),
        })
    }

    /// The held Send, if it holds, for the tray to send.
    fn take_held_send(&mut self) -> Option<(String, SendOnce)> {
        match std::mem::replace(&mut self.send, TraySend::Idle) {
            TraySend::Held { text, once, .. } => Some((text, once)),
            TraySend::Idle => None,
        }
    }

    /// Whether the held Send waits on the render `render`.
    fn waits_on(&self, render: &RenderToken) -> bool {
        matches!(&self.send, TraySend::Held { waiting, .. } if waiting.contains(render))
    }

    /// A Send is held for the renders: one it waits on still renders.
    pub(super) fn send_is_held(&self) -> bool {
        match &self.send {
            TraySend::Idle => false,
            TraySend::Held { waiting, .. } => self.attachments.iter().any(|attachment| {
                attachment
                    .extras
                    .photo_render()
                    .is_some_and(|render| waiting.contains(render))
            }),
        }
    }

    /// Nothing is chosen, so the entry can go. A held Send that no longer
    /// holds counts as nothing.
    fn is_empty(&self) -> bool {
        let Self {
            attachments,
            self_destruct,
            caption_above,
            hd_photos,
            group_media,
            paid_stars,
            scheduling,
            preview,
            send: _,
        } = self;
        attachments.is_empty()
            && self_destruct.is_none()
            && !caption_above
            && !hd_photos
            && group_media.is_none()
            && *paid_stars == 0
            && *scheduling == Self::EMPTY.scheduling
            && *preview == Self::EMPTY.preview
            && !self.send_is_held()
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
    /// The sends whose acknowledgement clears their slot's draft, one per
    /// slot, whichever window sent them (a slot is one chat and topic of
    /// this account). An entry goes with its ack; a newer send of the slot
    /// replaces it; the account's goes with it.
    draft_clears: HashMap<DraftKey, DraftClear>,
    /// Rich sends of chat views since replaced, still waiting for TDLib's
    /// answer (`LeftSend`); each goes with its answer.
    left_sends: Vec<LeftSend>,
}

/// How many left sends an account keeps waiting; an older one is forgotten.
const KEPT_LEFT_SENDS: usize = 8;

impl AccountSends {
    fn is_empty(&self) -> bool {
        self.slots.is_empty()
            && self.silent.is_empty()
            && self.draft_clears.is_empty()
            && self.left_sends.is_empty()
    }
}

/// Per-chat send state for every chat and topic the user touched, per
/// account. An entry that is back to nothing chosen is dropped.
#[derive(Default)]
pub(crate) struct ComposerSends {
    accounts: HashMap<AccountKey, AccountSends>,
    /// The media sends being prepared, of every window and account
    /// (`send_prep`): one guard per slot whichever window shows it.
    pub(super) prep: super::send_prep::SendPrepState,
    /// What was read about the files of every tray (`send_box_state`).
    pub(super) reads: super::send_box_state::TrayReads,
}

impl ComposerSends {
    /// A recording's Send was tapped: the slot's draft clear is held for
    /// it until its file is ready (`arm_draft_clear`), replacing the
    /// earlier send's, as any send of the slot does. A send made after the
    /// tap replaces this one, so the recording's ack can never clear what
    /// that send owns.
    pub(super) fn reserve_draft_clear(&mut self, slot: &SendSlot, chat: ChatId) -> Reservation {
        let claim = Reservation::new();
        self.set_draft_clear(slot, Some(DraftClear::reserved(chat, claim.clone())));
        claim
    }

    /// The recording `claim` was sent as `request`: its clear waits for the
    /// ack, unless a newer send of the slot took it over meanwhile.
    pub(super) fn arm_draft_clear(
        &mut self,
        slot: &SendSlot,
        claim: &Reservation,
        request: RequestId,
    ) {
        if let Some(clear) = self
            .accounts
            .get_mut(&slot.account)
            .and_then(|account| account.draft_clears.get_mut(&slot.draft))
        {
            clear.arm(claim, request);
        }
    }

    /// The recording `claim` was not sent: its held clear goes, and a newer
    /// send's stays.
    pub(super) fn release_draft_clear(&mut self, slot: &SendSlot, claim: &Reservation) {
        let held = self
            .accounts
            .get(&slot.account)
            .and_then(|account| account.draft_clears.get(&slot.draft))
            .is_some_and(|clear| clear.is_reserved_by(claim));
        if held {
            self.set_draft_clear(slot, None);
        }
    }

    /// A send whose chat view went waits on its own. Returns the oldest one
    /// when the account holds too many: the caller forgets its ack.
    pub(super) fn keep_left_send(&mut self, send: LeftSend) -> Option<LeftSend> {
        let sends = self.accounts.entry(send.slot.account.clone()).or_default();
        sends.left_sends.push(send);
        (sends.left_sends.len() > KEPT_LEFT_SENDS).then(|| sends.left_sends.remove(0))
    }

    /// The left sends of `account` whose answer `answer_of` has: each with
    /// its answer, no longer waiting.
    pub(super) fn take_left_sends(
        &mut self,
        account: &AccountKey,
        mut answer_of: impl FnMut(RequestId) -> Option<quill::state::Answer>,
    ) -> Vec<(LeftSend, quill::state::Answer)> {
        let Some(sends) = self.accounts.get_mut(account) else {
            return Vec::new();
        };
        let mut answered = Vec::new();
        let mut waiting = Vec::new();
        for send in std::mem::take(&mut sends.left_sends) {
            match answer_of(send.request) {
                Some(answer) => answered.push((send, answer)),
                None => waiting.push(send),
            }
        }
        sends.left_sends = waiting;
        self.drop_if_empty(account);
        answered
    }

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
        send.attachments
            .get_mut(item.index)
            .filter(|attachment| attachment.path == item.path)
            .map(f)
    }

    /// Whether `item` is still in its tray, as the same file.
    pub(super) fn holds_item(&self, item: &TrayItem) -> bool {
        self.get(&item.slot)
            .attachments
            .get(item.index)
            .is_some_and(|attachment| attachment.path == item.path)
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

    /// A send of `slot` was made: its ack clears the slot's draft. `None`
    /// when the send leaves the draft alone, which also drops an earlier
    /// send's entry of the slot.
    pub(super) fn set_draft_clear(&mut self, slot: &SendSlot, clear: Option<DraftClear>) {
        match clear {
            Some(clear) => {
                self.accounts
                    .entry(slot.account.clone())
                    .or_default()
                    .draft_clears
                    .insert(slot.draft, clear);
            }
            None => {
                if let Some(account) = self.accounts.get_mut(&slot.account) {
                    account.draft_clears.remove(&slot.draft);
                }
                self.drop_if_empty(&slot.account);
            }
        }
    }

    /// A send was answered: `request` made the temporary message `made`.
    /// The pending clear that holds the request learns the message.
    pub(super) fn note_send_answer(
        &mut self,
        account: &AccountKey,
        request: RequestId,
        made: MessageId,
    ) {
        if let Some(sends) = self.accounts.get_mut(account) {
            for clear in sends.draft_clears.values_mut() {
                clear.answered(request, made);
            }
        }
    }

    /// The clear for the ack of `message` in `account`, with the slot it
    /// waits in: the send that made the message (the ack names the
    /// message's own topic, which can differ from the composer's slot, as
    /// for a reply from a supergroup's main composer). Only the first ack
    /// of a send returns its clear; the entry stays until the send's last
    /// message has acked, so a later ack of an album finds it by id. A
    /// send that named no request (a menu command) is found by the slot
    /// the ack names, once. Never another send's.
    pub(super) fn take_draft_clear(
        &mut self,
        account: &AccountKey,
        message: MessageId,
        ack_slot: DraftKey,
    ) -> Option<(DraftKey, DraftClear)> {
        let sends = self.accounts.get_mut(account)?;
        let by_id = sends
            .draft_clears
            .iter()
            .find_map(|(slot, clear)| clear.made(message).then_some(*slot));
        let taken = match by_id {
            Some(slot) => {
                let clear = sends.draft_clears.get_mut(&slot);
                let taken = clear.map(|clear| {
                    let before = clear.clone();
                    (clear.ack(message), before, clear.is_settled())
                });
                taken.and_then(|(clears, before, settled)| {
                    if settled {
                        sends.draft_clears.remove(&slot);
                    }
                    clears.then_some((slot, before))
                })
            }
            None => sends
                .draft_clears
                .get(&ack_slot)
                .is_some_and(DraftClear::is_unasked)
                .then(|| sends.draft_clears.remove(&ack_slot))
                .flatten()
                .map(|clear| (ack_slot, clear)),
        };
        self.drop_if_empty(account);
        taken
    }

    /// The send of `message` failed: its draft stays, and no later ack of
    /// the same send (the rest of an album) clears it.
    pub(super) fn fail_draft_clear(
        &mut self,
        account: &AccountKey,
        message: MessageId,
        ack_slot: DraftKey,
    ) {
        let Some(sends) = self.accounts.get_mut(account) else {
            return;
        };
        let by_id = sends
            .draft_clears
            .iter()
            .find_map(|(slot, clear)| clear.made(message).then_some(*slot));
        match by_id {
            Some(slot) => {
                if let Some(clear) = sends.draft_clears.get_mut(&slot) {
                    clear.fail(message);
                    if clear.is_settled() {
                        sends.draft_clears.remove(&slot);
                    }
                }
            }
            None => {
                if sends
                    .draft_clears
                    .get(&ack_slot)
                    .is_some_and(DraftClear::is_unasked)
                {
                    sends.draft_clears.remove(&ack_slot);
                }
            }
        }
        self.drop_if_empty(account);
    }

    /// Whether `slot` has a send waiting for its ack to clear the draft.
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn has_draft_clear(&self, slot: &SendSlot) -> bool {
        self.accounts
            .get(&slot.account)
            .is_some_and(|account| account.draft_clears.contains_key(&slot.draft))
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
    /// The send just made from the composer in place clears its slot's draft
    /// when it is acknowledged (`finish_successful_sends`).
    pub(super) fn expect_draft_clear(&mut self, clear: DraftClear, request: RequestId) {
        if let Some(slot) = self.send_slot() {
            self.composer_sends
                .set_draft_clear(&slot, Some(clear.asked([request])));
        }
    }

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
        self.close_send_box_emoji();
        if let Some(sent) = self.composer_sends.finish(&slot) {
            self.forget_tray_checks(&sent.attachments);
        }
        // An editor still open on a photo of the tray that went would save
        // into nothing.
        let on_sent_tray = self.viewer.photo_editor.as_ref().is_some_and(|editor| {
            matches!(&editor.purpose, super::photo_editor::EditorPurpose::Attachment(item) if item.slot == slot)
        });
        if on_sent_tray {
            self.viewer.photo_editor.take();
        }
    }

    /// Drop the open slot's tray and the choices made for it; the schedule
    /// and link preview choice stay.
    pub(super) fn clear_pending_attachments(&mut self) {
        self.close_send_box_emoji();
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

    /// `account` logged out or was removed: drop its choices, and all it
    /// kept outside TDLib (`quill::account_data::forget`: agent grants and
    /// log, mini app trust, hashtags, and the rest), as tdesktop's
    /// `Storage::Account::reset` does.
    pub(in crate::ui) fn forget_account_sends(&mut self, account: &AccountKey) {
        // Its mini app's helper has the web view data open: it closes
        // first, and the data goes once the helper is gone.
        let helper_gone = self.close_mini_app_for_forget(account);
        if let Some(root) = self.accounts_root() {
            quill::account_data::forget_after_helper(&root, account, helper_gone);
        }
        self.screen.recent_hashtags.forget(account);
        self.delivery.forget_account(account);
        self.forget_widget_storage(account);
        self.composer_sends.prep.cancel_account(account);
        let dropped = self.composer_sends.forget_account(account);
        self.forget_tray_checks(&dropped);
    }

    /// These attachments left their tray: forget what was read
    /// about their files, unless another tray still holds the same file.
    pub(super) fn forget_tray_checks(&mut self, dropped: &[ComposerAttachment]) {
        let sends = &mut self.composer_sends;
        let gone: Vec<&std::path::Path> = dropped
            .iter()
            .map(|attachment| attachment.path.as_path())
            .filter(|path| !sends.holds(path))
            .collect();
        sends.reads.forget(gone.into_iter());
    }

    /// Whether the next send from the open chat goes without sound: the
    /// user's choice for this chat, else the chat's
    /// `default_disable_notification`.
    pub(super) fn composer_effective_silent(&self) -> bool {
        let Some(slot) = self.send_slot() else {
            return false;
        };
        self.chat_sends_silent(ChatId(slot.draft.chat_id))
    }

    /// Whether a send to `chat_id` of the account on screen goes without
    /// sound: the user's "send silently" choice for that chat, else its
    /// `default_disable_notification`.
    pub(in crate::ui) fn chat_sends_silent(&self, chat_id: ChatId) -> bool {
        let Some(account) = self.session().map(|session| &session.account) else {
            return false;
        };
        self.composer_sends
            .silent_override(account, chat_id)
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
