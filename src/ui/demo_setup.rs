//! per-demo screenshot setup methods carved from new_with_demo (pure code motion).

use super::app::QuillApp;
use super::audio_playback::{apply_ready_audio, apply_ready_voice};
use super::bots::{
    apply_ready_bot_chat, apply_ready_bot_command_menu, apply_ready_bot_keyboard,
    apply_ready_bot_profile, apply_ready_rich_message,
};
use super::calls::apply_ready_call;
use super::calls::{
    apply_ready_call_swap, apply_ready_call_video, apply_ready_calls_settings,
    apply_ready_group_call, apply_ready_group_call_invitation, apply_ready_group_call_invite,
    apply_ready_group_call_manage, apply_ready_group_call_scheduled,
};
use super::chat::apply_ready_slow_mode;
use super::chat_list::{
    apply_ready_chat_avatars, apply_ready_chat_list, apply_ready_chat_list_3,
    apply_ready_chat_list_menu, apply_ready_chat_preview, apply_ready_chat_rows,
    apply_ready_mute_archive, apply_ready_pin,
};
use super::chat_row::ChatPreviewState;
use super::chatlist_demo::{
    apply_ready_archive_row, apply_ready_join_bar, apply_ready_multiline_rows,
    apply_ready_search_previews,
};
use super::composer::apply_ready_reply;
use super::composer_ui::apply_ready_stickers;
use super::contacts::apply_ready_contacts;
use super::conversation::apply_ready_typing;
use super::demo::demo_video_frame;
use super::demo::{
    demo_media_allowlist, demo_password_state_manage, demo_password_state_pending, demo_sessions,
    demo_star_subscriptions, demo_storage_stats, demo_thumb_png_path, demo_websites,
};
use super::drafts::apply_ready_drafts;
use super::find_demo::{
    apply_ready_jump_date, apply_ready_search_filters, apply_ready_search_frequent,
    apply_ready_search_from, apply_ready_search_from_hits, apply_ready_search_public,
};
use super::folders::apply_ready_folders;
use super::forward::apply_ready_forward;
use super::group_calls::demo_group_video_frames;
use super::group_invites::{
    apply_ready_admin_log, apply_ready_admin_management, apply_ready_invite_links,
};
use super::groups::{
    apply_ready_channels, apply_ready_channels_admin, apply_ready_group_manage, apply_ready_groups2,
};
use super::groups_forum::{apply_ready_forum_topics, apply_ready_topic_post};
use super::history::apply_ready_albums;
use super::inline_playback::{apply_ready_gifs, apply_ready_video, apply_ready_video_note};
use super::inline_playback::{
    apply_ready_video_note_send, apply_ready_video_send, apply_ready_video_viewer,
};
use super::message_games::apply_ready_game_card;
use super::message_media::{apply_ready_dice, apply_ready_location};
use super::message_text::{
    apply_ready_blockquote_expandable, apply_ready_caption_position, apply_ready_link_preview,
    apply_ready_preview_cards, apply_ready_text_entities,
};
use super::notification_settings::apply_ready_notification_sound;
use super::payments::apply_ready_payments;
use super::polls::apply_ready_poll;
use super::profile::apply_ready_profile_edit;
use super::reactions::apply_ready_reactions;
use super::screenshot_demo::ScreenshotDemo;
use super::search_ui::{apply_ready_search, apply_ready_search_in_chat};
use super::secret_chats::{apply_ready_chat_ttl, apply_ready_key_verification};
use super::secret_chats::{apply_ready_secret_chat, apply_ready_self_destruct};
use super::shared_media::apply_ready_shared_media;
use super::sponsored::apply_ready_sponsored;
use super::statistics::apply_ready_channel_stats;
use super::story_composer::{apply_ready_story_edit, apply_ready_story_post};
use super::story_viewer::{apply_ready_stories, apply_ready_story_viewers};
use super::subsection_tabs::apply_ready_bot_topics;
use super::*;
use gpui_kit::*;
use quill::composer::{ComposerEdit, ComposerReplyTo, DeleteConfirm, ForwardDraft};
use quill::ids::{ChatId, FileId, MessageId};
use quill::settings::ThemeChoice;
use quill::state::{InfoPanelTarget, SearchStatus};
use quill::story_composer::{StoryExpiry, StoryPrivacy};
use quill::subsection_tabs::SubsectionTabsMode;
use quill::telegram::envelope::UsernameCheckResult;
use quill::telegram::requests::SelfDestructSend;
use quill::voice::VoiceCapture;
use std::sync::atomic::Ordering;
impl QuillApp {
    /// Screenshot-demo fixture setup (composer): applies the `composer` demo
    /// mutations carved out of `new_with_demo`. Pure code motion.
    pub(super) fn demo_setup_composer(
        &mut self,
        demo: Option<ScreenshotDemo>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(demo, Some(ScreenshotDemo::ReadyDeepLinkInfo)) {
            self.deep_link_dialog = Some("This link requires a newer version of Telegram. Please update your app to open it.".into());
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyDeepLinkShare)) {
            self.share_link_text = Some("https://example.com/article\nWorth a look".into());
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyDeepLinkInvite)) {
            self.deep_link_invite = Some(quill::state::DeepLinkState::InvitePreview {
                hash: "demo_invite".into(),
                title: "Rust Community".into(),
                member_count: 1248,
                creates_join_request: true,
                is_channel: false,
                generation: 1,
            });
        }
        if matches!(
            demo,
            Some(ScreenshotDemo::ReadySpellcheck | ScreenshotDemo::ReadySpellcheckPanel)
        ) {
            self.chat_prefs.spellcheck_enabled = true;
            // Ignore persisted app words so this fixture always shows typos.
            self.spellchecker = Self::new_spellchecker(false).0;
            // Off macOS the fixture must not depend on the host's dictionaries.
            #[cfg(not(target_os = "macos"))]
            {
                self.spellchecker =
                    std::sync::Arc::new(quill::spellcheck::SpellChecker::wordlist());
            }
            // `-panel`: a multi-line draft proving the skip rules — the
            // link, mention, hashtag, command and code stay unmarked.
            let draft = if matches!(demo, Some(ScreenshotDemo::ReadySpellcheckPanel)) {
                "Teh quick brown fox has a speling error.\n\
                 See https://exampel.com/tehh, ask @tehuser about #tehtag,\n\
                 run /strat or `cargo biuld` \u{1F60A} and recieve it tomorow."
            } else {
                "Teh quick brown fox has a speling error"
            };
            self.composer.update(cx, |input, cx| {
                input.set_value(draft, window, cx);
            });
            self.spellcheck_now(cx);
        }
        if matches!(
            demo,
            Some(ScreenshotDemo::ReadySuggestHashtag | ScreenshotDemo::ReadySuggestEmoji)
        ) {
            // In-memory fixtures; nothing is persisted.
            self.chat_prefs.suggest_emoji = true;
            self.suggest.hashtags = quill::suggest::RecentHashtags::default();
            for tag in [
                "#rustlang",
                "#rust",
                "#rustacean",
                "#ruby",
                "#gpui",
                "#rustlang",
            ] {
                self.suggest.hashtags.record_message(tag);
            }
            let draft = if matches!(demo, Some(ScreenshotDemo::ReadySuggestHashtag)) {
                "shipping the new composer today #ru"
            } else {
                "that release was :fire"
            };
            self.composer.update(cx, |input, cx| {
                input.set_value(draft, window, cx);
                input.set_selected_range(draft.len()..draft.len(), cx);
            });
            self.sync_suggest_menu(cx);
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyChatsComposer)) {
            self.composer.update(cx, |input, cx| {
                input.set_value("hello from composer", window, cx);
            });
        }
        // `QUILL_DEMO_HISTORY_ANIM=…,panel|menu|select`: something over the
        // animated history, to check what the animation layer draws under it.
        if matches!(demo, Some(ScreenshotDemo::ReadyChats)) {
            use super::demo::demo_history_extra;
            let sticker = quill::ids::MessageId(super::demo::HISTORY_ANIM_STICKER);
            if demo_history_extra("panel") {
                self.media_panel.open = true;
                self.media_panel.tab = super::media_panel::PanelTab::Emoji;
            }
            if demo_history_extra("menu") {
                self.message_menu = Some(super::menu_states::MessageMenuState {
                    chat_id: ChatId(11),
                    message_id: sticker,
                    position: point(px(340.), px(380.)),
                });
            }
            if demo_history_extra("select") {
                self.toggle_forward_select(ChatId(11), sticker, false, cx);
            }
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyChannelsAdmin)) {
            self.composer.update(cx, |input, cx| {
                input.set_value("admin post — hello from the channel", window, cx);
            });
        }
        if matches!(demo, Some(ScreenshotDemo::ReadySendMedia)) {
            self.composer.update(cx, |input, cx| {
                input.set_value("sending a photo too", window, cx);
            });
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyPasteImage)) {
            self.composer.update(cx, |input, cx| {
                input.set_value("pasted from clipboard", window, cx);
            });
            self.status_note = "screenshot demo — paste image → composer photo attachment".into();
        }
    }

    /// Screenshot-demo fixture setup (messages): applies the `messages` demo
    /// mutations carved out of `new_with_demo`. Pure code motion.
    pub(super) fn demo_setup_messages(
        &mut self,
        demo: Option<ScreenshotDemo>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(demo, Some(ScreenshotDemo::ReadySearch)) {
            self.search_input.update(cx, |input, cx| {
                input.set_value("hello", window, cx);
                input.focus(window, cx);
            });
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_search(session, &self.demo_sink, &self.demo_seq);
            }
        }
        if matches!(demo, Some(ScreenshotDemo::ReadySearchInChat)) {
            self.chat_search_input.update(cx, |input, cx| {
                input.set_value("hello", window, cx);
                input.focus(window, cx);
            });
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_search_in_chat(session, &self.demo_sink, &self.demo_seq);
            }
        }
        if matches!(
            demo,
            Some(
                ScreenshotDemo::ReadyJumpDate
                    | ScreenshotDemo::ReadySearchFrom
                    | ScreenshotDemo::ReadySearchFromHits
            )
        ) && let Some(session) = self.demo_session.as_mut()
        {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            match demo {
                Some(ScreenshotDemo::ReadyJumpDate) => {
                    apply_ready_jump_date(session, &self.demo_sink, &self.demo_seq)
                }
                Some(ScreenshotDemo::ReadySearchFrom) => {
                    apply_ready_search_from(session, &self.demo_sink, &self.demo_seq)
                }
                _ => apply_ready_search_from_hits(session, &self.demo_sink, &self.demo_seq),
            }
            if !matches!(demo, Some(ScreenshotDemo::ReadyJumpDate)) {
                self.chat_search_input
                    .update(cx, |input, cx| input.focus(window, cx));
            }
            self.status_note = "screenshot demo — find in history".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyScheduled)) {
            let view = super::scheduled_demo::ScheduledView::from_env();
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                super::scheduled_demo::apply_ready_scheduled(
                    session,
                    &self.demo_sink,
                    &self.demo_seq,
                    view,
                );
            }
            match view {
                super::scheduled_demo::ScheduledView::Picker
                | super::scheduled_demo::ScheduledView::Reminder => {
                    self.open_schedule_picker(
                        super::scheduled::ScheduleTarget::Composer,
                        window,
                        cx,
                    );
                }
                super::scheduled_demo::ScheduledView::List
                | super::scheduled_demo::ScheduledView::ReminderList => {
                    self.scheduled_dialog_open = true;
                }
                super::scheduled_demo::ScheduledView::Button => {}
            }
            self.status_note = "screenshot demo — scheduled messages".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadySearchFilters)) {
            self.search_input.update(cx, |input, cx| {
                input.set_value("hello", window, cx);
                input.focus(window, cx);
            });
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_search(session, &self.demo_sink, &self.demo_seq);
                apply_ready_search_filters(session);
            }
        }
        if matches!(demo, Some(ScreenshotDemo::ReadySearchFrequent)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_search_frequent(session, &self.demo_sink, &self.demo_seq);
            }
            self.search_input
                .update(cx, |input, cx| input.focus(window, cx));
        }
        if matches!(demo, Some(ScreenshotDemo::ReadySearchPublic)) {
            self.search_input.update(cx, |input, cx| {
                input.set_value("#dune", window, cx);
                input.focus(window, cx);
            });
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_search_public(session, &self.demo_sink, &self.demo_seq);
            }
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyReply)) {
            self.composer.update(cx, |input, cx| {
                input.set_value("sounds good", window, cx);
                input.focus(window, cx);
            });
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_reply(session, &self.demo_sink, &self.demo_seq);
                self.pending_reply = Some(ComposerReplyTo::new(
                    ChatId(11),
                    MessageId(101),
                    "Hello from injected JSON.",
                ));
            }
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyEditDelete)) {
            let edit = ComposerEdit::from_own_content(
                ChatId(11),
                MessageId(102),
                true,
                false,
                &quill::telegram::envelope::MessageContent::Text(
                    "Reply from the session reducer.".into(),
                ),
            );
            self.composer.update(cx, |input, cx| {
                input.set_value("Reply from the session reducer.", window, cx);
                input.focus(window, cx);
            });
            self.pending_edit = edit;
            self.saved_edit_draft = "unrelated draft stays".into();
            self.pending_delete = DeleteConfirm::own(ChatId(11), MessageId(102), true, false);
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyForward)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_forward(session, &self.demo_sink, &self.demo_seq);
                self.forward_result = session.last_forward.clone();
            }
            let mut draft =
                ForwardDraft::from_message(ChatId(11), MessageId(101), false).expect("forward 101");
            draft.toggle(ChatId(11), MessageId(102), false);
            self.pending_forward = Some(draft);
            self.forward_picker_open = true;
            self.forward_search_input.update(cx, |input, cx| {
                input.set_value("Demo chat B", window, cx);
                input.focus(window, cx);
            });
            self.status_note = "screenshot demo — select → pick dest → forwarded".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadySelectMode)) {
            let mut draft =
                ForwardDraft::from_message(ChatId(11), MessageId(101), false).expect("select 101");
            draft.toggle(ChatId(11), MessageId(102), false);
            self.pending_forward = Some(draft);
            self.status_note = "screenshot demo — selection mode".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyReplyMedia)) {
            self.composer.update(cx, |input, cx| {
                input.set_value("nice shot", window, cx);
                input.focus(window, cx);
            });
            self.pending_reply = Some(ComposerReplyTo::new(
                ChatId(11),
                MessageId(201),
                "Loaded photo",
            ));
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyEditMedia)) {
            let content = self
                .demo_session
                .as_ref()
                .and_then(|session| session.histories.get(&11))
                .and_then(|history| history.messages.get(&302))
                .map(|message| message.content.clone());
            self.pending_edit = content.and_then(|content| {
                ComposerEdit::from_own_content(ChatId(11), MessageId(302), true, false, &content)
            });
            self.composer.update(cx, |input, cx| {
                input.set_value("Outgoing photo", window, cx);
                input.focus(window, cx);
            });
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyReveal)) {
            // The last message "just arrived": reveal it (freeze the frame
            // with `QUILL_MOTION_HOLD_MS`).
            // Read, so the list opens at the bottom (no unread anchor).
            if let Some(session) = self.demo_session.as_mut() {
                if let Some(chat) = session.chats.get_mut(&11) {
                    chat.unread_count = 0;
                }
                if let Some(history) = session.histories.get_mut(&11) {
                    history.unread_anchor = None;
                }
            }
            let rows = self
                .demo_session
                .as_ref()
                .and_then(|session| session.histories.get(&11))
                .map_or(0, |history| history.messages.len());
            self.motion
                .start_reveal(rows.saturating_sub(1), std::time::Instant::now());
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyReactions)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_reactions(session, &self.demo_sink, &self.demo_seq);
            }
            // The message menu with its reaction strip, expanded.
            if let Some(session) = self.demo_session.as_mut() {
                use quill::state::{MessageReactionOptions, ReactionChoice};
                let emoji = |e: &str| ReactionChoice::Emoji(e.to_string());
                session.message_reaction_options = Some(MessageReactionOptions {
                    chat_id: ChatId(11),
                    message_id: MessageId(101),
                    top: ["❤", "👍", "🔥", "😂", "😮", "😢", "🎉"]
                        .into_iter()
                        .map(emoji)
                        .collect(),
                    recent: vec![emoji("👏")],
                    popular: [
                        "🤔", "🙏", "👌", "😍", "🤯", "😱", "🥰", "🤩", "💯", "⚡", "🏆", "🤝",
                    ]
                    .into_iter()
                    .map(emoji)
                    .collect(),
                    allow_custom_emoji: false,
                });
            }
            self.message_menu = Some(MessageMenuState {
                chat_id: ChatId(11),
                message_id: MessageId(101),
                position: point(px(420.), px(200.)),
            });
            self.reactions_expanded = true;
            self.status_note = "screenshot demo — react · unreact · chips".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyPin)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_pin(session, &self.demo_sink, &self.demo_seq);
                let _ = session.begin_chat_search_jump(MessageId(101));
            }
            self.status_note = "screenshot demo — pin · unpin · pinned bar".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyMuteArchive)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_mute_archive(session, &self.demo_sink, &self.demo_seq);
            }
            self.mute_menu_open = true;
            self.status_note = "screenshot demo — mute presets · muted icon · archive".into();
        }
    }

    /// Screenshot-demo fixture setup (chat_list): applies the `chat_list` demo
    /// mutations carved out of `new_with_demo`. Pure code motion.
    pub(super) fn demo_setup_chat_list(
        &mut self,
        demo: Option<ScreenshotDemo>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        // Slice CL1: pinned + archived + marked-as-unread rows, with the
        // row context menu open over the pinned chat.
        if matches!(demo, Some(ScreenshotDemo::ReadyChatListMenu)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_chat_list_menu(session, &self.demo_sink, &self.demo_seq);
            }
            self.chat_menu = Some(ChatMenuState {
                chat_id: ChatId(11),
                position: Point::new(px(120.), px(490.)),
            });
            self.status_note = "screenshot demo — pin · archive · marked unread · row menu".into();
        }
        // Slice CL2: folder tabs + category chips + pinned chat +
        // expanded archive section; Main stays selected.
        if matches!(demo, Some(ScreenshotDemo::ReadyChatList)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_chat_list(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note = "screenshot demo — folders · categories · pinned · archive".into();
        }
        // Slice CL2: same chat-list fixture with the archive
        // auto-settings dialog open.
        if matches!(demo, Some(ScreenshotDemo::ReadyChatListArchive)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_chat_list(session, &self.demo_sink, &self.demo_seq);
                session.archive_settings_open = true;
            }
            self.status_note = "screenshot demo — archive settings dialog".into();
        }
        // Archive row / bar / menu and the pinned drag, over the same
        // fixture (archived chats, story rings, three pinned chats).
        if matches!(
            demo,
            Some(
                ScreenshotDemo::ReadyArchiveRow
                    | ScreenshotDemo::ReadyArchiveBar
                    | ScreenshotDemo::ReadyArchiveMenu
                    | ScreenshotDemo::ReadyPinDrag
            )
        ) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_archive_row(session, &self.demo_sink, &self.demo_seq);
            }
            if matches!(demo, Some(ScreenshotDemo::ReadyArchiveBar)) {
                self.appearance.archive_collapsed = true;
            }
            if matches!(demo, Some(ScreenshotDemo::ReadyArchiveMenu)) {
                self.archive_menu = Some(Point::new(px(120.), px(150.)));
            }
            if matches!(demo, Some(ScreenshotDemo::ReadyPinDrag)) {
                // Pinned 11 / 12 / 13: drag 12 past 13. 13 has just
                // started sliding back up into the slot above.
                let heights = [11, 12, 13].into_iter().map(|id| (id, 64.0)).collect();
                if let Some(mut drag) =
                    quill::pin_reorder::PinReorder::begin(vec![11, 12, 13], heights, 12, 300.)
                {
                    drag.drag_to(352., std::time::Instant::now());
                    self.pin_reorder = Some(drag);
                }
            }
            self.status_note = "screenshot demo — archive row · story rings".into();
        }
        // Slice CL2: sidebar search showing the empty-result state.
        if matches!(demo, Some(ScreenshotDemo::ReadyChatListSearch)) {
            if let Some(session) = self.demo_session.as_mut() {
                session.open_search();
                session.search.begin_query("xyzzy-no-such-chat");
                session.search.status = SearchStatus::Empty;
            }
            self.status_note = "screenshot demo — search empty state".into();
        }
        // Slice media-shared-gallery: the gallery open on chat 11 — the
        // Media tab shows its empty state, the Files tab two injected
        // documents (through the real `foundChatMessages` reducer).
        if matches!(demo, Some(ScreenshotDemo::ReadySharedMedia)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_shared_media(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note = "screenshot demo — shared media gallery empty state".into();
        }
        // Slice CL3: select mode (chats 11 + 12 checked, select bar),
        // the @ mention badge on chat 11, the ♥ reaction badge on chat
        // 12, and the row menu open on chat 11 showing Report / Block
        // user / Select. The menu sits below the rows so both badges
        // and the select bar stay visible (capture at
        // QUILL_DEMO_WINDOW_SIZE=1200x1250 on a 1400x1400 display).
        if matches!(demo, Some(ScreenshotDemo::ReadyChatList3)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_chat_list_3(session, &self.demo_sink, &self.demo_seq);
            }
            self.selected_chats.insert(11);
            self.selected_chats.insert(12);
            self.chat_menu = Some(ChatMenuState {
                chat_id: ChatId(11),
                position: Point::new(px(120.), px(770.)),
            });
            self.status_note =
                "screenshot demo — mentions · reactions · multi-select · report · block".into();
        }
        // Slice CL: peek preview open on chat 12 ("Demo chat B") with a
        // few injected messages; chat 11 stays the open chat so the
        // panel floats over the chat list. The anchor sits just right of
        // the second chat row.
        if matches!(demo, Some(ScreenshotDemo::ReadyChatPreview)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_chat_preview(session, &self.demo_sink, &self.demo_seq);
            }
            self.chat_preview = Some(ChatPreviewState {
                chat_id: ChatId(12),
                anchor: Point::new(px(170.), px(470.)),
            });
            self.status_note = "screenshot demo — chat peek preview".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyNotificationSound)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_notification_sound(session, &self.demo_sink, &self.demo_seq);
            }
            self.mute_menu_open = true;
            self.notif_sound_picker_open = true;
            self.status_note =
                "screenshot demo — notification sounds · per-chat panel · scope defaults".into();
        }
        // Chat-row swipe and stories-strip collapse over one long list:
        // the archive-row fixture (story rings, pins, archive) plus the
        // chat-row fixture's extra chats, so the list scrolls.
        if matches!(
            demo,
            Some(
                ScreenshotDemo::ReadySwipeMute
                    | ScreenshotDemo::ReadySwipeReached
                    | ScreenshotDemo::ReadyStoriesExpanded
                    | ScreenshotDemo::ReadyStoriesCollapsing
                    | ScreenshotDemo::ReadyStoriesCollapsed
            )
        ) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_archive_row(session, &self.demo_sink, &self.demo_seq);
                apply_ready_chat_rows(session, &self.demo_sink, &self.demo_seq);
            }
            match demo {
                Some(ScreenshotDemo::ReadySwipeMute) => {
                    self.appearance.swipe_action = quill::chat_swipe::SwipeAction::Mute;
                    self.demo_hold_swipe(21, 0.6);
                }
                Some(ScreenshotDemo::ReadySwipeReached) => {
                    self.appearance.swipe_action = quill::chat_swipe::SwipeAction::Delete;
                    if let Some(chat) = self
                        .demo_session
                        .as_mut()
                        .and_then(|session| session.chats.get_mut(&22))
                    {
                        chat.can_be_deleted_only_for_self = true;
                    }
                    self.demo_hold_swipe(22, 1.25);
                }
                Some(ScreenshotDemo::ReadyStoriesExpanded) => {
                    // Resting list, Mute configured: the base for scripted
                    // gestures (`QUILL_DEMO_CLICK=w:x,y,dx,dy,s|m|e`).
                    self.appearance.swipe_action = quill::chat_swipe::SwipeAction::Mute;
                }
                Some(ScreenshotDemo::ReadyStoriesCollapsing) => {
                    self.chat_list_scroll.set_offset(point(px(0.), px(-38.)));
                }
                Some(ScreenshotDemo::ReadyStoriesCollapsed) => {
                    self.chat_list_scroll.set_offset(point(px(0.), px(-96.)));
                }
                _ => {}
            }
            self.status_note = "screenshot demo — swipe actions · stories strip".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyChatRows)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_chat_rows(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note = "screenshot demo — chat rows".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyJoinBar)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_join_bar(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note = "screenshot demo — non-member channel".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyTopBars)) {
            let variant = std::env::var("QUILL_DEMO_BAR").unwrap_or_default();
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                super::chat_bars::apply_ready_top_bars(
                    session,
                    &self.demo_sink,
                    &self.demo_seq,
                    &variant,
                );
            }
            match variant.as_str() {
                "requests-box" => {
                    self.join_requests_dialog =
                        Some(quill::ids::ChatId(super::chat_bars::DEMO_GROUP));
                }
                "block-box" => {
                    self.block_bar_dialog = Some(super::chat_bars::BlockBarDialog {
                        chat_id: quill::ids::ChatId(super::chat_bars::DEMO_STRANGER),
                        user_id: super::chat_bars::DEMO_STRANGER,
                        report: true,
                        delete_chat: true,
                    });
                }
                _ => {}
            }
            self.status_note = "screenshot demo — chat top bars".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadySearchPreviews)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_search_previews(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note = "screenshot demo — search previews".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyMultilineRows)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_multiline_rows(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note = "screenshot demo — multi-line row previews".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyTyping)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_typing(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note = "screenshot demo — typing…".into();
        }
    }

    /// Screenshot-demo fixture setup (media): applies the `media` demo
    /// mutations carved out of `new_with_demo`. Pure code motion.
    pub(super) fn demo_setup_media(
        &mut self,
        demo: Option<ScreenshotDemo>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if demo == Some(ScreenshotDemo::ReadyDownloads) {
            // Exercise failed playback recovery without a network request.
            self.pending_gif_play = Some((MessageId(205), FileId(26), String::new()));
            self.pending_video_play = Some((MessageId(205), FileId(26), String::new(), 0, None));
            self.pending_audio_play = Some((ChatId(11), MessageId(205), FileId(26), 1.));
            self.viewer_pending_play = Some((MessageId(205), FileId(26)));
            self.pending_voice_play = Some((ChatId(11), MessageId(205), FileId(26), false, 1.));
            self.discard_stopped_media_playback(cx);
            assert!(
                self.pending_gif_play.is_none()
                    && self.pending_video_play.is_none()
                    && self.pending_audio_play.is_none()
                    && self.viewer_pending_play.is_none()
                    && self.pending_voice_play.is_none()
            );
            self.pending_audio_play = Some((ChatId(11), MessageId(204), FileId(24), 1.));
            self.discard_stopped_media_playback(cx);
            assert!(self.pending_audio_play.is_some());
            self.pending_audio_play = None;
        }
        if matches!(
            demo,
            Some(ScreenshotDemo::ReadyStickers | ScreenshotDemo::ReadyStickerPlayback)
        ) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_stickers(session, &self.demo_sink, &self.demo_seq);
                if demo == Some(ScreenshotDemo::ReadyStickerPlayback) {
                    super::composer_ui::apply_ready_sticker_playback(
                        session,
                        &self.demo_sink,
                        &self.demo_seq,
                    );
                }
            }
            // The panel's library holds the demo set's contents.
            if let Some(session) = self.demo_session.as_mut() {
                let mut stickers = session.stickers.stickers.clone();
                // Performance fixture: `QUILL_DEMO_STICKERS=<n>` fills the
                // picker with `n` distinct animated stickers (more than the
                // playback cache holds).
                let extra: i32 = std::env::var("QUILL_DEMO_STICKERS")
                    .ok()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0);
                if demo == Some(ScreenshotDemo::ReadyStickerPlayback) && extra > 0 {
                    let seq = std::sync::atomic::AtomicU64::new(session.last_seq);
                    let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
                        self.demo_sink.clone();
                    let root = super::demo::demo_media_allowlist();
                    for i in 0..extra {
                        let id = 9_000 + i;
                        let name = if i % 2 == 0 {
                            "demo-sticker.tgs"
                        } else {
                            "demo-sticker.webm"
                        };
                        let json = super::demo::demo_file_json(
                            id,
                            &root.join(name).to_string_lossy(),
                            true,
                        );
                        if let Some(owned) =
                            quill::telegram::client::copy_and_parse(&json, &seq, &sink)
                        {
                            session.apply(owned);
                        }
                        let mut item = stickers[0].clone();
                        item.id = i64::from(id);
                        item.file_id = quill::ids::FileId(id);
                        item.format = if i % 2 == 0 {
                            quill::telegram::envelope::StickerFormat::Tgs
                        } else {
                            quill::telegram::envelope::StickerFormat::Webm
                        };
                        stickers.push(item);
                    }
                }
                session.media_library.set_stickers.insert(77, stickers);
            }
            self.media_panel.open = true;
            self.media_panel.tab = super::media_panel::PanelTab::Stickers;
            self.status_note = "screenshot demo — stickers · tap to send".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyEmojiPanel)) {
            self.set_media_pref(
                |prefs| {
                    prefs.recent_emoji = ["👍", "😂", "❤️", "🔥", "🎉", "😍", "🙏", "😭"]
                        .iter()
                        .map(|e| (*e).to_string())
                        .collect();
                },
                cx,
            );
            self.media_panel.open = true;
            self.media_panel.tab = super::media_panel::PanelTab::Emoji;
            self.status_note = "screenshot demo — emoji panel".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyVoice)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_voice(session, &self.demo_sink, &self.demo_seq);
            }
            // One Play request must survive the download and start the note.
            let local = self.demo_session.as_ref().unwrap().files[&82].clone();
            let mut waiting = local.clone();
            waiting.local.path.clear();
            waiting.local.is_downloading_completed = false;
            self.demo_session
                .as_mut()
                .unwrap()
                .files
                .insert(82, waiting);
            self.toggle_voice_playback(ChatId(11), MessageId(91), FileId(82), true, 3., cx);
            assert!(self.pending_voice_play.is_some());
            self.demo_session.as_mut().unwrap().files.insert(82, local);
            self.resume_pending_voice(cx);
            assert!(self.pending_voice_play.is_none() && self.playing_voice == Some(MessageId(91)));
            self.stop_voice_playback();
            let bars = vec![4, 16, 28, 12, 8, 20, 6, 18, 10, 24, 8, 14];
            self.voice_capture = Some(VoiceCapture::preview(
                demo_media_allowlist().join("demo-voice.ogg"),
                2,
                bars,
            ));
            self.playing_voice = Some(MessageId(91));
            // MED2: demo shows the locked record bar + a transcribed note.
            self.record_locked = true;
            self.status_note =
                "screenshot demo — recording voice · locked · playing voice note".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyShortcuts)) {
            self.shortcuts_open = true;
            self.status_note = "screenshot demo — keyboard shortcuts reference".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyGameCard)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_game_card(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note = "screenshot demo — game card + high scores".into();
        }
        if matches!(
            demo,
            Some(ScreenshotDemo::ReadyGifs | ScreenshotDemo::ReadyGifPlayback)
        ) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_gifs(session, &self.demo_sink, &self.demo_seq);
                if demo == Some(ScreenshotDemo::ReadyGifPlayback) {
                    session.gifs.open = false;
                    if let Some(history) = session.histories.get_mut(&11) {
                        history.messages.retain(|id, _| *id == 501);
                    }
                }
            }
            if demo == Some(ScreenshotDemo::ReadyGifs) {
                self.media_panel.open = true;
                self.media_panel.tab = super::media_panel::PanelTab::Gifs;
                self.toggle_animation_playback(
                    MessageId(501),
                    quill::ids::FileId(63),
                    "image/gif".into(),
                    cx,
                );
            }
        }
        if demo == Some(ScreenshotDemo::ReadyEmojiPacks)
            && let Some(session) = self.demo_session.as_mut()
        {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_gifs(session, &self.demo_sink, &self.demo_seq);
            session.gifs.open = false;
            session.emoji.open = true;
            session.emoji.installed_sets = [
                "Downloaded pack",
                "Downloading pack",
                "Updated pack",
                "Installing pack",
            ]
            .iter()
            .enumerate()
            .map(|(index, title)| quill::telegram::envelope::StickerSetInfo {
                id: index as i64 + 1,
                title: (*title).into(),
                name: (*title).into(),
                size: 1,
                is_installed: true,
                is_official: false,
            })
            .collect();
            session
                .emoji
                .pack_files
                .insert(1, vec![quill::ids::FileId(63)]);
            session
                .emoji
                .pack_files
                .insert(2, vec![quill::ids::FileId(62)]);
            session.downloading.insert(62);
            session.emoji.outdated_packs.insert(3);
            session.emoji.mutating_set = Some((4, true));
            session.media_prefs.recent_emoji_packs = vec![2, 1];
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyVideo)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_video(session, &self.demo_sink, &self.demo_seq);
            }
            self.playing_video = Some(MessageId(601));
            self.video_frames = vec![
                demo_media_allowlist().join("demo-gif-1.png"),
                demo_media_allowlist().join("demo-gif-2.png"),
            ];
            self.spawn_video_tick(cx);
            self.status_note = "screenshot demo — video · playing".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyVideoNote)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_video_note(session, &self.demo_sink, &self.demo_seq);
            }
            self.playing_video = Some(MessageId(611));
            self.video_frames = vec![
                demo_media_allowlist().join("demo-gif-1.png"),
                demo_media_allowlist().join("demo-gif-2.png"),
            ];
            self.spawn_video_tick(cx);
            self.status_note = "screenshot demo — video note · playing".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyAudio)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_audio(session, &self.demo_sink, &self.demo_seq);
            }
            self.playing_audio = Some(MessageId(801));
            self.status_note = "screenshot demo — audio · playing".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyPlayerBar)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_audio(session, &self.demo_sink, &self.demo_seq);
            }
            // Music paused at 1:27 of 3:34; the bar shows repeat-all and
            // shuffle as active.
            self.begin_track_playback(
                PlaybackKind::Audio,
                ChatId(11),
                MessageId(801),
                214.0,
                87.0,
                cx,
            );
            self.pause_active_playback();
            self.player.repeat = quill::playlist::RepeatMode::All;
            self.player.order = quill::playlist::OrderMode::Shuffle;
            self.status_note = "screenshot demo — player bar".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadySeekBars)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_voice(session, &self.demo_sink, &self.demo_seq);
                apply_ready_audio(session, &self.demo_sink, &self.demo_seq);
            }
            // Fake an in-progress playback without starting audio: voice
            // note 90 (12 s) playing from 5.0 s — the tick advances it —
            // and the music track 801 (214 s) paused with a remembered
            // 1:27 position, so both rows show seek bars.
            self.begin_track_playback(
                PlaybackKind::Voice,
                ChatId(11),
                MessageId(90),
                12.0,
                5.0,
                cx,
            );
            self.playback_positions.insert(MessageId(801), 87.0);
            self.status_note = "screenshot demo — seek bars · voice playing · audio paused".into();
        }
    }

    /// Screenshot-demo fixture setup (groups): applies the `groups` demo
    /// mutations carved out of `new_with_demo`. Pure code motion.
    pub(super) fn demo_setup_groups(
        &mut self,
        demo: Option<ScreenshotDemo>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(demo, Some(ScreenshotDemo::ReadyForumTopics)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_forum_topics(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note = "screenshot demo — forum topics list".into();
        }
        let bot_topics_mode = match demo {
            Some(ScreenshotDemo::ReadyBotTopics) => Some(SubsectionTabsMode::Top),
            Some(ScreenshotDemo::ReadyBotTopicsBottom) => Some(SubsectionTabsMode::Bottom),
            Some(ScreenshotDemo::ReadyBotTopicsLeft) => Some(SubsectionTabsMode::Left),
            _ => None,
        };
        if let Some(mode) = bot_topics_mode {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_bot_topics(session, &self.demo_sink, &self.demo_seq, mode);
            }
            self.status_note = "screenshot demo — bot topic tabs".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyTopicPost)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_topic_post(session, &self.demo_sink, &self.demo_seq);
            }
            self.composer.update(cx, |input, cx| {
                input.set_value("Posting into the General topic…", window, cx);
            });
            self.status_note = "screenshot demo — posting to a forum topic".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyContacts)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_contacts(session, &self.demo_sink, &self.demo_seq);
            }
            self.contacts_tab_open = true;
            self.status_note = "screenshot demo — contacts tab + user info panel".into();
        }
        // Slice A6: contacts management — the fixture's panel is moved
        // to Ada (31, a contact) so Delete contact + Block user show,
        // and a notice proves the settings-section wiring.
        if matches!(demo, Some(ScreenshotDemo::ReadyContactsManage)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_contacts(session, &self.demo_sink, &self.demo_seq);
                session.open_info_panel = Some(InfoPanelTarget::User(31));
                session.contacts_notice = Some("Imported 2 contacts.".to_string());
            }
            self.contacts_tab_open = true;
            self.status_note = "screenshot demo — contacts management".into();
        }
        // Slice A6: block-user confirm dialog open for Ada (31) over the
        // contacts fixture — TGX `BlockUserConfirm`. Ada's info panel
        // is open behind the dialog so the demo is consistent.
        if matches!(demo, Some(ScreenshotDemo::ReadyBlockUser)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_contacts(session, &self.demo_sink, &self.demo_seq);
                session.open_info_panel = Some(InfoPanelTarget::User(31));
            }
            self.contacts_tab_open = true;
            self.group_confirm_dialog = Some(GroupConfirmDialog {
                chat_id: ChatId(0),
                action: GroupConfirmAction::BlockContact {
                    user_id: 31,
                    block: true,
                },
            });
            self.status_note = "screenshot demo — block user confirm".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyFolders)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_folders(session, &self.demo_sink, &self.demo_seq);
            }
            // Select the non-default "News" folder so the screenshot shows
            // the filtered chat list.
            self.folder_tab = Some(2);
            self.status_note = "screenshot demo — folder tabs · News folder".into();
        }
        // Parity slice: manage dialog over the same folder fixture.
        if matches!(demo, Some(ScreenshotDemo::ReadyFoldersManage)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_folders(session, &self.demo_sink, &self.demo_seq);
            }
            // Open the manage dialog over the folder fixture.
            self.folder_manage_open = true;
            self.status_note = "screenshot demo — folder management dialog".into();
        }
        // Parity slice: chat-list avatars + channel header extras fixture.
        if matches!(demo, Some(ScreenshotDemo::ReadyChatAvatars)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_chat_avatars(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note = "chat avatars & channel header".into();
        }
        // Phase A1: slow-mode enforcement fixture — the composer shows the
        // countdown and blocks sends until it expires.
        if matches!(demo, Some(ScreenshotDemo::ReadySlowMode)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_slow_mode(session, &self.demo_sink, &self.demo_seq);
            }
            self.composer.update(cx, |input, cx| {
                input.set_value("this send will be blocked by slow mode…", window, cx);
            });
            self.status_note =
                "slow-mode enforcement — sends blocked until the timer expires".into();
        }
    }

    /// Screenshot-demo fixture setup (security): applies the `security` demo
    /// mutations carved out of `new_with_demo`. Pure code motion.
    pub(super) fn demo_setup_security(
        &mut self,
        demo: Option<ScreenshotDemo>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Batch 4: new-login alert fixture — an unconfirmed Android login
        // resolved from the sessions list (injected, no live Telegram).
        if matches!(demo, Some(ScreenshotDemo::ReadyNewLogin))
            && let Some(session) = self.demo_session.as_mut()
        {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            session.notices.unconfirmed_count = 1;
            session.notices.unconfirmed_entries = vec![quill::state::UnconfirmedEntry {
                id: 77,
                device: "Pixel 9".into(),
                location: "Berlin, Germany".into(),
            }];
        }
        // Batch 4: "New Login Prevented" box (injected, no live Telegram).
        if matches!(demo, Some(ScreenshotDemo::ReadyLoginPrevented)) {
            self.login_prevented = Some(vec!["Berlin, Germany (Pixel 9)".into()]);
        }
        // Batch 4: server service notification popup (injected).
        if matches!(demo, Some(ScreenshotDemo::ReadyServiceNotice))
            && let Some(session) = self.demo_session.as_mut()
        {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            session.notices.service.push_back(quill::state::ServiceNotice {
                    kind: String::new(),
                    text: "Your Telegram Premium subscription ends in 3 days. Renew it to keep your extra features.".into(),
                });
        }
        // Batch 4: terms of service prompt with the age check (injected).
        if matches!(demo, Some(ScreenshotDemo::ReadyTerms))
            && let Some(session) = self.demo_session.as_mut()
        {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            session.notices.terms = Some(quill::telegram::envelope::TermsOfService {
                    id: "tos-2026".into(),
                    text: "1. Telegram is a cloud service. Your messages, media and files are stored on our servers so you can reach them from any device.\n\n2. Do not use Telegram to spam, scam or harm others, and do not promote violence or sell illegal goods.\n\n3. We do not use your data for ad targeting. You can adjust how your data is used in Privacy & Security settings.\n\nBy continuing you accept these updated terms.".into(),
                    min_user_age: 16,
                    show_popup: true,
                });
        }
        // Batch 6: local storage fixture — two ticked types with the clear
        // confirmation open, limits applied (injected, no live Telegram).
        if matches!(demo, Some(ScreenshotDemo::ReadyLocalStorage)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                session.storage_stats = Some(demo_storage_stats());
                session.storage_stats_loading = false;
                session.data_storage = demo_data_storage_prefs();
                for (name, value) in quill::storage_limits::options_for(
                    Some(2 * 1024 * 1024 * 1024),
                    Some(31 * 86_400),
                ) {
                    session.storage_limits.apply_option(
                        name,
                        &match value {
                            quill::storage_limits::StorageOptionValue::Boolean(on) => {
                                quill::telegram::envelope::OptionValue::Boolean(on)
                            }
                            quill::storage_limits::StorageOptionValue::Integer(n) => {
                                quill::telegram::envelope::OptionValue::Integer(n)
                            }
                        },
                    );
                }
            }
            self.storage_selected.insert("fileTypePhoto");
            self.storage_selected.insert("fileTypeVideo");
            self.storage_confirm = Some(StorageClear::Selected);
            self.storage_usage_open = true;
        }
        // Batch 6: "Forgot password?" code step (injected).
        if matches!(demo, Some(ScreenshotDemo::Ready2faForgot)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                session.password_state = Some(demo_password_state_manage());
                session.password_state_loading = false;
                session.twofa_flow.recovery_code_sent_to = Some("i***@example.com".into());
            }
            self.twofa_view = TwofaView::Recover;
            self.twofa_open = true;
        }
        // Batch 6: reset waiting period (injected).
        if matches!(demo, Some(ScreenshotDemo::Ready2faReset)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                let mut state = demo_password_state_manage();
                state.has_recovery_email_address = false;
                state.pending_reset_date =
                    ((quill::state::unix_ms_now() / 1000) + 5 * 86_400 + 3_600) as i32;
                session.password_state = Some(state);
                session.password_state_loading = false;
            }
            self.twofa_view = TwofaView::Recover;
            self.twofa_open = true;
        }
        // Batch 6: login email code step (injected).
        if matches!(demo, Some(ScreenshotDemo::ReadyLoginEmail)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                let mut state = demo_password_state_manage();
                state.login_email_address_pattern = "i***@example.com".into();
                session.password_state = Some(state);
                session.password_state_loading = false;
                session.twofa_flow.login_email_code_sent_to = Some("m***@example.com".into());
            }
            self.twofa_view = TwofaView::LoginEmail;
            self.twofa_open = true;
        }
        // Phase B1: secret chat lifecycle fixture — a Ready secret chat
        // with Zed, opened with E2E history and the composer live.
        if matches!(demo, Some(ScreenshotDemo::ReadySecretChat)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_secret_chat(session, &self.demo_sink, &self.demo_seq);
            }
            self.composer.update(cx, |input, cx| {
                input.set_value("this goes through the E2E session…", window, cx);
            });
            self.status_note = "secret chat — Ready, 🔒 badge in the chat list".into();
        }
        // Phase S1: "New secret chat" picker fixture — the secret chat
        // fixture plus contacts, with the picker open. `contacts` is
        // assigned directly (fixture equivalent of a `getContacts`
        // answer); the picker rows come from `contact_rows()` through the
        // real eligibility gate.
        if matches!(demo, Some(ScreenshotDemo::ReadySecretPicker)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_secret_chat(session, &self.demo_sink, &self.demo_seq);
                apply_ready_contacts(session, &self.demo_sink, &self.demo_seq);
                session.contacts = Some(vec![31, 33]);
                // `apply_ready_contacts` opens the contact info panel for
                // its own demo; the picker screenshot wants it closed.
                session.open_info_panel = None;
            }
            self.new_secret_picker_open = true;
            self.status_note = "screenshot demo — new secret chat picker".into();
        }
        // Phase S2: inline-bot warning fixture — the Ready secret chat
        // with a stashed `SwitchInline` query, so the warning banner
        // renders above the composer (injected, no live Telegram).
        if matches!(demo, Some(ScreenshotDemo::ReadySecretBotAlert)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_secret_chat(session, &self.demo_sink, &self.demo_seq);
            }
            self.pending_inline_bot_alert = Some("@gif cats".to_string());
            self.status_note = "screenshot demo — inline-bot warning in secret chat".into();
        }
        // Slice S4: Data & Storage fixture — fixture stats (including the
        // secret category and per-chat rows) plus seeded per-network
        // download settings, with the dialog open (injected, no live
        // Telegram).
        if matches!(demo, Some(ScreenshotDemo::ReadyStorageUsage)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                session.storage_stats = Some(demo_storage_stats());
                session.storage_stats_loading = false;
                session.data_storage = demo_data_storage_prefs();
                session.storage_freed = Some(54_525_952);
            }
            self.storage_usage_open = true;
            self.status_note = "screenshot demo — data & storage".into();
        }
        // Slice `parity:bots-payment-recurring`: Subscriptions fixture —
        // fixture `starSubscriptions` with the dialog open (injected, no
        // live Telegram).
        if matches!(demo, Some(ScreenshotDemo::ReadySubscriptions)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                session.star_subscriptions = Some(demo_star_subscriptions());
                session.star_subscriptions_loading = false;
                session.subscriptions_open = true;
            }
            self.status_note = "screenshot demo — ⭐ subscriptions".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyMarketplaceGift)) {
            if let Some(session) = self.demo_session.as_mut() {
                let quote=quill::marketplace::GiftQuote::parse(&serde_json::json!({"name":"PlushPepe-123","title":"Plush Pepe","resale_parameters":{"star_count":25,"gram_cent_count":0,"gram_only":false}})).unwrap();
                session.gift_text_length_max = Some(128);
                session.marketplace_gift = Some(quill::marketplace::GiftPurchase {
                    chat_id: ChatId(11),
                    recipient: quill::telegram::envelope::MessageSender::User { user_id: 11 },
                    recipient_name: "Demo chat A".into(),
                    requested_name: quote.name.clone(),
                    price: quote.stars,
                    quote: Some(quote),
                    loading: false,
                    sending: false,
                    completed: false,
                    note: Some("Injected quote — no purchase is sent in this demo.".into()),
                });
            }
            self.marketplace_name_input
                .update(cx, |input, cx| input.set_value("PlushPepe-123", window, cx));
            self.marketplace_comment_input.update(cx, |input, cx| {
                input.set_value("A little gift for you 🎁", window, cx)
            });
            self.marketplace_open = true;
        }
        // Settings → Appearance: the Appearance dialog open over the
        // ReadyChats fixture (injected, no live Telegram). Non-default
        // values so the screenshot shows the slice live: dark theme,
        // blue accent, dark wallpaper, 16px message text. They are only
        // in-memory for the demo — `apply_appearance` (end of this fn)
        // picks them up; nothing is persisted.
        if matches!(
            demo,
            Some(
                ScreenshotDemo::ReadyAppearance
                    | ScreenshotDemo::ReadySpellcheckToggle
                    | ScreenshotDemo::ReadyKeybindings
            )
        ) {
            if matches!(demo, Some(ScreenshotDemo::ReadySpellcheckToggle)) {
                self.chat_prefs.spellcheck_enabled = true;
            }
            // stories-high-contrast: `QUILL_DEMO_THEME=high-contrast`
            // captures the dialog with the HC theme selected.
            self.appearance.theme =
                if std::env::var("QUILL_DEMO_THEME").as_deref() == Ok("high-contrast") {
                    ThemeChoice::HighContrast
                } else {
                    ThemeChoice::Dark
                };
            self.appearance.accent_rgb = 0x2f81f7;
            self.appearance.wallpaper_rgb = Some(0x0e1621);
            self.appearance.font_size_px = 16;
            self.appearance_open = true;
            self.keybindings_screenshot = matches!(demo, Some(ScreenshotDemo::ReadyKeybindings));
            self.status_note = if self.keybindings_screenshot {
                "screenshot demo — keyboard shortcuts".into()
            } else {
                "screenshot demo — appearance settings".into()
            };
        }
        // Slice parity:auth-multi-account (UI): the Accounts dialog open
        // over the ReadyChats fixture (injected, no live Telegram). The
        // list reads the real local registry, read-only — nothing is
        // added, switched, or removed by the fixture.
        if matches!(
            demo,
            Some(
                ScreenshotDemo::ReadyPasscodeSettings
                    | ScreenshotDemo::ReadyPasscodeCreate
                    | ScreenshotDemo::ReadyLockScreen
            )
        ) {
            self.passcode_ui.fixture(
                !matches!(demo, Some(ScreenshotDemo::ReadyPasscodeCreate)),
                matches!(demo, Some(ScreenshotDemo::ReadyLockScreen)),
                matches!(demo, Some(ScreenshotDemo::ReadyLockScreen)).then_some("Wrong passcode"),
            );
            self.passcode_ui.autolock_secs = 300;
            self.passcode_ui.system_unlock = true;
            if matches!(demo, Some(ScreenshotDemo::ReadyPasscodeSettings)) {
                self.passcode_ui.open = true;
            }
            if matches!(demo, Some(ScreenshotDemo::ReadyPasscodeCreate)) {
                self.passcode_ui.open = true;
                self.passcode_ui.view = super::passcode::PasscodeView::Create;
                self.passcode_ui.error = Some("Passcodes are different".into());
            }
            self.status_note = "screenshot demo — local passcode".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyAccounts)) {
            self.accounts_ui.open = true;
            self.status_note = "screenshot demo — accounts".into();
        }
        // Slice A2: 2FA overlay fixture — password set with recovery
        // email (injected `passwordState`, no live Telegram).
        if matches!(demo, Some(ScreenshotDemo::Ready2faManage)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                session.password_state = Some(demo_password_state_manage());
                session.password_state_loading = false;
            }
            self.twofa_open = true;
            self.status_note = "screenshot demo — two-step verification".into();
        }
        // Slice A2: 2FA overlay fixture — recovery email pending
        // confirmation (injected `passwordState`, no live Telegram).
        if matches!(demo, Some(ScreenshotDemo::ReadyRecoveryEmail)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                session.password_state = Some(demo_password_state_pending());
                session.password_state_loading = false;
            }
            self.twofa_open = true;
            self.status_note = "screenshot demo — recovery email pending".into();
        }
        // Slice A9: account lifecycle fixture — injected `accountTtl`
        // (180 days) + `passwordState` with a password set (no live
        // Telegram), dialog open.
        if matches!(demo, Some(ScreenshotDemo::ReadyAccountLifecycle)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                session.account_ttl_days = Some(180);
                session.default_auto_delete_secs = Some(604_800);
                session.account_ttl_loading = false;
                session.password_state = Some(demo_password_state_manage());
                session.password_state_loading = false;
            }
            self.account_lifecycle.open = true;
            // Slice auth-logout-warning: arm the logout confirm so the
            // screenshot shows the SignOutHint2 warning.
            self.account_lifecycle.confirm_logout = true;
            self.status_note = "screenshot demo — account lifecycle".into();
        }
        // Slice A3: Active Sessions fixture — fixture sessions (current
        // device, two other sessions, one incomplete login attempt) with
        // the overlay open (injected, no live Telegram).
        if matches!(demo, Some(ScreenshotDemo::ReadySessions)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                session.sessions = Some(demo_sessions());
                session.sessions_loading = false;
                session.sessions_error = None;
            }
            self.sessions_open = true;
            if std::env::var_os("QUILL_DEMO_DEVICE_LINK").is_some() {
                self.device_login_qr =
                    Some(zeroize::Zeroizing::new("tg://login?token=AQID".into()));
            }
            self.status_note = "screenshot demo — active sessions".into();
        }
        // Slice A4: session acceptance toggles fixture — the same fixture
        // sessions (varied Secret Chats / Calls flags) with the sessions
        // overlay open so the per-row direct toggles are visible
        // (injected, no live Telegram).
        if matches!(demo, Some(ScreenshotDemo::ReadySessionToggles)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                session.sessions = Some(demo_sessions());
                session.sessions_loading = false;
                session.sessions_error = None;
            }
            self.sessions_open = true;
            self.status_note = "screenshot demo — session acceptance toggles".into();
        }
        // Slice A4: Connected Websites fixture — fixture websites with
        // the overlay open (injected, no live Telegram).
        if matches!(demo, Some(ScreenshotDemo::ReadyWebSessions)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                session.connected_websites = Some(demo_websites());
                session.connected_websites_loading = false;
                session.websites_error = None;
            }
            self.websites_open = true;
            self.status_note = "screenshot demo — connected websites".into();
        }
        // Phase B2: key verification fixture — the Ready secret chat with
        // a real 36-byte key_hash and Zed's info panel open on the
        // "Encryption key" fingerprint grid.
        if matches!(demo, Some(ScreenshotDemo::ReadyKeyVerification)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_key_verification(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note =
                "secret chat key verification — compare with your contact's device".into();
        }
        // Phase B3: self-destructing media fixture — a private chat with
        // Zed carrying a live-timer incoming photo and a view-once
        // outgoing photo; the composer's pending photo attachment has
        // the picker pre-set to 30s.
        if matches!(demo, Some(ScreenshotDemo::ReadySelfDestruct)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_self_destruct(session, &self.demo_sink, &self.demo_seq);
            }
            self.composer_self_destruct = Some(SelfDestructSend::Timer(30));
            self.status_note =
                    "screenshot demo — self-destructing media · picker on 30s (injected, no live Telegram)"
                        .into();
        }
    }

    /// Screenshot-demo fixture setup (calls): applies the `calls` demo
    /// mutations carved out of `new_with_demo`. Pure code motion.
    pub(super) fn demo_setup_calls(
        &mut self,
        demo: Option<ScreenshotDemo>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        // Phase C1: incoming-call fixture — Zed rings with a pending
        // voice call, so the overlay renders the incoming-call card
        // (Accept / Decline + ticking clock).
        if matches!(demo, Some(ScreenshotDemo::ReadyCall)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_call(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note =
                "screenshot demo — incoming call from Zed (injected, no live Telegram)".into();
        }
        // Swap prompt: active outgoing call with Zed + incoming pending
        // video call from Ada, so the state machine raises the swap
        // prompt and the kit dialog renders (End & answer / Decline).
        if matches!(demo, Some(ScreenshotDemo::ReadyCallSwap)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_call_swap(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note =
                "screenshot demo — swap prompt: Ada calling while in a call with Zed (injected, no live Telegram)".into();
        }
        // Phase C1b: connected-video-call fixture — Zed's incoming
        // video call goes pending → exchanging keys → ready, so the
        // overlay renders the video-stage placeholder grid (remote +
        // local tiles), the 📹 kind line, duration clock, Mute / Hang
        // up, and the honest no-video-transport note.
        // Phase C2e: now with synthetic video — an injected camera
        // device, the peer streaming (Active), and two distinct
        // generated test patterns (remote: teal + circle; local: warm +
        // crosshair). NOT a real camera: generated in code, demo only.
        if matches!(demo, Some(ScreenshotDemo::ReadyCallVideo)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_call_video(session, &self.demo_sink, &self.demo_seq);
            }
            self.demo_call_devices = Some(vec![quill::calls::engine::MediaDevice {
                id: "demo-cam".into(),
                name: "Demo Camera (synthetic)".into(),
                kind: quill::calls::engine::MediaDeviceKind::Camera,
            }]);
            self.demo_remote_frame = Some(demo_video_frame(false));
            self.demo_local_frame = Some(demo_video_frame(true));
            if let Some(call) = self
                .demo_session
                .as_mut()
                .and_then(|session| session.active_call.as_mut())
            {
                call.remote_video = quill::calls::engine::RemoteVideoState::Active;
                call.transport = Some(quill::calls::engine::TransportState::Connected);
            }
            self.status_note =
                "screenshot demo — connected video call with Zed (injected, no live Telegram)"
                    .into();
        }
        // Phase C2i: 1:1 screen-share send fixture — the connected
        // video call with the toggle engaged (injected state, no live
        // Telegram, no real capture). The demo devices carry a screen
        // source so the "Stop sharing" button renders.
        if matches!(demo, Some(ScreenshotDemo::ReadyCallScreenShare)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_call_video(session, &self.demo_sink, &self.demo_seq);
            }
            self.demo_call_devices = Some(vec![
                quill::calls::engine::MediaDevice {
                    id: "demo-cam".into(),
                    name: "Demo Camera (synthetic)".into(),
                    kind: quill::calls::engine::MediaDeviceKind::Camera,
                },
                quill::calls::engine::MediaDevice {
                    id: "demo-screen".into(),
                    name: "Demo Screen (synthetic)".into(),
                    kind: quill::calls::engine::MediaDeviceKind::Screen,
                },
            ]);
            self.demo_remote_frame = Some(demo_video_frame(false));
            self.demo_local_frame = Some(demo_video_frame(true));
            if let Some(call) = self
                .demo_session
                .as_mut()
                .and_then(|session| session.active_call.as_mut())
            {
                call.remote_video = quill::calls::engine::RemoteVideoState::Active;
                call.transport = Some(quill::calls::engine::TransportState::Connected);
                call.screen_sharing = true;
                call.camera_on = false;
            }
            self.status_note =
                "screenshot demo — 1:1 call screen-share send (injected, no live Telegram)".into();
        }
        // Phase C2l: 1:1 screen-share receive fixture — the peer shares
        // their screen, so the main video-stage tile renders the
        // screen frame with the badge; the local camera stays the PiP.
        if matches!(demo, Some(ScreenshotDemo::ReadyCallScreenShareReceive)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_call_video(session, &self.demo_sink, &self.demo_seq);
            }
            self.demo_call_devices = Some(vec![
                quill::calls::engine::MediaDevice {
                    id: "demo-cam".into(),
                    name: "Demo Camera (synthetic)".into(),
                    kind: quill::calls::engine::MediaDeviceKind::Camera,
                },
                quill::calls::engine::MediaDevice {
                    id: "demo-screen".into(),
                    name: "Demo Screen (synthetic)".into(),
                    kind: quill::calls::engine::MediaDeviceKind::Screen,
                },
            ]);
            self.demo_local_frame = Some(demo_video_frame(true));
            self.demo_screen_frame = Some(QuillApp::demo_screen_frame());
            if let Some(call) = self
                .demo_session
                .as_mut()
                .and_then(|session| session.active_call.as_mut())
            {
                call.remote_video = quill::calls::engine::RemoteVideoState::Active;
                call.remote_screen = quill::calls::engine::RemoteVideoState::Active;
                call.transport = Some(quill::calls::engine::TransportState::Connected);
            }
            self.status_note =
                "screenshot demo — 1:1 peer screen-share receive (injected, no live Telegram)"
                    .into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyCallDevices)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_call_video(session, &self.demo_sink, &self.demo_seq);
                if let Some(call) = session.active_call.as_mut() {
                    call.is_video = false;
                    call.transport = Some(quill::calls::engine::TransportState::Connected);
                }
            }
            // Injected demo devices only (no live Telegram, no real
            // hardware); pre-selects the USB headset pair.
            self.demo_call_devices = Some(vec![
                quill::calls::engine::MediaDevice {
                    id: "default-mic".into(),
                    name: "Default - Built-in Microphone".into(),
                    kind: quill::calls::engine::MediaDeviceKind::Microphone,
                },
                quill::calls::engine::MediaDevice {
                    id: "usb-headset-mic".into(),
                    name: "USB Headset Microphone".into(),
                    kind: quill::calls::engine::MediaDeviceKind::Microphone,
                },
                quill::calls::engine::MediaDevice {
                    id: "default-sp".into(),
                    name: "Default - Built-in Output".into(),
                    kind: quill::calls::engine::MediaDeviceKind::Speaker,
                },
                quill::calls::engine::MediaDevice {
                    id: "usb-headset-sp".into(),
                    name: "USB Headset".into(),
                    kind: quill::calls::engine::MediaDeviceKind::Speaker,
                },
            ]);
            self.demo_selected_devices = (
                Some("usb-headset-mic".into()),
                Some("usb-headset-sp".into()),
            );
            self.status_note =
                "screenshot demo — real-audio device selection (injected, no live Telegram)".into();
        }
        // Phase C2d: reconnecting-audio fixture — a ready voice call
        // whose driver is retrying the retained connect parameters.
        if matches!(demo, Some(ScreenshotDemo::ReadyCallReconnecting)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_call_video(session, &self.demo_sink, &self.demo_seq);
                if let Some(call) = session.active_call.as_mut() {
                    call.is_video = false;
                    call.transport = Some(quill::calls::engine::TransportState::Reconnecting);
                }
            }
            self.status_note =
                "screenshot demo — reconnecting call audio (injected, no live Telegram)".into();
        }
        // Phase B4: chat TTL fixture — the Ready secret chat with a 1h
        // self-destruct timer, a live `auto_delete_in` countdown on one
        // message, the timer-change service row, and the picker expanded.
        if matches!(demo, Some(ScreenshotDemo::ReadyChatTtl)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_chat_ttl(session, &self.demo_sink, &self.demo_seq);
            }
            self.ttl_picker_open = true;
            self.status_note =
                    "screenshot demo — chat self-destruct timer 1h · picker open (injected, no live Telegram)"
                        .into();
        }
        // Notifications and mute: chat 11 muted, the Mute submenu open on
        // the Custom duration row (2 days 3 hours).
        if matches!(demo, Some(ScreenshotDemo::ReadyMuteCustom)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_mute_archive(session, &self.demo_sink, &self.demo_seq);
            }
            self.mute_menu_open = true;
            self.mute_custom_open = true;
            self.mute_custom = quill::mute_menu::CustomMute { days: 2, hours: 3 };
            self.status_note = "screenshot demo — mute menu · custom duration".into();
        }
        // Auto-delete in a regular chat: 1 week timer, picker with the
        // Custom stepper on 2 weeks.
        if matches!(demo, Some(ScreenshotDemo::ReadyAutoDelete)) {
            if let Some(session) = self.demo_session.as_mut()
                && let Some(chat) = session
                    .open_chat
                    .and_then(|id| session.chats.get_mut(&id.0))
            {
                chat.message_auto_delete_time = 604_800;
            }
            self.ttl_picker_open = true;
            self.ttl_custom_open = true;
            self.ttl_custom_secs = 1_209_600;
            self.status_note = "screenshot demo — auto-delete timer · custom".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyGroupCall)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_group_call(session, &self.demo_sink, &self.demo_seq);
                // Phase C2g: synthetic per-participant video frames for
                // the demo tiles (camera for Zed, screen share for Mia).
                // Injected demo data, not real media.
                self.demo_group_frames = demo_group_video_frames();
                // Slice calls-group-self-tile: the self tile renders the
                // local camera preview (fixture camera is on).
                self.demo_local_frame = Some(demo_video_frame(true));
            }
            self.status_note =
                "screenshot demo — group voice chat (injected, no live Telegram)".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyGroupCallInvite)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_group_call_invite(session, &self.demo_sink, &self.demo_seq);
                // Slice calls-group-self-tile: fixture camera is on —
                // the self tile behind the panel renders the preview.
                self.demo_local_frame = Some(demo_video_frame(true));
            }
            self.group_call_invite_open = true;
            self.status_note =
                "screenshot demo — group voice chat invite picker (injected, no live Telegram)"
                    .into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyGroupCallInvitation)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_group_call_invitation(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note =
                "screenshot demo — incoming voice-chat invitation (injected, no live Telegram)"
                    .into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyGroupCallManage)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_group_call_manage(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note =
                    "screenshot demo — voice chat management: title, invite link, recording, RTMP, chat (injected, no live Telegram)"
                        .into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyGroupCallScheduled)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_group_call_scheduled(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note =
                    "screenshot demo — scheduled voice chat: start time, Start now, notify-me toggle (injected, no live Telegram)"
                        .into();
        }
        // Phase C2i: Recent-calls tab with injected `foundMessages`
        // (through the real reducer), privacy values, and the
        // confirm-before-calling pref on.
        if matches!(demo, Some(ScreenshotDemo::ReadyCallsSettings)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_calls_settings(session, &self.demo_sink, &self.demo_seq);
            }
            self.calls_tab_open = true;
            self.status_note =
                "screenshot demo — recent calls + call settings (injected, no live Telegram)"
                    .into();
        }
    }

    /// Screenshot-demo fixture setup (privacy_media): applies the `privacy_media` demo
    /// mutations carved out of `new_with_demo`. Pure code motion.
    pub(super) fn demo_setup_privacy_media(
        &mut self,
        demo: Option<ScreenshotDemo>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Slice S3: Privacy overlay with injected rules, the read-date
        // setting, and the blocked list (no live Telegram).
        if matches!(demo, Some(ScreenshotDemo::ReadyPrivacy)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_privacy(session, &self.demo_sink, &self.demo_seq);
            }
            self.privacy_open = true;
            self.status_note =
                "screenshot demo — privacy settings (injected, no live Telegram)".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyVideoSend)) {
            self.composer.update(cx, |input, cx| {
                input.set_value("sending a clip", window, cx);
            });
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_video_send(session, &self.demo_sink, &self.demo_seq);
            }
            self.playing_video = Some(MessageId(701));
            self.video_frames = vec![
                demo_media_allowlist().join("demo-gif-1.png"),
                demo_media_allowlist().join("demo-gif-2.png"),
            ];
            self.spawn_video_tick(cx);
            self.status_note = "screenshot demo — attach video · own clip playing".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyVideoNoteSend)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_video_note_send(session, &self.demo_sink, &self.demo_seq);
            }
            self.playing_video = Some(MessageId(721));
            self.video_frames = vec![
                demo_media_allowlist().join("demo-gif-1.png"),
                demo_media_allowlist().join("demo-gif-2.png"),
            ];
            self.spawn_video_tick(cx);
            self.status_note = "screenshot demo — video note attach · own round note".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyAlbums)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_albums(session, &self.demo_sink, &self.demo_seq);
            }
            self.composer.update(cx, |input, cx| {
                input.set_value("album caption", window, cx);
            });
            self.status_note = "screenshot demo — received album · own album".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyMentions)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                let dyn_sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
                    self.demo_sink.clone();
                for (id, first, last, username) in [
                    (901, "Ada", "Lovelace", "ada"),
                    (902, "Alan", "Turing", ""),
                    (903, "Grace", "Hopper", "grace"),
                ] {
                    let json = format!(
                        r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":"{first}","last_name":"{last}","usernames":{{"@type":"usernames","active_usernames":["{username}"],"disabled_usernames":[],"editable_username":"{username}","collectible_usernames":[]}},"accent_color_id":{},"type":{{"@type":"userTypeRegular"}},"status":{{"@type":"userStatusRecently"}}}}}}"#,
                        id % 7
                    );
                    if let Some(owned) =
                        quill::telegram::client::copy_and_parse(&json, &self.demo_seq, &dyn_sink)
                    {
                        session.apply(owned);
                    }
                }
                // Demo chat A (11) is the chat the fixture opens.
                session.mention_search = Some(quill::state::MentionSearch {
                    chat_id: session.open_chat.unwrap_or(ChatId(11)),
                    query: "a".into(),
                    user_ids: vec![901, 902, 903],
                    request: None,
                });
            }
            self.composer.update(cx, |input, cx| {
                input.set_value("Thanks @a", window, cx);
            });
            self.status_note = "screenshot demo — @ member suggestions".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyDrafts)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_drafts(session, &self.demo_sink, &self.demo_seq);
            }
            self.restore_open_draft(window, cx);
            self.status_note = "screenshot demo — draft restored".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyLinkPreview)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_link_preview(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note = "screenshot demo — link preview".into();
        }
        // MED4b: composer link-preview chip — type a URL so the
        // detected-URL chip + preview controls render; inject a fake
        // prefetched preview (no live TDLib in demo mode) with large
        // media on offer so the size toggle renders too.
        if matches!(demo, Some(ScreenshotDemo::ReadyComposerPreview)) {
            self.composer.update(cx, |input, cx| {
                input.set_value("see https://example.com/story", window, cx);
            });
            if let Some(session) = self.demo_session.as_mut() {
                session.composer_preview = Some(quill::state::ComposerLinkPreview {
                    url: "https://example.com/story".to_string(),
                    preview: Some(Some(quill::telegram::envelope::LinkPreview {
                        url: "https://example.com/story".to_string(),
                        display_url: "example.com".to_string(),
                        site_name: "Example".to_string(),
                        title: "A short story".to_string(),
                        description: "Telegram-style link preview for a private chat.".to_string(),
                        show_large_media: false,
                        has_large_media: true,
                        show_media_above_description: false,
                        show_above_text: false,
                        instant_view_version: 0,
                        photo: None,
                        kind: quill::telegram::envelope::LinkPreviewKind::EmbeddedPlayer {
                            url: "https://example.com/embed/1".to_string(),
                            duration_secs: 95,
                            audio: false,
                        },
                    })),
                });
            }
            self.status_note = "screenshot demo — composer preview chip".into();
        }
        // MED4: embedded-player + album preview cards.
        if matches!(demo, Some(ScreenshotDemo::ReadyPreviewCards)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_preview_cards(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note = "screenshot demo — preview cards".into();
        }
        // MED4: caption above vs below the media.
        if matches!(demo, Some(ScreenshotDemo::ReadyCaptionPosition)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_caption_position(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note = "screenshot demo — caption position".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyUnsupportedMessage))
            && let Some(session) = self.demo_session.as_mut()
        {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
                self.demo_sink.clone();
            for json in [
                r#"{"@type":"updateNewMessage","message":{"id":110,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageFutureFeature"}}}"#,
                r#"{"@type":"updateNewMessage","message":{"id":111,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageExpiredPhoto"}}}"#,
            ] {
                if let Some(message) =
                    quill::telegram::client::copy_and_parse(json, &self.demo_seq, &sink)
                {
                    session.apply(message);
                }
            }
            session.open_chat(ChatId(11));
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyTextEntities)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_text_entities(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note = "screenshot demo — text entities".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyBlockquoteExpandable)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_blockquote_expandable(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note = "screenshot demo — expandable block quotes".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyServiceMessages)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                super::service_demo::apply_ready_service_messages(
                    session,
                    &self.demo_sink,
                    &self.demo_seq,
                );
            }
            self.status_note = "screenshot demo — service messages".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyReplyKeyboard)) {
            self.demo_setup_reply_keyboard(cx);
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyTranslate)) {
            self.demo_setup_translate(window, cx);
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyThreads)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                let view = std::env::var("QUILL_DEMO_THREADS_VIEW").unwrap_or_default();
                super::threads_demo::apply_ready_threads(
                    session,
                    &self.demo_sink,
                    &self.demo_seq,
                    &view,
                );
            }
            self.status_note = "screenshot demo — comments and threads".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyForumsSaved)) {
            let view = std::env::var("QUILL_DEMO_FORUMS_SAVED_VIEW").unwrap_or_default();
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                super::forums_saved_demo::apply_ready_forums_saved(
                    session,
                    &self.demo_sink,
                    &self.demo_seq,
                    &view,
                );
            }
            if view == "editor" {
                self.open_forum_topic_editor(ChatId(16), None, window, cx);
            }
            self.status_note = "screenshot demo — forums and saved sublists".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyRenderingLeftovers)) {
            let view = std::env::var("QUILL_DEMO_RENDERING_VIEW").unwrap_or_default();
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                super::rendering_demo::apply_ready_rendering_leftovers(
                    session,
                    &self.demo_sink,
                    &self.demo_seq,
                    &view,
                );
            }
            match view.as_str() {
                "viewer" => {
                    self.open_media_viewer(
                        ChatId(super::rendering_demo::CARDS),
                        MessageId(310),
                        cx,
                    );
                }
                "contact" => self.open_share_contact_panel(window, cx),
                "location" => self.open_share_location_panel(window, cx),
                _ => {}
            }
            self.status_note = "screenshot demo — rendering leftovers".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyBubbleHeaders)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                super::bubble_header_demo::apply_ready_bubble_headers(
                    session,
                    &self.demo_sink,
                    &self.demo_seq,
                );
            }
            self.status_note = "screenshot demo — bubble headers".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyShowcase)) {
            use super::showcase_demo as sc;
            let view = std::env::var("QUILL_DEMO_SHOWCASE").unwrap_or_default();
            let scene = match view.as_str() {
                "poll" => sc::Scene::Lunch,
                "player" | "accent" => sc::Scene::Maya,
                "channel" => sc::Scene::Channel,
                _ => sc::Scene::Hikers,
            };
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                sc::apply_ready_showcase(session, &self.demo_sink, &self.demo_seq, scene);
            }
            match view.as_str() {
                "reactions" => {
                    if let Some(session) = self.demo_session.as_mut() {
                        use quill::state::{MessageReactionOptions, ReactionChoice};
                        let emoji = |e: &str| ReactionChoice::Emoji(e.to_string());
                        session.message_reaction_options = Some(MessageReactionOptions {
                            chat_id: ChatId(sc::HIKERS),
                            message_id: MessageId(sc::HIKERS_FIRST_MESSAGE),
                            top: ["❤", "👍", "🔥", "😂", "😮", "😢", "🎉"]
                                .into_iter()
                                .map(emoji)
                                .collect(),
                            recent: vec![emoji("👏")],
                            popular: [
                                "🤔", "🙏", "👌", "😍", "🤯", "😱", "🥰", "🤩", "💯", "⚡", "🏆",
                                "🤝",
                            ]
                            .into_iter()
                            .map(emoji)
                            .collect(),
                            allow_custom_emoji: false,
                        });
                    }
                    self.message_menu = Some(MessageMenuState {
                        chat_id: ChatId(sc::HIKERS),
                        message_id: MessageId(sc::HIKERS_FIRST_MESSAGE),
                        position: point(px(560.), px(150.)),
                    });
                    self.reactions_expanded = true;
                }
                "viewer" => {
                    self.open_media_viewer(
                        ChatId(sc::HIKERS),
                        MessageId(sc::HIKERS_PHOTO_MESSAGE),
                        cx,
                    );
                }
                "player" => {
                    self.begin_track_playback(
                        PlaybackKind::Audio,
                        ChatId(sc::MAYA),
                        MessageId(sc::MAYA_AUDIO_MESSAGE),
                        214.0,
                        87.0,
                        cx,
                    );
                    self.pause_active_playback();
                }
                "appearance" => {
                    self.appearance.theme = ThemeChoice::Dark;
                    self.appearance.accent_rgb = 0x8b5cf6;
                    self.appearance.wallpaper_rgb = Some(0x1b1230);
                    self.appearance_open = true;
                }
                "accent" => {
                    self.appearance.accent_rgb = 0x8b5cf6;
                }
                _ => {}
            }
            // The README captures carry no debug caption.
            self.status_note = String::new();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyRtlPolish)) {
            let view = std::env::var("QUILL_DEMO_RTL_VIEW").unwrap_or_default();
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                super::rtl_demo::apply_ready_rtl_polish(
                    session,
                    &self.demo_sink,
                    &self.demo_seq,
                    &view,
                );
            }
            if view == "search" {
                self.search_input.update(cx, |input, cx| {
                    input.set_value("שלום", window, cx);
                    input.focus(window, cx);
                });
            }
            self.status_note = "screenshot demo — RTL polish".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyRtlComposer)) {
            let text = match std::env::var("QUILL_DEMO_RTL").as_deref() {
                Ok("mixed") => "היי, ההזמנה 12345 מוכנה ב-Telegram Desktop",
                Ok("lines") => "שלום עולם, מה קורה?\nHello world, how are you?\n123",
                Ok("empty") => "",
                _ => "שלום עולם, מה שלומך היום",
            };
            // `QUILL_DEMO_RTL_SELECT=<start>..<end>` (byte offsets) focuses the
            // composer with that range selected, `QUILL_DEMO_RTL_CARET=<at>`
            // with the caret there.
            let select = std::env::var("QUILL_DEMO_RTL_SELECT").ok().and_then(|v| {
                let (a, b) = v.split_once("..")?;
                Some(a.parse::<usize>().ok()?..b.parse::<usize>().ok()?)
            });
            // Right-to-left bubbles beside the composer: an incoming Hebrew
            // message (wraps) and an outgoing mixed one.
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                let dyn_sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
                    self.demo_sink.clone();
                let chat = session.open_chat.unwrap_or(ChatId(11));
                for (id, outgoing, body) in [
                    (
                        901_i64,
                        false,
                        "שלום עולם, מה שלומך היום? זו הודעה ארוכה יותר כדי לראות את הטקסט נשבר לשורות בתוך הבועה.",
                    ),
                    (902, true, "היי, ההזמנה 12345 מוכנה ב-Telegram Desktop"),
                    (904, false, "מחכה לעוד עדכונים ממנה"),
                    (
                        903,
                        false,
                        "קישור https://example.com/he בתוך הודעה ארוכה עם מילה מודגשת וקוד לשורות נוספות בבועה",
                    ),
                ] {
                    // Entities by needle: a link, a bold word and inline code.
                    let entity = |needle: &str, kind: &str| -> Option<String> {
                        let start = body.find(needle)?;
                        let from = quill::text::utf8_to_utf16_offset(body, start).ok()?;
                        let to =
                            quill::text::utf8_to_utf16_offset(body, start + needle.len()).ok()?;
                        Some(format!(
                            r#"{{"@type":"textEntity","offset":{from},"length":{},"type":{{"@type":"{kind}"}}}}"#,
                            to - from
                        ))
                    };
                    let entities = if id == 903 {
                        [
                            entity("https://example.com/he", "textEntityTypeUrl"),
                            entity("מודגשת", "textEntityTypeBold"),
                            entity("וקוד", "textEntityTypeCode"),
                        ]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(",")
                    } else {
                        String::new()
                    };
                    let body = serde_json::to_string(body).unwrap_or_default();
                    let json = format!(
                        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{},"is_outgoing":{outgoing},"date":1700000000,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{body},"entities":[{entities}]}}}}}}}}"#,
                        chat.0
                    );
                    if let Some(owned) =
                        quill::telegram::client::copy_and_parse(&json, &self.demo_seq, &dyn_sink)
                    {
                        session.apply(owned);
                    }
                }
            }
            let caret = std::env::var("QUILL_DEMO_RTL_CARET")
                .ok()
                .and_then(|v| v.parse::<usize>().ok());
            self.composer.update(cx, |input, cx| {
                input.set_value(text, window, cx);
                if let Some(range) = select.clone().or(caret.map(|at| at..at)) {
                    input.focus(window, cx);
                    input.set_selected_range(range, cx);
                }
            });
            self.status_note = "screenshot demo — RTL composer".into();
        }
    }

    /// Screenshot-demo fixture setup (payments): applies the `payments` demo
    /// mutations carved out of `new_with_demo`. Pure code motion.
    pub(super) fn demo_setup_payments(
        &mut self,
        demo: Option<ScreenshotDemo>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(demo, Some(ScreenshotDemo::ReadyPoll)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_poll(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note = "screenshot demo — polls: voted + closed".into();
        }
        // Slice P1: invoice card, payment rows, and the checkout dialog.
        if matches!(demo, Some(ScreenshotDemo::ReadyPayments)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_payments(session, &self.demo_sink, &self.demo_seq);
            }
            self.open_payment_dialog(window, cx);
            self.status_note = "screenshot demo — payments: invoice + checkout".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyLocation)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_location(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note = "screenshot demo — location / venue / contact".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyDice)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_dice(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note = "screenshot demo — dice rolls".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyMediaViewer)) {
            // The Media seed opens chat 11 with a downloaded photo
            // (message 201, "Loaded photo") and a pending one (202, loading
            // state); the document (203) is not viewer-openable.
            self.open_media_viewer(ChatId(11), MessageId(201), cx);
            self.status_note = "screenshot demo — fullscreen media viewer".into();
        }
        if matches!(
            demo,
            Some(ScreenshotDemo::ReadyVideoPlayback | ScreenshotDemo::ReadyVideoPip)
        ) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_video_viewer(session, &self.demo_sink, &self.demo_seq);
            }
            // The Media seed plus a downloaded 12 s video (204, "Demo clip",
            // file 96). The demo extracts + decodes frames synchronously
            // (blocking ~2 s) for a deterministic capture — real in-viewer
            // playback, not faked: the clock keeps ticking and the 125 ms
            // refresh shows the frame for the current clock position.
            // `viewer_demo_sync_frames` suppresses the async extraction that
            // `open_media_viewer` would otherwise start. The audio
            // engine is skipped (demo), like the audio slice.
            self.viewer_demo_sync_frames = true;
            self.open_media_viewer(ChatId(11), MessageId(204), cx);
            if let Some(item) = self.media_viewer.current().cloned()
                && let Some(path) = self.viewer_clip_path(&item)
            {
                // Demo caches use negative IDs, outside TDLib’s live file-ID range.
                let file_id = -item.play_file_id.map(|id| id.0).unwrap_or(0);
                let cache = quill::video::viewer_frame_cache_dir(file_id);
                let mime = item.mime_type.clone().unwrap_or_default();
                let duration = item.duration_secs.unwrap_or(0);
                let start_timestamp = item.start_timestamp.unwrap_or(0);
                if let Ok(viewer_frames) = quill::video::viewer_playback_frames(
                    &path,
                    &mime,
                    &cache,
                    start_timestamp,
                    duration,
                ) && let Ok(decoded) = Self::decode_viewer_frames(&viewer_frames.frames)
                {
                    self.viewer_video_frames = decoded;
                    self.viewer_video_fps = viewer_frames.fps;
                    self.viewer_frame_cache_file = Some(file_id);
                    self.play_viewer_video(&item, &path, cx);
                    if let Some(clock) = self.viewer_clock.as_mut() {
                        clock.seek(5.0);
                    }
                }
            }
            // Keep `viewer_demo_sync_frames` true so no background extraction
            // races the synchronously decoded frames.
            self.status_note = "screenshot demo — in-viewer video playback".into();
            if demo == Some(ScreenshotDemo::ReadyVideoPip) {
                if let Some(clock) = self.viewer_clock.as_mut() {
                    clock.seek(0.0);
                }
                let weak = cx.entity().downgrade();
                cx.defer(move |cx| {
                    let _ = weak.update(cx, |this, cx| this.open_video_pip(cx));
                });
            }
        }
    }

    /// Screenshot-demo fixture setup (stories): applies the `stories` demo
    /// mutations carved out of `new_with_demo`. Pure code motion.
    pub(super) fn demo_setup_stories(
        &mut self,
        demo: Option<ScreenshotDemo>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(demo, Some(ScreenshotDemo::ReadyStories)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_stories(session, &self.demo_sink, &self.demo_seq);
            }
            // Phase 9.1: the tray above the chat list shows the seeded
            // active stories for chats 11/12; the viewer opens on chat 11's
            // downloaded photo story.
            self.open_story_viewer(ChatId(11), 5, cx);
            self.status_note = "screenshot demo — story viewer".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyStoryPost)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_story_post(session, &self.demo_sink, &self.demo_seq);
            }
            // Phase 9.2 / stories-custom-reactions: viewer opens on the
            // seeded own photo story with the reaction picker and the
            // reply row visible, seeded `availableReactions` (emoji +
            // custom-emoji Premium tile with a local sticker thumb), and
            // a chosen ❤ reaction. The composer isn't opened here — it
            // has its own `ReadyStoryComposer` demo (Phase 9.3).
            self.open_story_viewer(ChatId(11), 5, cx);
            self.story_reaction_picker_open = true;
            self.story_reply_open = true;
            self.story_reply_input.update(cx, |input, cx| {
                input.set_value("Great photo!", window, cx);
            });
            self.status_note = "screenshot demo — story reactions / reply / delete".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyStoryViewers)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_story_viewers(session, &self.demo_sink, &self.demo_seq);
            }
            // Phase 9.5: viewer opens on the seeded own photo story with
            // the viewers panel open — the fixture injected a real
            // `storyInteractions` page through the reducer.
            self.open_story_viewer(ChatId(11), 5, cx);
            self.story_viewers_open = true;
            self.status_note = "screenshot demo — story viewers list".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyStoryAlbums)) {
            // Phase 9.7: seed the albums / chat-page / archive fixtures,
            // then open the story page on chat 11 (demo mode — the live
            // notice renders as the status note).
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_story_albums(session, &self.demo_sink, &self.demo_seq);
            }
            self.open_story_page(ChatId(11), window, cx);
            self.status_note = "screenshot demo — story albums / chat page / archive".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyStoryAreas)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                story_areas::apply_ready_story_areas(session, &self.demo_sink, &self.demo_seq);
            }
            // Phase 9.8: viewer opens on the seeded photo story carrying
            // one of every `storyAreaType` — the fixture injected the real
            // `story` (with `areas`) through the reducer.
            self.open_story_viewer(ChatId(11), 5, cx);
            self.status_note = "screenshot demo — clickable story areas".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyStoryComposer)) {
            // Phase 9.3: the composer opens with a seeded photo path (the
            // demo thumbnail, so the preview renders), a caption draft,
            // and the privacy selector on Close friends. (open resets the
            // server-side round-trip state, so eligibility is seeded after.)
            self.open_story_composer(window, cx);
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_stories(session, &self.demo_sink, &self.demo_seq);
                // Phase 9.3: the Saved Messages chat id `postStory` posts
                // to, and a seeded `canPostStoryResultOk` so the status
                // line shows "✓ Eligible to post".
                session.my_user_id = Some(777);
                session.story_post.eligibility =
                    Some(quill::telegram::envelope::CanPostStoryResult::Ok { story_count: 0 });
            }
            self.story_composer_path.update(cx, |input, cx| {
                input.set_value(demo_thumb_png_path(), window, cx);
            });
            self.story_composer_caption.update(cx, |input, cx| {
                input.set_value("Posting my first story **from Quill**!", window, cx);
            });
            self.story_composer.privacy = StoryPrivacy::CloseFriends;
            // Phase 9.4: seed the new options so the screenshot shows
            // them — 48h expiry, both toggles on, a link sticker URL and
            // reaction stickers.
            self.story_composer.expiry = StoryExpiry::TwoDays;
            self.story_composer.post_to_chat_page = true;
            self.story_composer.protect_content = true;
            self.story_composer.link_url = "https://t.me/quill".into();
            self.story_composer.reaction_emojis = "❤️ 🔥".into();
            self.story_composer_link.update(cx, |input, cx| {
                input.set_value("https://t.me/quill", window, cx);
            });
            self.story_composer_reaction.update(cx, |input, cx| {
                input.set_value("❤️ 🔥", window, cx);
            });
            self.status_note = "screenshot demo — story posting composer".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyStoryEdit)) {
            // Phase 9.5: seed the editable own-story fixture, then open
            // the composer in edit mode — caption + area inputs prefill
            // from the cached story, the path stays empty.
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_story_edit(session, &self.demo_sink, &self.demo_seq);
            }
            self.open_story_edit(11, 5, window, cx);
            self.status_note = "screenshot demo — story edit composer".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadySponsored)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_sponsored(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note = "screenshot demo — sponsored messages".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyChannels)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_channels(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note = "screenshot demo — broadcast channel".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyChannelsAdmin)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_channels_admin(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note = "screenshot demo — broadcast channel admin".into();
        }
    }

    /// Screenshot-demo fixture setup (groups_admin): applies the `groups_admin` demo
    /// mutations carved out of `new_with_demo`. Pure code motion.
    pub(super) fn demo_setup_groups_admin(
        &mut self,
        demo: Option<ScreenshotDemo>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Avatar click: group history with a member's profile layer open.
        if matches!(demo, Some(ScreenshotDemo::ReadyAvatarProfile)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                super::profile_modal::apply_ready_avatar_profile(
                    session,
                    &self.demo_sink,
                    &self.demo_seq,
                );
            }
            self.open_avatar_profile(
                quill::telegram::envelope::MessageSender::User { user_id: 602 },
                window,
                cx,
            );
            // Captures show the settled layer, not the fade.
            self.profile_modal = Some(super::profile_modal::ProfileModal::shown(
                InfoPanelTarget::User(602),
            ));
            self.status_note = "screenshot demo — profile layer from an avatar click".into();
        }
        // Phase D2: channel statistics fixture, then open the stats panel
        // directly in the info panel (demo path just sets the target).
        if matches!(demo, Some(ScreenshotDemo::ReadyChannelStats)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_channel_stats(session, &self.demo_sink, &self.demo_seq);
            }
            self.open_info_panel_target(InfoPanelTarget::Statistics(13), window, cx);
            self.status_note = "screenshot demo — channel statistics".into();
        }
        // Phase D3a: invite-links fixture, then open the channel info panel
        // (admin with can_invite_users, seeded by apply_ready_channels_admin).
        if matches!(demo, Some(ScreenshotDemo::ReadyInviteLinks)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_invite_links(session, &self.demo_sink, &self.demo_seq);
            }
            self.open_info_panel_target(InfoPanelTarget::Supergroup(13), window, cx);
            self.status_note = "screenshot demo — invite links".into();
        }
        // Phase D3b: admin-management fixture, then open the channel info
        // panel plus the promote picker (viewer 777 has
        // can_promote_members, seeded by apply_ready_channels_admin).
        if matches!(demo, Some(ScreenshotDemo::ReadyAdminManagement)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_admin_management(session, &self.demo_sink, &self.demo_seq);
            }
            self.open_info_panel_target(InfoPanelTarget::Supergroup(13), window, cx);
            self.admin_dialog = Some(AdminDialog::promote(window, cx, ChatId(13)));
            if let Some(dialog) = self.admin_dialog.as_mut()
                && let AdminDialogKind::Promote { selected_user, .. } = &mut dialog.kind
            {
                *selected_user = Some(5);
            }
            self.status_note = "screenshot demo — admin management".into();
        }
        // Phase D3c: admin-log fixture, then open the channel info panel
        // (viewer 777 is an administrator, seeded by
        // apply_ready_channels_admin).
        if matches!(demo, Some(ScreenshotDemo::ReadyAdminLog)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_admin_log(session, &self.demo_sink, &self.demo_seq);
            }
            self.open_info_panel_target(InfoPanelTarget::Supergroup(13), window, cx);
            self.status_note = "screenshot demo — recent actions".into();
        }
        // Slice G2: channel-management fixture, then open the info panel
        // on the demo channel so the signatures, boost, welcome-message,
        // and recent-actions sections render directly.
        if matches!(demo, Some(ScreenshotDemo::ReadyGroups2)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_groups2(session, &self.demo_sink, &self.demo_seq);
            }
            self.open_info_panel_target(InfoPanelTarget::Supergroup(13), window, cx);
            self.status_note = "screenshot demo — groups/channels G2".into();
            cx.notify();
        }
        // Slice G1: group-management fixture, then open the member
        // dialog on the demo supergroup (viewer 777 is an admin with
        // restrict/invite/tag rights, seeded by
        // apply_ready_group_manage).
        if matches!(demo, Some(ScreenshotDemo::ReadyGroupManage)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_group_manage(session, &self.demo_sink, &self.demo_seq);
            }
            self.member_dialog = Some(MemberDialog::new(window, cx, ChatId(61), false));
            self.status_note = "screenshot demo — group management".into();
            cx.notify();
        }
        // Slice G8: same group-management fixture (viewer 777 is an
        // admin with `can_change_info`), but open the info panel
        // instead of the member dialog so the Edit title / Edit
        // description / Change photo rows render directly.
        if matches!(demo, Some(ScreenshotDemo::ReadyGroupInfoEdit)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_group_manage(session, &self.demo_sink, &self.demo_seq);
            }
            self.open_info_panel_target(InfoPanelTarget::Supergroup(61), window, cx);
            self.status_note = "screenshot demo — group info edit".into();
            cx.notify();
        }
        // Slice G10: community fixtures, then open the new surface.
        if matches!(
            demo,
            Some(
                ScreenshotDemo::ReadyCommunityCreate
                    | ScreenshotDemo::ReadyCommunityHub
                    | ScreenshotDemo::ReadyCommunityInfo
            )
        ) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                community::apply_ready_communities(session, &self.demo_sink, &self.demo_seq);
            }
            match demo {
                Some(ScreenshotDemo::ReadyCommunityCreate) => {
                    self.open_create_community_dialog(window, cx);
                }
                Some(ScreenshotDemo::ReadyCommunityHub) => {
                    self.open_community_hub(cx);
                }
                _ => {
                    self.open_info_panel_target(InfoPanelTarget::Community(9001), window, cx);
                }
            }
            self.status_note = "screenshot demo — communities G10".into();
            cx.notify();
        }
    }

    /// Screenshot-demo fixture setup (bots_profile): applies the `bots_profile` demo
    /// mutations carved out of `new_with_demo`. Pure code motion.
    pub(super) fn demo_setup_bots_profile(
        &mut self,
        demo: Option<ScreenshotDemo>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(demo, Some(ScreenshotDemo::ReadyBotChat)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_bot_chat(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note = "screenshot demo — bot chat with info panel".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyInlineResults)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                super::inline_mode::apply_ready_inline_results(
                    session,
                    &self.demo_sink,
                    &self.demo_seq,
                );
            }
            self.composer.update(cx, |input, cx| {
                input.set_value("@gif cats", window, cx);
            });
            self.sync_inline_mode(cx);
            self.status_note = "screenshot demo — @bot inline results".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyBotKeyboard)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_bot_keyboard(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note =
                "screenshot demo — bot keyboards: inline buttons, custom keyboard, force reply"
                    .into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyRichMessage)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_rich_message(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note = "screenshot demo — rich message blocks".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyRichEditor)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_bot_chat(session, &self.demo_sink, &self.demo_seq);
            }
            self.composer.update(cx, |input, cx| {
                    input.set_value(
                        "# Club night\n\nPick **one**:\n\n- Live set\n- DJ set\n- [] Bring a friend\n\n>> Details\nDoors at 9pm, show at 10pm.\n\n---\nSee you there!",
                        window,
                        cx,
                    );
                });
            self.rich_editor_open = true;
            self.status_note = "screenshot demo — rich editor".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyRichAiTools)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_bot_chat(session, &self.demo_sink, &self.demo_seq);
            }
            self.composer.update(cx, |input, cx| {
                input.set_value(
                    "Please fix this sentance and rewrite it as a short invite.",
                    window,
                    cx,
                );
            });
            self.rich_editor_open = true;
            self.status_note =
                "screenshot demo — rich editor AI tools: Fix · Rewrite · Create".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyRichPremiumGate)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_bot_chat(session, &self.demo_sink, &self.demo_seq);
            }
            // >3 lines so the ⛶ Rich editor button is visible; editor stays
            // closed and the status note shows the non-Premium refusal.
            self.composer.update(cx, |input, cx| {
                input.set_value(
                    "Line one of a long draft\nLine two\nLine three\nLine four — tap Rich editor",
                    window,
                    cx,
                );
            });
            self.rich_editor_open = false;
            self.status_note = "Rich messages require Telegram Premium".into();
        }
        if matches!(
            demo,
            Some(ScreenshotDemo::ReadyProfileEdit | ScreenshotDemo::ReadyUsername)
        ) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_profile_edit(session, &self.demo_sink, &self.demo_seq);
            }
            self.open_edit_profile_dialog(window, cx);
            if matches!(demo, Some(ScreenshotDemo::ReadyUsername)) {
                if let Some(session) = self.demo_session.as_mut() {
                    session.username_check =
                        Some(("newhandle".to_string(), UsernameCheckResult::Available));
                }
                if let Some(dialog) = self.edit_profile_dialog.as_ref() {
                    dialog.username_input.update(cx, |input, cx| {
                        input.set_value("newhandle", window, cx);
                    });
                }
            }
            self.status_note = "screenshot demo — edit profile dialog".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyBotProfile)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_bot_profile(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note = "screenshot demo — B2 bot profile actions".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyBotCommandMenu)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_bot_command_menu(session, &self.demo_sink, &self.demo_seq);
            }
            self.composer.update(cx, |input, cx| {
                input.set_value("/", window, cx);
            });
            self.sync_command_menu(cx);
            self.status_note = "screenshot demo — bot chat with / command menu".into();
        }
    }
}
