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
    apply_ready_chat_list_menu, apply_ready_chat_preview, apply_ready_mute_archive,
    apply_ready_pin,
};
use super::chat_row::ChatPreviewState;
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
    apply_ready_caption_position, apply_ready_link_preview, apply_ready_preview_cards,
    apply_ready_text_entities,
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
use super::*;
use gpui_kit::*;
use quill::composer::{ComposerEdit, ComposerReplyTo, DeleteConfirm, ForwardDraft};
use quill::ids::{ChatId, MessageId};
use quill::settings::ThemeChoice;
use quill::state::{InfoPanelTarget, SearchStatus};
use quill::story_composer::{StoryExpiry, StoryPrivacy};
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
        if matches!(demo, Some(ScreenshotDemo::ReadyChatsComposer)) {
            self.composer.update(cx, |input, cx| {
                input.set_value("hello from composer", window, cx);
            });
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
        // kit Phase 5: these demos documented the attach-row controls
        // (group-media toggles, self-destruct timer picker, Clear), which
        // now live in the attach menu — keep the menu open for the shot.
        if matches!(
            demo,
            Some(
                ScreenshotDemo::ReadySendMedia
                    | ScreenshotDemo::ReadyVideoSend
                    | ScreenshotDemo::ReadyVideoNoteSend
                    | ScreenshotDemo::ReadyAlbums
                    | ScreenshotDemo::ReadySelfDestruct
            )
        ) {
            self.attach_menu_open = true;
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
        if matches!(demo, Some(ScreenshotDemo::ReadyReactions)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_reactions(session, &self.demo_sink, &self.demo_seq);
            }
            self.pending_react = Some((ChatId(11), MessageId(101)));
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
        if matches!(demo, Some(ScreenshotDemo::ReadyStickers)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_stickers(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note = "screenshot demo — stickers · tap to send".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyVoice)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_voice(session, &self.demo_sink, &self.demo_seq);
            }
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
        if matches!(demo, Some(ScreenshotDemo::ReadyGameCard)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_game_card(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note = "screenshot demo — game card + high scores".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyGifs)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_gifs(session, &self.demo_sink, &self.demo_seq);
            }
            self.playing_animation = Some(MessageId(501));
            self.animation_frames = vec![
                demo_media_allowlist().join("demo-gif-1.png"),
                demo_media_allowlist().join("demo-gif-2.png"),
            ];
            self.spawn_animation_tick(cx);
            self.status_note = "screenshot demo — GIFs · tap to send · playing".into();
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
        if matches!(demo, Some(ScreenshotDemo::ReadySeekBars)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_voice(session, &self.demo_sink, &self.demo_seq);
                apply_ready_audio(session, &self.demo_sink, &self.demo_seq);
            }
            // Fake an in-progress playback without spawning ffplay: voice
            // note 90 (12 s) playing from 5.0 s — the tick advances it —
            // and the music track 801 (214 s) paused with a remembered
            // 1:27 position, so both rows show seek bars.
            self.begin_track_playback(PlaybackKind::Voice, MessageId(90), 12.0, 5.0, cx);
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
        // Settings → Appearance: the Appearance dialog open over the
        // ReadyChats fixture (injected, no live Telegram). Non-default
        // values so the screenshot shows the slice live: dark theme,
        // blue accent, dark wallpaper, 16px message text. They are only
        // in-memory for the demo — `apply_appearance` (end of this fn)
        // picks them up; nothing is persisted.
        if matches!(demo, Some(ScreenshotDemo::ReadyAppearance)) {
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
            self.status_note = "screenshot demo — appearance settings".into();
        }
        // Slice parity:auth-multi-account (UI): the Accounts dialog open
        // over the ReadyChats fixture (injected, no live Telegram). The
        // list reads the real local registry, read-only — nothing is
        // added, switched, or removed by the fixture.
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
        if matches!(demo, Some(ScreenshotDemo::ReadyTextEntities)) {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_text_entities(session, &self.demo_sink, &self.demo_seq);
            }
            self.status_note = "screenshot demo — text entities".into();
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
        if matches!(demo, Some(ScreenshotDemo::ReadyVideoPlayback)) {
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
            // `open_media_viewer` would otherwise start. The ffplay
            // subprocess is skipped (demo), like the audio slice.
            self.viewer_demo_sync_frames = true;
            self.open_media_viewer(ChatId(11), MessageId(204), cx);
            if let Some(item) = self.media_viewer.current().cloned()
                && let Some(path) = self.viewer_clip_path(&item)
            {
                let file_id = item.play_file_id.map(|id| id.0).unwrap_or(0);
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
                input.set_value(&demo_thumb_png_path(), window, cx);
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
            self.sponsored_demo = true;
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
