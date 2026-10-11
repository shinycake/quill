//! chat-list behavior: tabs, folders, select mode, archive/pin/unread, community hub.

use super::app::QuillApp;
use super::app::{ChatListFilter, PaneMode};
use super::chat_row::PinnedChatDrag;
use super::chat_row::{
    ChatListItem, chat_list_caption, chat_list_empty_state, chat_list_skeleton_row,
    chat_row_height, chat_row_tags, static_chat_row,
};
use super::notifications::notification_settings_json;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::*;
use gpui_kit::gpui::Size as ItemSize;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::auth::AuthView;
use quill::community_mode;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::state::{ChatSummary, RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{
    AuthorizationState, ChatKind, ChatNotificationSettings, MUTE_FOREVER,
};
use quill::telegram::requests_privacy::PrivacySettingKey;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

pub(super) fn apply_ready_mute_archive(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let muted = ChatNotificationSettings::default().with_mute_for(MUTE_FOREVER);
    let jsons = [
        format!(
            r#"{{"@type":"updateChatNotificationSettings","chat_id":11,"notification_settings":{}}}"#,
            notification_settings_json(&muted)
        ),
        r#"{"@type":"updateChatPosition","chat_id":12,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"0","is_pinned":false}}"#
            .to_string(),
        r#"{"@type":"updateChatRemovedFromList","chat_id":12,"chat_list":{"@type":"chatListMain"}}"#
            .to_string(),
        r#"{"@type":"updateChatPosition","chat_id":12,"position":{"@type":"chatPosition","list":{"@type":"chatListArchive"},"order":"20","is_pinned":false}}"#
            .to_string(),
        r#"{"@type":"updateChatAddedToList","chat_id":12,"chat_list":{"@type":"chatListArchive"}}"#
            .to_string(),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// Slice CL3: chat-list screenshot fixture — chat 11 (private) carries
/// 2 unread mentions (the @ badge; the single-digit main counter hides
/// per TGX `setCounter`), chat 12 (private) carries an unread reaction
/// (the ♥ badge) and is muted + blocked so the badge dims and its row
/// menu offers Unblock. Chat 11 is reportable and unblocked, so the row
/// menu (opened on chat 11) shows Report / Block user.
/// All injected through the normal reducer, no live Telegram.
pub(super) fn apply_ready_chat_list_3(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let jsons = [
        r#"{"@type":"updateChatUnreadMentionCount","chat_id":11,"unread_mention_count":2}"#
            .to_string(),
        r#"{"@type":"updateChatUnreadReactionCount","chat_id":12,"unread_reaction_count":1}"#
            .to_string(),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    // Capability flags TDLib sends on real chats but the demo seed
    // doesn't carry — set directly like the CL1 fixture does.
    if let Some(chat) = session.chats.get_mut(&11) {
        chat.can_be_deleted_only_for_self = true;
        chat.can_be_reported = true;
    }
    if let Some(chat) = session.chats.get_mut(&12) {
        chat.blocked = true;
        chat.notification_settings =
            ChatNotificationSettings::default().with_mute_for(MUTE_FOREVER);
    }
}

pub(super) fn apply_ready_pin(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let extra = session.request(RequestPurpose::PinChatMessage, Some(ChatId(11)));
    let jsons = [
        format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
        r#"{"@type":"updateMessageIsPinned","chat_id":11,"message_id":101,"is_pinned":true}"#
            .to_string(),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

impl QuillApp {
    /// Slice CL: the long-press delay before the peek preview opens
    /// (600 ms keeps quick taps unmistakably clicks).
    pub(super) const CHAT_PREVIEW_LONG_PRESS: Duration = Duration::from_millis(600);

    pub(super) fn select_listed_chat(
        &mut self,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_chat_with(chat_id, false, window, cx);
    }

    /// Open `chat_id` as the conversation. `thread_switch` moves a resolved
    /// comment thread into its discussion group (the driver keeps the
    /// thread) instead of selecting the chat from scratch.
    pub(super) fn select_chat_with(
        &mut self,
        chat_id: ChatId,
        thread_switch: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.flush_leaving_draft(cx);
        // A reply aimed at this chat from another one survives the draft
        // restore below ("Reply in Another Chat").
        let carried_reply = self
            .composer_ui
            .pending_reply
            .clone()
            .filter(|reply| reply.target_chat == Some(chat_id));
        // Choosing a chat from the list ends the peek at it.
        self.chat_list.forum_chats_peek = false;
        // Phase B4: the TTL picker belongs to the previous chat.
        self.notify.ttl_picker_open = false;
        if self
            .composer_ui
            .pending_reply
            .as_ref()
            .is_some_and(|reply| !reply.belongs_to(chat_id))
        {
            self.composer_ui.pending_reply = None;
        }
        if self
            .composer_ui
            .pending_edit
            .as_ref()
            .is_some_and(|edit| edit.chat_id != chat_id)
        {
            self.composer_ui.pending_edit = None;
            self.composer_ui.saved_edit_draft.clear();
            self.composer_ui.saved_edit_reply = None;
        }
        if self
            .message_ui
            .pending_delete
            .as_ref()
            .is_some_and(|confirm| confirm.chat_id != chat_id)
        {
            self.message_ui.pending_delete = None;
        }
        // B4: a pending stop-poll confirm belongs to its own chat.
        if self
            .message_ui
            .pending_stop_poll
            .is_some_and(|(id, _, _)| id != chat_id)
        {
            self.message_ui.pending_stop_poll = None;
        }
        self.dismiss_forward_for_chat(chat_id);
        if self.recording_active() {
            self.cancel_recording(cx);
        }
        // Voice notes and music keep playing across chats (the player bar
        // follows, as in Telegram Desktop).
        self.stop_animation_playback();
        self.playback.autoplayed_gifs.clear();
        self.stop_sticker_playback();
        self.stop_video_playback();
        if self.gif_panel_open() {
            if let Some(live) = self.live.as_mut() {
                live.driver.close_gif_panel();
            } else if let Some(session) = self.demo_session.as_mut() {
                session.stickers.gifs.close();
            }
        }
        if self.sticker_panel_open() {
            if let Some(live) = self.live.as_mut() {
                live.driver.close_sticker_panel();
            } else if let Some(session) = self.demo_session.as_mut() {
                session.stickers.stickers.close();
            }
        }
        if self.live.is_some() {
            let driver = &mut self.live.as_mut().expect("live").driver;
            let result = if thread_switch {
                driver.switch_to_thread_chat()
            } else {
                driver.select_chat(chat_id)
            };
            self.connection.status_note = match result {
                Ok(_) => "".into(),
                Err(_) => "could not open chat".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            session.open_chat(chat_id);
        }
        self.restore_open_draft(window, cx);
        if carried_reply.is_some() {
            self.composer_ui.pending_reply = carried_reply;
        }
        let text = self.composer.read(cx).value().to_string();
        self.sync_composer_typing(&text);
        // Telegram Desktop's info column follows the open chat.
        if self
            .session()
            .is_some_and(|s| s.users_state.open_info_panel.is_some())
        {
            match self
                .session()
                .and_then(|s| s.info_panel_target_for_chat(chat_id))
            {
                Some(target) => self.open_info_panel_target(target, window, cx),
                None => self.close_info_panel(cx),
            }
        }
        cx.notify();
    }

    /// Slice G10: open the communities hub dialog (side-menu entry).
    pub(super) fn open_community_hub(&mut self, cx: &mut Context<Self>) {
        self.admin.community.hub_open = true;
        cx.notify();
    }

    pub(super) fn close_community_hub(&mut self, cx: &mut Context<Self>) {
        self.admin.community.hub_open = false;
        cx.notify();
    }

    /// Parity slice `parity:communities-chatlist-mode`: enter community
    /// chat-list mode from the hub's "View chats" button. Replaces any
    /// folder tab / category filter (like `Archived` does); fires
    /// `getCommunityFullInfo` so membership resolves for communities
    /// never opened in the info panel (deduped by the driver).
    pub(super) fn enter_community_chat_list_mode(
        &mut self,
        community_id: i64,
        cx: &mut Context<Self>,
    ) {
        self.folders.tab = None;
        self.chat_list.contacts_tab_open = false;
        self.chat_list.filter = ChatListFilter::Community(community_id);
        if let Some(live) = self.live.as_mut()
            && let Err(err) = live.driver.get_community_full_info(community_id)
        {
            self.connection.status_note = format!("community info request failed: {err:?}");
        }
        cx.notify();
    }

    /// Parity slice `parity:communities-chatlist-mode`: leave the mode
    /// (banner ✕). Selecting a folder tab or the Unread/Archived tabs
    /// also exits it — they overwrite `chat_filter`.
    pub(super) fn exit_community_chat_list_mode(&mut self, cx: &mut Context<Self>) {
        if matches!(self.chat_list.filter, ChatListFilter::Community(_)) {
            self.chat_list.filter = ChatListFilter::All;
            cx.notify();
        }
    }

    /// Parity slice `parity:communities-chatlist-mode`: the mode banner
    /// under the folder tabs — the community name plus an ✕ that exits
    /// the mode (kit `Button`, same treatment as the CL3 select bar).
    pub(super) fn community_mode_banner(
        &self,
        community_id: i64,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let name = self
            .session()
            .and_then(|s| s.groups.communities.get(&community_id))
            .map(|c| c.name.clone())
            .unwrap_or_else(|| "Community".to_string());
        div()
            .id("community-mode-banner")
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(cx.theme().accent.opacity(0.12))
            .child(
                div()
                    .flex_1()
                    .text_sm()
                    .font_semibold()
                    .child(format!("👥 {name}")),
            )
            .child(
                Button::new("community-mode-clear")
                    .label("✕")
                    .ghost()
                    .tooltip("Show all chats")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.exit_community_chat_list_mode(cx);
                    })),
            )
    }

    /// Parity slice `parity:communities-chatlist-mode`: the folder tabs
    /// plus the mode banner directly underneath when the mode is active
    /// (the banner's ✕ exits the mode).
    pub(super) fn folder_tabs_with_community_banner(
        &self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // "Tabs on the left": the folders live in the column beside the
        // list, so only the community banner stays here.
        let tabs = (!self.folder_rail_active()).then(|| self.folder_tabs(cx));
        if let ChatListFilter::Community(community_id) = self.chat_list.filter {
            div()
                .flex()
                .flex_col()
                .gap_2()
                .children(tabs)
                .child(self.community_mode_banner(community_id, cx))
                .into_any_element()
        } else {
            tabs.map(IntoElement::into_any_element)
                .unwrap_or_else(|| div().into_any_element())
        }
    }

    /// Slice CL3: multi-select mode — enter with one chat checked (from
    /// the row-menu "Select" item).
    pub(super) fn enter_select_mode(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        self.chat_list.selected.insert(chat_id.0);
        cx.notify();
    }

    /// Slice CL3: leave multi-select mode, clearing the checks.
    pub(super) fn exit_select_mode(&mut self, cx: &mut Context<Self>) {
        self.chat_list.selected.clear();
        cx.notify();
    }

    /// Slice CL3: toggle one row's check in multi-select mode; the last
    /// uncheck exits the mode.
    pub(super) fn toggle_chat_selected(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if !self.chat_list.selected.remove(&chat_id.0) {
            self.chat_list.selected.insert(chat_id.0);
        }
        cx.notify();
    }

    /// Slice CL3: "Select unread" — check every listed chat with unread
    /// messages or a marked-as-unread flag (TGX `ChatsController`
    /// select-unread), main list and archive alike.
    pub(super) fn select_unread_chats(&mut self, cx: &mut Context<Self>) {
        // Collect first: `session()` borrows `self`, so the ids must be
        // owned before touching `selected_chats`.
        let unread: Vec<i64> = self
            .session()
            .map(|session| {
                session
                    .ordered_chats()
                    .into_iter()
                    .chain(session.ordered_archived_chats())
                    .filter(|chat| chat.is_unread())
                    .map(|chat| chat.id.0)
                    .collect()
            })
            .unwrap_or_default();
        for id in unread {
            self.chat_list.selected.insert(id);
        }
        cx.notify();
    }

    /// Slice CL3: bulk pin for the selection, all-or-nothing like TGX:
    /// if any selected chat is unpinned, pin all; otherwise unpin all.
    /// Reuses `toggle_chat_pin` (with its rollback note) per chat that
    /// still needs the change.
    pub(super) fn toggle_selected_pins(&mut self, cx: &mut Context<Self>) {
        let chats = self.selected_chat_pin_states();
        let pin_all = chats.iter().any(|(_, pinned)| !pinned);
        for (id, pinned) in chats {
            if pinned != pin_all {
                self.toggle_chat_pin(id, cx);
            }
        }
        self.chat_list.selected.clear();
        cx.notify();
    }

    /// Slice CL3: bulk mark-as-read for the selection, reusing the
    /// single-chat mark-read path (`toggle_chat_marked_as_unread`
    /// does the TGX `viewMessages` flow when there is unread state).
    pub(super) fn mark_selected_read(&mut self, cx: &mut Context<Self>) {
        let ids: Vec<ChatId> = self
            .chat_list
            .selected
            .iter()
            .map(|id| ChatId(*id))
            .collect();
        for id in ids {
            // `toggle_chat_marked_as_unread` is a genuine toggle: calling it
            // on a fully-read chat would mark it *unread* — never do that
            // under a "Read" button.
            let unread = self
                .session()
                .and_then(|s| s.chats.get(&id.0))
                .is_some_and(|chat| chat.is_unread());
            if unread {
                self.toggle_chat_marked_as_unread(id, cx);
            }
        }
        self.chat_list.selected.clear();
        cx.notify();
    }

    /// Slice CL3: bulk mute for the selection, all-or-nothing like TGX:
    /// if any selected chat is unmuted, mute all; otherwise unmute all.
    /// Reuses `apply_chat_mute` (mute-forever / unmute, like the row menu)
    /// per chat that still needs the change.
    pub(super) fn toggle_selected_mute(&mut self, cx: &mut Context<Self>) {
        let chats: Vec<(ChatId, bool)> = self
            .chat_list
            .selected
            .iter()
            .map(|&id| {
                let muted = self
                    .session()
                    .and_then(|s| s.chats.get(&id))
                    .is_some_and(|chat| chat.is_muted());
                (ChatId(id), muted)
            })
            .collect();
        let mute_all = chats.iter().any(|(_, muted)| !muted);
        for (id, muted) in chats {
            if muted != mute_all {
                self.apply_chat_mute(id, if mute_all { MUTE_FOREVER } else { 0 }, cx);
            }
        }
        self.chat_list.selected.clear();
        cx.notify();
    }

    /// Slice CL3: bulk archive for the selection, all-or-nothing like
    /// TGX: if any selected chat is unarchived, archive all; otherwise
    /// unarchive all. Reuses `toggle_archive` per chat that still needs
    /// the change.
    pub(super) fn toggle_selected_archive(&mut self, cx: &mut Context<Self>) {
        let chats: Vec<(ChatId, bool)> = self
            .chat_list
            .selected
            .iter()
            .map(|&id| {
                let archived = self
                    .session()
                    .and_then(|s| s.chats.get(&id))
                    .is_some_and(|chat| chat.in_archive);
                (ChatId(id), archived)
            })
            .collect();
        let archive_all = chats.iter().any(|(_, archived)| !archived);
        for (id, archived) in chats {
            if archived != archive_all {
                self.toggle_archive(id, cx);
            }
        }
        self.chat_list.selected.clear();
        cx.notify();
    }

    /// Slice CL3: per-selected-chat pin state, honoring the pinned flag
    /// of the list the chat actually sits in (main vs archive).
    pub(super) fn selected_chat_pin_states(&self) -> Vec<(ChatId, bool)> {
        self.chat_list
            .selected
            .iter()
            .map(|&id| {
                let pinned = self
                    .session()
                    .and_then(|s| s.chats.get(&id))
                    .is_some_and(|chat| {
                        if chat.in_archive {
                            chat.archive_is_pinned
                        } else {
                            chat.is_pinned
                        }
                    });
                (ChatId(id), pinned)
            })
            .collect()
    }

    /// Slice CL3: bulk delete — confirms first, like the single-chat
    /// row-menu path.
    pub(super) fn delete_selected_chats(&mut self, cx: &mut Context<Self>) {
        // The dialog needs a chat id; the ids to delete are read from
        // the live selection at submit time.
        if let Some(first) = self.chat_list.selected.iter().next().map(|id| ChatId(*id)) {
            self.open_group_confirm(first, GroupConfirmAction::RemoveSelectedChats, cx);
        }
    }

    /// Sidebar "Chats | Contacts | Calls" tabs (Ready mode). Other modes
    /// keep the plain "Chats" title.
    /// kit Phase 3: the Chats/Contacts/Calls strip as a kit segmented
    /// `TabBar` — same three tabs and handlers as before.
    pub(super) fn list_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        if self.pane_mode() != PaneMode::Ready {
            return div().font_semibold().child("Chats").into_any_element();
        }
        let selected = if self.chat_list.contacts_tab_open {
            1
        } else if self.chat_list.calls_tab_open {
            2
        } else {
            0
        };
        let weak = cx.weak_entity();
        TabBar::new("list-tabs")
            .segmented()
            .selected_index(selected)
            .child(Tab::new().label("Chats"))
            .child(Tab::new().label("Contacts"))
            // Phase C2i: Recent-calls tab (server-side `searchCallMessages`
            // history + call settings).
            .child(Tab::new().label("Calls"))
            .on_click(move |ix, _window, cx| {
                let _ = weak.update(cx, |this, cx| match ix {
                    0 => this.open_chats_tab(cx),
                    1 => this.open_contacts_tab(cx),
                    _ => this.open_calls_tab(cx),
                });
            })
            .into_any_element()
    }

    pub(super) fn open_chats_tab(&mut self, cx: &mut Context<Self>) {
        self.chat_list.contacts_tab_open = false;
        self.chat_list.calls_tab_open = false;
        cx.notify();
    }

    pub(super) fn open_contacts_tab(&mut self, cx: &mut Context<Self>) {
        self.chat_list.contacts_tab_open = true;
        self.chat_list.calls_tab_open = false;
        // Slice A6: the sync toggle gates the `getContacts` refresh —
        // with sync off the tab shows the last loaded snapshot.
        let sync_on = self
            .session()
            .map(|s| s.settings.contact_prefs.sync_enabled)
            .unwrap_or(true);
        if sync_on
            && let Some(live) = self.live.as_mut()
            && let Err(err) = live.driver.fetch_contacts()
        {
            self.connection.status_note = format!("contacts request failed: {err:?}");
        }
        cx.notify();
    }

    /// Phase C2i: open the Recent-calls tab — first page of the
    /// server-side call history plus both call privacy settings. Demo
    /// mode injects synthetic data instead (screenshot proof).
    pub(super) fn open_calls_tab(&mut self, cx: &mut Context<Self>) {
        self.chat_list.contacts_tab_open = false;
        self.chat_list.calls_tab_open = true;
        if let Some(live) = self.live.as_mut() {
            if let Err(err) = live.driver.fetch_call_history() {
                self.connection.status_note = format!("call history request failed: {err:?}");
            }
            for key in [PrivacySettingKey::AllowCalls, PrivacySettingKey::PeerToPeer] {
                if let Err(err) = live.driver.fetch_privacy_rules(key) {
                    self.connection.status_note = format!("call privacy request failed: {err:?}");
                }
            }
            // Slice S4: the call-settings "Use less data for calls"
            // toggle reads the TDLib-backed per-network settings —
            // fetch the presets so it shows the true value (guarded:
            // once per session).
            let _ = live.driver.fetch_auto_download_presets();
        }
        cx.notify();
    }

    pub(super) fn open_folder_tab(&mut self, folder: Option<i32>, cx: &mut Context<Self>) {
        self.folders.tab = folder;
        self.chat_list.filter = ChatListFilter::All;
        self.chat_list.contacts_tab_open = false;
        if let (Some(live), Some(folder_id)) = (self.live.as_mut(), folder)
            && let Err(err) = live.driver.load_folder_chats(folder_id)
        {
            self.connection.status_note = format!("folder load failed: {err:?}");
        }
        // A shared folder: has its owner added chats?
        self.poll_folder_new_chats();
        cx.notify();
    }

    /// Pinned drag, pointer moved (tdesktop `updateReorderPinned`): the
    /// first move starts the reorder, later ones swap rows live.
    pub(super) fn pin_drag_move(&mut self, drag: &PinnedChatDrag, y: f32, cx: &mut Context<Self>) {
        let now = std::time::Instant::now();
        if let Some(reorder) = self.chat_list.pin_reorder.as_mut()
            && reorder.dragging() == Some(drag.chat_id.0)
        {
            reorder.drag_to(y, now);
        } else {
            // tdesktop `kStartReorderThreshold`: the pointer must travel
            // 30px vertically before the press becomes a reorder.
            let anchor = match self.chat_list.pin_drag_anchor {
                Some((id, anchor)) if id == drag.chat_id.0 => anchor,
                _ => {
                    self.chat_list.pin_drag_anchor = Some((drag.chat_id.0, y));
                    return;
                }
            };
            if (y - anchor).abs() < quill::pin_reorder::START_THRESHOLD {
                return;
            }
            let Some(session) = self.session() else {
                return;
            };
            let ids = session.pinned_chat_ids(drag.archived);
            let names: Vec<(i32, String, i32)> = session
                .chat_list
                .chat_folders
                .iter()
                .map(|f| (f.id, f.name.clone(), f.color_id))
                .collect();
            let tags = session.chat_list.are_folder_tags_enabled;
            let viewing = self.folders.tab;
            let lines = self.appearance.preview_lines;
            let heights = ids
                .iter()
                .filter_map(|id| {
                    let chat = session.chats.get(id)?;
                    let height =
                        chat_row_height(&chat_row_tags(chat, &names, tags, viewing), lines);
                    Some((*id, f32::from(height)))
                })
                .collect();
            self.chat_list.pin_reorder_archived = drag.archived;
            self.chat_list.pin_reorder =
                quill::pin_reorder::PinReorder::begin(ids, heights, drag.chat_id.0, y);
        }
        self.notify_sidebar(cx);
    }

    /// Pinned drag released (tdesktop `finishReorderOnRelease` +
    /// `savePinnedOrder`): the dragged row slides into its slot and the
    /// full order goes out as `setPinnedChats`.
    pub(super) fn finish_pin_drag(&mut self, cx: &mut Context<Self>) {
        self.chat_list.pin_drag_anchor = None;
        let archived = self.chat_list.pin_reorder_archived;
        let Some(reorder) = self.chat_list.pin_reorder.as_mut() else {
            return;
        };
        if reorder.dragging().is_none() {
            return;
        }
        let ids = reorder.release(std::time::Instant::now());
        if let Some(live) = self.live.as_mut() {
            if let Err(err) = live.driver.set_pinned_chat_order(archived, ids) {
                self.connection.status_note = format!("pin reorder failed: {err:?}");
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.reorder_pinned_chats(archived, &ids);
        }
        cx.notify();
    }

    /// Slice CL2: "Mark all as read" for a chat list — `readChatList`
    /// (schema 1.8.67, line 13684). The badges clear via the server
    /// updates; nothing is faked locally.
    pub(super) fn mark_all_chats_as_read(&mut self, archived: bool, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if let Err(err) = live.driver.mark_all_chats_as_read(archived) {
                self.connection.status_note = format!("mark all read failed: {err:?}");
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            // Demo: clear the badges directly so the fixture shows the
            // result (no server sends updates in a screenshot demo).
            for chat in session.chats.values_mut() {
                let in_list = if archived {
                    chat.in_archive
                } else {
                    chat.in_main_list
                };
                if in_list {
                    chat.unread_count = 0;
                    chat.is_marked_as_unread = false;
                }
            }
        }
        cx.notify();
    }

    /// Slice CL2: Saved Messages entry — opens the private chat with
    /// yourself (schema 1.8.67, line 9590: "Call createPrivateChat
    /// with getOption(\"my_id\") and open the chat"). An already-listed
    /// self chat opens directly; otherwise `createPrivateChat` is sent
    /// and its answer opens the chat.
    pub(super) fn open_saved_messages(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let self_chat = self.session().and_then(|s| {
            let my_id = s.my_user_id?;
            s.chats
                .values()
                .find(|c| matches!(c.kind, ChatKind::Private { user_id } if user_id.0 == my_id))
                .map(|c| c.id)
        });
        if let Some(chat_id) = self_chat {
            self.select_listed_chat(chat_id, window, cx);
            return;
        }
        if self.live.is_some() {
            // Flush the open chat's composer before switching away — the
            // answer arrives asynchronously, so the driver can't do it.
            self.flush_leaving_draft(cx);
        }
        if let Some(live) = self.live.as_mut() {
            match live.driver.create_private_chat_with_self() {
                Ok(Some(_)) => {}
                Ok(None) if live.driver.session.my_user_id.is_none() => {
                    self.connection.status_note = "account info not loaded yet".into();
                }
                Ok(None) => {}
                Err(err) => self.connection.status_note = format!("saved messages failed: {err:?}"),
            }
        } else {
            self.connection.status_note = "no Saved Messages chat in this demo".into();
        }
        cx.notify();
    }

    pub(super) fn toggle_archive(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let archived = self
            .session()
            .and_then(|session| session.chats.get(&chat_id.0))
            .is_some_and(|chat| chat.in_archive);
        if self.live.is_some() {
            let result = if archived {
                self.live
                    .as_mut()
                    .expect("live")
                    .driver
                    .unarchive_chat(chat_id)
            } else {
                self.live
                    .as_mut()
                    .expect("live")
                    .driver
                    .archive_chat(chat_id)
            };
            self.connection.status_note = match result {
                Ok(_) if archived => "unarchiving…".into(),
                Ok(_) => "archiving…".into(),
                Err(_) => "could not change archive".into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_archive(chat_id, !archived);
            self.connection.status_note = if archived {
                "unarchived".into()
            } else {
                "archived".into()
            };
            cx.notify();
        }
    }

    // ------------------------------------------------------------------
    // Parity slice: folder management (create / edit / delete / reorder /
    // tags / per-chat membership).
    // ------------------------------------------------------------------
}

impl QuillApp {
    /// Keyboard chat navigation: open the chat `step` rows away from the
    /// open one in the list as currently shown (folder, filter and archive
    /// state included), wrapping at the ends. With no chat open, starts
    /// at the top.
    pub(super) fn step_open_chat(
        &mut self,
        step: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ids: Vec<ChatId> = self
            .chat_list
            .items
            .iter()
            .filter_map(|item| match item {
                ChatListItem::Chat { id, .. } => Some(*id),
                _ => None,
            })
            .collect();
        if ids.is_empty() {
            return;
        }
        let open = self.session().and_then(|s| s.open_chat);
        let len = ids.len() as isize;
        let next = match open.and_then(|open| ids.iter().position(|id| *id == open)) {
            Some(index) => (index as isize + step).rem_euclid(len),
            None if step >= 0 => 0,
            None => len - 1,
        };
        self.select_listed_chat(ids[next as usize], window, cx);
    }
}

mod toggle_chat_pin;
