//! chat-list behavior: tabs, folders, select mode, archive/pin/unread, community hub.

use super::app::QuillApp;
use super::app::{ChatListFilter, PaneMode};
use super::chat_row::chat_avatar;
use super::chat_row::{
    ChatListItem, chat_list_caption, chat_list_empty_state, chat_list_skeleton_row,
    chat_row_height, chat_row_tags, static_chat_row,
};
use super::demo::{demo_file_json, demo_thumb_png_path};
use super::notifications::notification_settings_json;
use super::pressable::PressableDiv;
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
use quill::telegram::requests::ArchiveChatListSettings;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;
/// `ReadyChatAvatars` fixture (parity slice): chat-list avatars and the
/// channel/supergroup header — all injected through the normal reducer, no
/// live Telegram.
///
/// - `updateChatPhoto` gives "Demo chat A" (private, id 11) and the demo
///   channel (id 13) downloaded `chatPhotoInfo.small` thumbnails (the
///   shared demo-thumb fixture, marked completed).
/// - "Demo chat B" (private, id 12), "Demo basic group" (id 14) and "Demo
///   discussion" (supergroup, id 16) keep no photo → colored-initial
///   fallbacks.
/// - `updateSupergroup` caches the channel's primary `@username`
///   (`demochannel`).
/// - A `getSupergroupFullInfo` round-trip seeds the channel description,
///   12,345 subscribers and `linked_chat_id: 16`, so the header shows the
///   description snippet, the count, and the "Discuss" affordance.
/// - The channel is opened with two broadcast posts.
pub(super) fn apply_ready_chat_avatars(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let thumb_path = demo_thumb_png_path();
    let chat_photo = |file_id: i32| {
        let small = demo_file_json(file_id, &thumb_path, true);
        format!(
            r#"{{"@type":"chatPhotoInfo","small":{small},"big":null,"minithumbnail":null,"has_animation":false,"is_personal":false}}"#
        )
    };
    let position = |chat_id: i64, order: &str| {
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{chat_id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"{order}","is_pinned":false}}}}"#
        )
    };
    let usernames = |names: &[&str]| {
        let active = names
            .iter()
            .map(|n| serde_json::to_string(n).unwrap())
            .collect::<Vec<_>>()
            .join(",");
        format!(
            r#"{{"@type":"usernames","active_usernames":[{active}],"disabled_usernames":[],"editable_username":{first},"collectible_usernames":[]}}"#,
            first = serde_json::to_string(names.first().copied().unwrap_or("")).unwrap(),
        )
    };
    let full_info_extra = session.request_for_supergroup(RequestPurpose::GetSupergroupFullInfo, 13);
    let group_full_info_extra =
        session.request_for_supergroup(RequestPurpose::GetSupergroupFullInfo, 16);
    let description = "Demo channel — product updates, release notes, and the occasional meme. New posts every weekday morning.";
    let description_json = serde_json::to_string(description).unwrap();
    let group_description_json =
        serde_json::to_string("The discussion group for the Demo channel.").unwrap();
    let views = |count: i32| {
        format!(
            r#""interaction_info":{{"@type":"messageInteractionInfo","view_count":{count},"forward_count":7,"reply_info":null,"reactions":null}}"#
        )
    };
    let post = |id: i64, text: &str, view_count: i32| {
        // Phase D2: channel author signatures (`message.author_signature`,
        // schema 1.8.67 line 3165) render below the post.
        let signature = if id == 201 { "Demo Admin" } else { "News Desk" };
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":13,"sender_id":{{"@type":"messageSenderChat","chat_id":13}},"is_outgoing":false,"is_channel_post":true,"author_signature":"{signature}",{},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}}}}}"#,
            views(view_count),
        )
    };
    let jsons = [
        // Photos for the private chat and the channel.
        format!(
            r#"{{"@type":"updateChatPhoto","chat_id":11,"photo":{}}}"#,
            chat_photo(91)
        ),
        format!(
            r#"{{"@type":"updateChatPhoto","chat_id":13,"photo":{}}}"#,
            chat_photo(93)
        ),
        // A basic group (initials fallback) and the discussion supergroup.
        r#"{"@type":"updateNewChat","chat":{"id":14,"title":"Demo basic group","type":{"@type":"chatTypeBasicGroup","basic_group_id":14},"unread_count":0}}"#.to_string(),
        r#"{"@type":"updateNewChat","chat":{"id":16,"title":"Demo discussion","type":{"@type":"chatTypeSupergroup","supergroup_id":16,"is_channel":false},"unread_count":0}}"#.to_string(),
        position(14, "25"),
        position(16, "5"),
        // Channel + discussion-group metadata.
        format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":13,"usernames":{},"is_forum":false,"is_channel":true}}}}"#,
            usernames(&["demochannel"])
        ),
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":16,"usernames":null,"is_forum":false,"is_channel":false}}"#.to_string(),
        format!(
            r#"{{"@type":"supergroupFullInfo","@extra":"{}","description":{description_json},"member_count":12345,"linked_chat_id":16}}"#,
            full_info_extra.0,
        ),
        format!(
            r#"{{"@type":"supergroupFullInfo","@extra":"{}","description":{group_description_json},"member_count":42,"linked_chat_id":0}}"#,
            group_full_info_extra.0,
        ),
        post(201, "Broadcast one — channel post from the channel itself.", 12345),
        post(202, "Broadcast two — a second post with fewer views.", 987),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(ChatId(13));
}

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

/// Slice CL1: chat-list screenshot fixture — chat 11 stays pinned
/// (base seed), chat 12 moves to the archive section, chat 13 is
/// marked as unread (badge dot). Injected, no live Telegram.
pub(super) fn apply_ready_chat_list_menu(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let jsons = [
        r#"{"@type":"updateChatPosition","chat_id":12,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"0","is_pinned":false}}"#
            .to_string(),
        r#"{"@type":"updateChatRemovedFromList","chat_id":12,"chat_list":{"@type":"chatListMain"}}"#
            .to_string(),
        r#"{"@type":"updateChatPosition","chat_id":12,"position":{"@type":"chatPosition","list":{"@type":"chatListArchive"},"order":"20","is_pinned":false}}"#
            .to_string(),
        r#"{"@type":"updateChatAddedToList","chat_id":12,"chat_list":{"@type":"chatListArchive"}}"#
            .to_string(),
        r#"{"@type":"updateChatIsMarkedAsUnread","chat_id":13,"is_marked_as_unread":true}"#.to_string(),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    // The demo seed doesn't carry delete-capability flags; TDLib sends
    // them for real private chats — set directly so the fixture shows
    // the full row menu (Clear history / Delete chat).
    if let Some(chat) = session.chats.get_mut(&11) {
        chat.can_be_deleted_only_for_self = true;
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

/// Slice CL: chat peek-preview screenshot fixture — a few messages on
/// chat 12 ("Demo chat B") so the preview has rows to show; chat 11
/// stays the open chat. All injected through the normal reducer, no
/// live Telegram.
pub(super) fn apply_ready_chat_preview(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let photo_file = demo_file_json(41, "", false);
    let jsons = [
        r#"{"@type":"updateNewMessage","message":{"id":41,"chat_id":12,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Are we still on for lunch tomorrow?","entities":[]}}}}"#
            .to_string(),
        r#"{"@type":"updateNewMessage","message":{"id":42,"chat_id":12,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Yes — noon at the usual place.","entities":[]}}}}"#
            .to_string(),
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":43,"chat_id":12,"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{photo_file},"width":240,"height":160,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"The menu, in case you forgot","entities":[]}},"has_spoiler":false,"is_secret":false}}}}}}"#
        ),
        r#"{"@type":"updateNewMessage","message":{"id":44,"chat_id":12,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"See you there!","entities":[]}}}}"#
            .to_string(),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// Slice CL2: chat-list screenshot fixture — folder tabs (Work/News;
/// the chats stay on Main so the Main tab renders with the category
/// chips), chat 12 moved to the archive (expanded section), chat 11
/// pinned + unread, and the archive auto-settings seeded. All injected
/// through the normal reducer, no live Telegram.
pub(super) fn apply_ready_chat_list(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let folders = r#"{"@type":"updateChatFolders","chat_folders":[{"@type":"chatFolderInfo","id":1,"name":{"@type":"chatFolderName","text":{"@type":"formattedText","text":"Work","entities":[]}},"icon":{"@type":"chatFolderIcon","name":"Work"},"color_id":2,"is_shareable":false,"has_my_invite_links":false},{"@type":"chatFolderInfo","id":2,"name":{"@type":"chatFolderName","text":{"@type":"formattedText","text":"News","entities":[]}},"icon":{"@type":"chatFolderIcon","name":"Channels"},"color_id":4,"is_shareable":false,"has_my_invite_links":false}],"main_chat_list_position":0,"are_tags_enabled":false}"#;
    if let Some(owned) = copy_and_parse(folders, seq, &dyn_sink) {
        session.apply(owned);
    }
    let jsons = [
        r#"{"@type":"updateChatPosition","chat_id":12,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"0","is_pinned":false}}"#.to_string(),
        r#"{"@type":"updateChatRemovedFromList","chat_id":12,"chat_list":{"@type":"chatListMain"}}"#.to_string(),
        r#"{"@type":"updateChatPosition","chat_id":12,"position":{"@type":"chatPosition","list":{"@type":"chatListArchive"},"order":"20","is_pinned":false}}"#.to_string(),
        r#"{"@type":"updateChatAddedToList","chat_id":12,"chat_list":{"@type":"chatListArchive"}}"#.to_string(),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.archive_collapsed = false;
    session.archive_chat_list_settings = Some(ArchiveChatListSettings {
        archive_and_mute_new_chats_from_unknown_users: false,
        keep_unmuted_chats_archived: true,
        keep_chats_from_folders_archived: false,
    });
}

/// Chat-row polish fixture: drafts (with and without a reply), a sending
/// and a failed outgoing message, delivered / read ticks, an online user
/// with Premium, and verified / Premium / SCAM / FAKE titles. All injected
/// through the normal reducer, no live Telegram.
pub(super) fn apply_ready_chat_rows(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let now = quill::local_time::now_unix() - 600;
    let user = |id: i64, first: &str, last: &str, status: &str, extra: &str| {
        format!(
            r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":"{first}","last_name":"{last}","usernames":null,"phone_number":"","status":{status},"profile_photo":null,"is_contact":true,"type":{{"@type":"userTypeRegular"}}{extra}}}}}"#
        )
    };
    let verified = r#","verification_status":{"@type":"verificationStatus","is_verified":true,"is_scam":false,"is_fake":false}"#;
    let scam = r#","verification_status":{"@type":"verificationStatus","is_verified":false,"is_scam":true,"is_fake":false}"#;
    let fake = r#","verification_status":{"@type":"verificationStatus","is_verified":false,"is_scam":false,"is_fake":true}"#;
    let premium = r#","is_premium":true"#;
    let offline = r#"{"@type":"userStatusRecently"}"#;
    let online = r#"{"@type":"userStatusOnline","expires":4102444800}"#;
    let draft = |text: &str, reply: bool| {
        let reply_to = if reply {
            r#"{"@type":"inputMessageReplyToMessage","message_id":5,"quote":null,"checklist_task_id":0,"poll_option_id":""}"#
        } else {
            "null"
        };
        format!(
            r#","draft_message":{{"@type":"draftMessage","reply_to":{reply_to},"date":{now},"content":{{"@type":"draftMessageContentText","text":{{"@type":"formattedText","text":"{text}","entities":[]}},"link_preview_options":null}},"effect_id":"0","suggested_post_info":null}}"#
        )
    };
    let new_chat = |id: i64, title: &str, kind: &str, unread: i32, extra: &str| {
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{kind},"unread_count":{unread}{extra}}}}}"#
        )
    };
    let private = |id: i64| format!(r#"{{"@type":"chatTypePrivate","user_id":{id}}}"#);
    let last = |chat: i64, id: i64, outgoing: bool, state: &str, text: &str, order: i64| {
        format!(
            r#"{{"@type":"updateChatLastMessage","chat_id":{chat},"last_message":{{"id":{id},"chat_id":{chat},"date":{now},"is_outgoing":{outgoing},{state}"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}}},"positions":[{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"{order}","is_pinned":false}}]}}"#
        )
    };
    let pending = r#""sending_state":{"@type":"messageSendingStatePending","sending_id":0},"#;
    let failed = r#""sending_state":{"@type":"messageSendingStateFailed","can_retry":true},"#;
    let step = 1_048_576_i64;
    let jsons = [
        user(21, "Mira", "Cohen", offline, premium),
        new_chat(21, "Mira Cohen", &private(21), 0, &draft("see you at six, bring the notes", false)),
        last(21, 3 * step, false, "", "Are we still on for tonight?", 960),
        user(22, "Noam", "Katz", offline, verified),
        new_chat(22, "Noam Katz", &private(22), 0, ""),
        last(22, -1, true, pending, "On my way, two minutes", 950),
        user(23, "Dana", "Levi", offline, ""),
        new_chat(23, "Dana Levi", &private(23), 0, ""),
        last(23, -2, true, failed, "Did the files arrive?", 940),
        user(24, "Omar", "Haddad", online, premium),
        new_chat(24, "Omar Haddad", &private(24), 0, &draft("", true)),
        last(24, 4 * step, false, "", "Sounds good", 930),
        new_chat(
            124,
            "Quill News",
            r#"{"@type":"chatTypeSupergroup","supergroup_id":124,"is_channel":true}"#,
            3,
            "",
        ),
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":124,"is_channel":true,"verification_status":{"@type":"verificationStatus","is_verified":true,"is_scam":false,"is_fake":false}}}"#.to_string(),
        last(124, 5 * step, false, "", "Release notes for this week are out", 920),
        user(25, "Quick", "Crypto Profit", offline, scam),
        new_chat(25, "Quick Crypto Profit", &private(25), 1, ""),
        last(25, 6 * step, false, "", "Double your coins in 24 hours", 910),
        user(26, "Support", "Desk", offline, fake),
        new_chat(26, "Support Desk", &private(26), 0, ""),
        last(26, 7 * step, false, "", "Please confirm your account", 900),
        user(27, "Yael", "Barak", offline, ""),
        new_chat(27, "Yael Barak", &private(27), 0, ""),
        last(27, 8 * step, true, "", "Delivered, not read yet", 890),
        user(28, "Eli", "Mor", offline, ""),
        new_chat(28, "Eli Mor", &private(28), 0, ""),
        last(28, 9 * step, true, "", "Read by Eli", 880),
        r#"{"@type":"updateChatReadOutbox","chat_id":28,"last_read_outbox_message_id":9437184}"#.to_string(),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
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
        self.flush_leaving_draft(cx);
        // Phase B4: the TTL picker belongs to the previous chat.
        self.ttl_picker_open = false;
        if self
            .pending_reply
            .as_ref()
            .is_some_and(|reply| reply.chat_id != chat_id)
        {
            self.pending_reply = None;
        }
        if self
            .pending_edit
            .as_ref()
            .is_some_and(|edit| edit.chat_id != chat_id)
        {
            self.pending_edit = None;
            self.saved_edit_draft.clear();
            self.saved_edit_reply = None;
        }
        if self
            .pending_delete
            .as_ref()
            .is_some_and(|confirm| confirm.chat_id != chat_id)
        {
            self.pending_delete = None;
        }
        // B4: a pending stop-poll confirm belongs to its own chat.
        if self
            .pending_stop_poll
            .is_some_and(|(id, _, _)| id != chat_id)
        {
            self.pending_stop_poll = None;
        }
        if self
            .pending_forward
            .as_ref()
            .is_some_and(|draft| draft.from_chat_id != chat_id)
        {
            self.pending_forward = None;
            self.forward_picker_open = false;
        }
        if self.recording_active() {
            self.cancel_recording(cx);
        }
        self.stop_voice_playback();
        self.stop_audio_playback();
        self.stop_animation_playback();
        self.autoplayed_gifs.clear();
        self.stop_sticker_playback();
        self.stop_video_playback();
        if self.gif_panel_open() {
            if let Some(live) = self.live.as_mut() {
                live.driver.close_gif_panel();
            } else if let Some(session) = self.demo_session.as_mut() {
                session.gifs.close();
            }
        }
        if self.sticker_panel_open() {
            if let Some(live) = self.live.as_mut() {
                live.driver.close_sticker_panel();
            } else if let Some(session) = self.demo_session.as_mut() {
                session.stickers.close();
            }
        }
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .select_chat(chat_id);
            self.status_note = match result {
                Ok(_) => "".into(),
                Err(_) => "could not open chat".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            session.open_chat(chat_id);
        }
        self.restore_open_draft(window, cx);
        let text = self.composer.read(cx).value().to_string();
        self.sync_composer_typing(&text);
        // Telegram Desktop's info column follows the open chat.
        if self.session().is_some_and(|s| s.open_info_panel.is_some()) {
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
        self.community_ui.hub_open = true;
        cx.notify();
    }

    pub(super) fn close_community_hub(&mut self, cx: &mut Context<Self>) {
        self.community_ui.hub_open = false;
        cx.notify();
    }

    /// Parity slice `parity:communities-chatlist-mode`: enter community
    /// chat-list mode from the hub's "View chats" button. Replaces any
    /// folder tab / category filter (like `Archived` does); fires
    /// `loadCommunityFullInfo` so membership resolves for communities
    /// never opened in the info panel (deduped by the driver).
    pub(super) fn enter_community_chat_list_mode(
        &mut self,
        community_id: i64,
        cx: &mut Context<Self>,
    ) {
        self.folder_tab = None;
        self.contacts_tab_open = false;
        self.chat_filter = ChatListFilter::Community(community_id);
        if let Some(live) = self.live.as_mut()
            && let Err(err) = live.driver.load_community_full_info(community_id)
        {
            self.status_note = format!("community info request failed: {err:?}");
        }
        cx.notify();
    }

    /// Parity slice `parity:communities-chatlist-mode`: leave the mode
    /// (banner ✕). Selecting a folder tab or the Unread/Archived tabs
    /// also exits it — they overwrite `chat_filter`.
    pub(super) fn exit_community_chat_list_mode(&mut self, cx: &mut Context<Self>) {
        if matches!(self.chat_filter, ChatListFilter::Community(_)) {
            self.chat_filter = ChatListFilter::All;
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
            .and_then(|s| s.communities.get(&community_id))
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
        let tabs = self.folder_tabs(cx);
        if let ChatListFilter::Community(community_id) = self.chat_filter {
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(tabs)
                .child(self.community_mode_banner(community_id, cx))
                .into_any_element()
        } else {
            tabs.into_any_element()
        }
    }

    /// Slice CL3: multi-select mode — enter with one chat checked (from
    /// the row-menu "Select" item).
    pub(super) fn enter_select_mode(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        self.selected_chats.insert(chat_id.0);
        cx.notify();
    }

    /// Slice CL3: leave multi-select mode, clearing the checks.
    pub(super) fn exit_select_mode(&mut self, cx: &mut Context<Self>) {
        self.selected_chats.clear();
        cx.notify();
    }

    /// Slice CL3: toggle one row's check in multi-select mode; the last
    /// uncheck exits the mode.
    pub(super) fn toggle_chat_selected(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if !self.selected_chats.remove(&chat_id.0) {
            self.selected_chats.insert(chat_id.0);
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
            self.selected_chats.insert(id);
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
        self.selected_chats.clear();
        cx.notify();
    }

    /// Slice CL3: bulk mark-as-read for the selection, reusing the
    /// single-chat mark-read path (`toggle_chat_marked_as_unread`
    /// does the TGX `viewMessages` flow when there is unread state).
    pub(super) fn mark_selected_read(&mut self, cx: &mut Context<Self>) {
        let ids: Vec<ChatId> = self.selected_chats.iter().map(|id| ChatId(*id)).collect();
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
        self.selected_chats.clear();
        cx.notify();
    }

    /// Slice CL3: bulk mute for the selection, all-or-nothing like TGX:
    /// if any selected chat is unmuted, mute all; otherwise unmute all.
    /// Reuses `apply_chat_mute` (mute-forever / unmute, like the row menu)
    /// per chat that still needs the change.
    pub(super) fn toggle_selected_mute(&mut self, cx: &mut Context<Self>) {
        let chats: Vec<(ChatId, bool)> = self
            .selected_chats
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
        self.selected_chats.clear();
        cx.notify();
    }

    /// Slice CL3: bulk archive for the selection, all-or-nothing like
    /// TGX: if any selected chat is unarchived, archive all; otherwise
    /// unarchive all. Reuses `toggle_archive` per chat that still needs
    /// the change.
    pub(super) fn toggle_selected_archive(&mut self, cx: &mut Context<Self>) {
        let chats: Vec<(ChatId, bool)> = self
            .selected_chats
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
        self.selected_chats.clear();
        cx.notify();
    }

    /// Slice CL3: per-selected-chat pin state, honoring the pinned flag
    /// of the list the chat actually sits in (main vs archive).
    pub(super) fn selected_chat_pin_states(&self) -> Vec<(ChatId, bool)> {
        self.selected_chats
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
        if let Some(first) = self.selected_chats.iter().next().map(|id| ChatId(*id)) {
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
        let selected = if self.contacts_tab_open {
            1
        } else if self.calls_tab_open {
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

    /// Phase 9.1: tdesktop-style active-stories tray above the chat list.
    /// Each entry shows the poster's avatar with an unread (accent) or read
    /// (muted) ring; tapping opens the story viewer on that chat's latest
    /// story (`getStory` prefetches any missing story details first).
    /// Phase 9.3: the row always renders — the leading "+" tile is the
    /// story composer entry point (kept visible even with no active
    /// stories).
    pub(super) fn story_tray(&self, cx: &mut Context<Self>) -> AnyElement {
        let entries: Vec<quill::telegram::envelope::ChatActiveStoriesView> = self
            .session()
            .map(|s| s.ordered_story_tray().into_iter().cloned().collect())
            .unwrap_or_default();
        if entries.is_empty() {
            // Nobody has active stories: the tray collapses. "New story"
            // stays reachable from the main menu.
            return div().into_any_element();
        }
        let mut row = div()
            .id("story-tray")
            .flex()
            .flex_row()
            .flex_wrap()
            .items_start()
            .gap_2()
            .px_3()
            .py_2()
            .child(
                div()
                    .id("story-tray-add")
                    .role(gpui_kit::Role::Button)
                    .aria_label("Create story")
                    .tab_index(0)
                    .cursor_pointer()
                    .pressable(cx.theme())
                    .flex()
                    .flex_col()
                    .items_center()
                    .w(px(60.))
                    .gap_1()
                    .child(
                        div()
                            .rounded_full()
                            .p(px(2.))
                            .border_2()
                            .border_color(cx.theme().primary)
                            .child(
                                div()
                                    .w(px(40.))
                                    .h(px(40.))
                                    .rounded_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_xl()
                                    .text_color(cx.theme().primary)
                                    .child("+"),
                            ),
                    )
                    .child(div().text_xs().child("New story"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_story_composer(window, cx);
                    })),
            );
        for entry in entries {
            let unread = entry.has_unread();
            let chat_id = entry.chat_id;
            let title = self
                .session()
                .and_then(|s| s.chats.get(&chat_id))
                .map(|chat| chat.title.clone())
                .unwrap_or_else(|| format!("Chat {chat_id}"));
            let photo = self
                .session()
                .and_then(|s| s.chat_photo_path(ChatId(chat_id)))
                .and_then(|path| {
                    quill::local_path::sandboxed_display_path(path, &self.media_display_roots())
                });
            let latest_story = entry
                .stories
                .iter()
                .map(|info| info.story_id)
                .max()
                .unwrap_or(0);
            row = row.child(
                div()
                    .id(("story-tray-item", chat_id as u64))
                    .role(gpui_kit::Role::Button)
                    .aria_label(format!("Open stories for {title}"))
                    .tab_index(0)
                    .cursor_pointer()
                    .pressable(cx.theme())
                    .flex()
                    .flex_col()
                    .items_center()
                    .w(px(60.))
                    .gap_1()
                    .child(
                        div()
                            .rounded_full()
                            .p(px(2.))
                            .border_2()
                            .border_color(if unread {
                                cx.theme().accent
                            } else {
                                cx.theme().border
                            })
                            .child(chat_avatar(&title, photo.as_deref(), 40.0)),
                    )
                    .child(div().text_xs().max_w(px(60.)).child(title))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_story_viewer(ChatId(chat_id), latest_story, cx);
                    })),
            );
        }
        row.into_any_element()
    }

    pub(super) fn open_chats_tab(&mut self, cx: &mut Context<Self>) {
        self.contacts_tab_open = false;
        self.calls_tab_open = false;
        cx.notify();
    }

    pub(super) fn open_contacts_tab(&mut self, cx: &mut Context<Self>) {
        self.contacts_tab_open = true;
        self.calls_tab_open = false;
        // Slice A6: the sync toggle gates the `getContacts` refresh —
        // with sync off the tab shows the last loaded snapshot.
        let sync_on = self
            .session()
            .map(|s| s.contact_prefs.sync_enabled)
            .unwrap_or(true);
        if sync_on
            && let Some(live) = self.live.as_mut()
            && let Err(err) = live.driver.fetch_contacts()
        {
            self.status_note = format!("contacts request failed: {err:?}");
        }
        cx.notify();
    }

    /// Phase C2i: open the Recent-calls tab — first page of the
    /// server-side call history plus both call privacy settings. Demo
    /// mode injects synthetic data instead (screenshot proof).
    pub(super) fn open_calls_tab(&mut self, cx: &mut Context<Self>) {
        self.contacts_tab_open = false;
        self.calls_tab_open = true;
        if let Some(live) = self.live.as_mut() {
            if let Err(err) = live.driver.fetch_call_history() {
                self.status_note = format!("call history request failed: {err:?}");
            }
            if let Err(err) = live.driver.fetch_call_privacy() {
                self.status_note = format!("call privacy request failed: {err:?}");
            }
            // Slice S4: the call-settings "Use less data for calls"
            // toggle reads the TDLib-backed per-network settings —
            // fetch the presets so it shows the true value (guarded:
            // once per session).
            let _ = live.driver.fetch_auto_download_presets();
        }
        cx.notify();
    }

    /// kit Phase 3: folder tabs as a kit `TabBar` — `Main` plus the
    /// `updateChatFolders` folders, with the ⋯ manage entry as the bar's
    /// suffix and the Unread/Archived category filters as trailing tabs.
    /// Selecting a folder filters the chat list to `chatListFolder` chats
    /// and fires a single-shot `loadChats(chatListFolder)` when live
    /// (`open_folder_tab`). Slice CL2: the filters filter the loaded model,
    /// never the server query; `Archived` is global, so it leaves any
    /// folder tab. Only rendered when the account actually has folders —
    /// the manage entry stays always present so folders can be created
    /// even when the account has none yet.
    pub(super) fn folder_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let folders: Vec<(i32, String)> = self
            .session()
            .map(|s| {
                s.chat_folders
                    .iter()
                    .map(|f| (f.id, f.name.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let weak = cx.weak_entity();
        // Tab slots: Main, the folders, then the two category filters.
        let mut bar = TabBar::new("folder-tabs").child(Tab::new().label("All"));
        for (_, name) in &folders {
            bar = bar.child(Tab::new().label(name.clone()));
        }
        let unread_ix = folders.len() + 1;
        let archived_ix = folders.len() + 2;
        bar = bar
            .child(Tab::new().label("Unread"))
            .child(Tab::new().label("Archived"));
        let selected = if self.chat_filter == ChatListFilter::Unread {
            unread_ix
        } else if self.chat_filter == ChatListFilter::Archived {
            archived_ix
        } else {
            match self.folder_tab {
                None => 0,
                Some(id) => folders
                    .iter()
                    .position(|(folder_id, _)| *folder_id == id)
                    .map(|pos| pos + 1)
                    .unwrap_or(0),
            }
        };
        // Parity slice: the manage entry is always present so folders can
        // be created even when the account has none yet.
        let manage_weak = weak.clone();
        let folder_ids: Vec<Option<i32>> = std::iter::once(None)
            .chain(folders.iter().map(|(id, _)| Some(*id)))
            .collect();
        bar.selected_index(selected)
            // Many folders: the strip scrolls, tabs truncate long names, and
            // an overflow menu lists every tab by its full name.
            .underline()
            .menu(true)
            .max_width(px(140.))
            .suffix(
                Button::new("folder-manage")
                    .icon(gpui_kit::assets::IconName::Settings)
                    .small()
                    .tooltip("Chat folders")
                    .accessibility_label("Manage chat folders")
                    .ghost()
                    .on_click(move |_, _, cx| {
                        let _ = manage_weak.update(cx, |this, cx| this.open_folder_manage(cx));
                    }),
            )
            .on_click(move |ix, _window, cx| {
                let _ = weak.update(cx, |this, cx| {
                    if *ix == unread_ix {
                        this.chat_filter = ChatListFilter::Unread;
                        cx.notify();
                    } else if *ix == archived_ix {
                        this.chat_filter = ChatListFilter::Archived;
                        this.folder_tab = None;
                        cx.notify();
                    } else if let Some(folder) = folder_ids.get(*ix).copied() {
                        this.open_folder_tab(folder, cx);
                    }
                });
            })
            .into_any_element()
    }

    pub(super) fn open_folder_tab(&mut self, folder: Option<i32>, cx: &mut Context<Self>) {
        self.folder_tab = folder;
        self.chat_filter = ChatListFilter::All;
        self.contacts_tab_open = false;
        if let (Some(live), Some(folder_id)) = (self.live.as_mut(), folder)
            && let Err(err) = live.driver.load_folder_chats(folder_id)
        {
            self.status_note = format!("folder load failed: {err:?}");
        }
        cx.notify();
    }

    /// Slice CL2: pin-drag drop — moves the dragged pinned chat to the
    /// drop target's slot, then sends the full reordered pinned-id list
    /// via `setPinnedChats` (TGX `ChatsAdapter.movePinnedChat`). A drop
    /// onto its own row is a no-op.
    pub(super) fn drop_pinned_chat(
        &mut self,
        dragged: ChatId,
        archived: bool,
        target: ChatId,
        cx: &mut Context<Self>,
    ) {
        if dragged == target {
            return;
        }
        let mut ids = match self.session() {
            Some(s) => s.pinned_chat_ids(archived),
            None => return,
        };
        if !ids.contains(&dragged.0) || !ids.contains(&target.0) {
            return;
        }
        ids.retain(|id| *id != dragged.0);
        let at = ids
            .iter()
            .position(|id| *id == target.0)
            .unwrap_or(ids.len());
        ids.insert(at, dragged.0);
        if let Some(live) = self.live.as_mut() {
            if let Err(err) = live.driver.set_pinned_chat_order(archived, ids) {
                self.status_note = format!("pin reorder failed: {err:?}");
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
                self.status_note = format!("mark all read failed: {err:?}");
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

    /// Slice CL2: archive section collapse toggle.
    pub(super) fn toggle_archive_collapsed(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let collapsed = live.driver.session.archive_collapsed;
            live.driver.session.archive_collapsed = !collapsed;
        } else if let Some(session) = self.demo_session.as_mut() {
            session.archive_collapsed = !session.archive_collapsed;
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
                    self.status_note = "account info not loaded yet".into();
                }
                Ok(None) => {}
                Err(err) => self.status_note = format!("saved messages failed: {err:?}"),
            }
        } else {
            self.status_note = "no Saved Messages chat in this demo".into();
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
            self.status_note = match result {
                Ok(_) if archived => "unarchiving…".into(),
                Ok(_) => "archiving…".into(),
                Err(_) => "could not change archive".into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_archive(chat_id, !archived);
            self.status_note = if archived {
                "unarchived".into()
            } else {
                "archived".into()
            };
            cx.notify();
        }
    }

    /// Slice CL1: Pin / Unpin from the chat-row context menu
    /// (`toggleChatIsPinned`, schema 1.8.67, line 13678). The pin-limit
    /// pre-check mirrors TGX `ChatsController`: count pinned chats of
    /// the same secrecy class in the target list against
    /// `pinned_chat_count_max` / `pinned_archived_chat_count_max`
    /// (`updateOption`); at the limit the request is never sent and the
    /// TGX message shows instead.
    pub(super) fn toggle_chat_pin(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let session = match self.session() {
            Some(session) => session,
            None => return,
        };
        let chat = match session.chats.get(&chat_id.0) {
            Some(chat) => chat,
            None => return,
        };
        let archived = chat.in_archive;
        let pinned = if archived {
            chat.archive_is_pinned
        } else {
            chat.is_pinned
        };
        let secret = matches!(chat.kind, ChatKind::Secret { .. });
        if !pinned {
            let limit = if archived {
                session.pinned_archived_chat_count_max
            } else {
                session.pinned_chat_count_max
            };
            let pinned_count = session
                .chats
                .values()
                .filter(|c| {
                    let is_pinned = if archived {
                        c.in_archive && c.archive_is_pinned
                    } else {
                        c.in_main_list && c.is_pinned
                    };
                    is_pinned && matches!(c.kind, ChatKind::Secret { .. }) == secret
                })
                .count() as i32;
            if pinned_count >= limit.max(0) {
                self.status_note = if archived {
                    format!(
                        "Sorry, you can pin up to {limit} chats and {limit} secret chats at once"
                    )
                } else {
                    format!(
                        "Sorry, you can only pin {limit} chats in your main list. If you're \
                         looking for more organization, try archiving some chats — the \
                         Archived Chats folder allows unlimited pins"
                    )
                };
                cx.notify();
                return;
            }
        }
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .toggle_chat_pin(chat_id, !pinned);
            self.status_note = match result {
                Ok(Some(_)) if pinned => "unpinning…".into(),
                Ok(Some(_)) => "pinning…".into(),
                Ok(None) => "pin request already in flight".into(),
                Err(_) => "could not change pin".into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_pin(chat_id, !pinned, archived);
            self.status_note = if pinned {
                "unpinned".into()
            } else {
                "pinned".into()
            };
            cx.notify();
        }
    }

    /// Slice CL1: Mark as read / Mark as unread from the chat-row
    /// context menu. Mark-as-read follows Telegram X
    /// (`Tdlib.markChatAsRead` with `MessageSourceChatList`):
    /// `viewMessages` over the newest known message reads real unread
    /// history, plus `toggleChatIsMarkedAsUnread(false)` clears the
    /// manual marker. Mark-as-unread is the plain toggle (TGX only
    /// offers it when `unreadCount == 0`, which the menu label gates).
    pub(super) fn toggle_chat_marked_as_unread(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let marked = self
            .session()
            .and_then(|session| session.chats.get(&chat_id.0))
            .is_some_and(|chat| chat.is_marked_as_unread || chat.unread_count > 0);
        // Slice CL1 review nit: server-side unread with no cached history
        // can't be marked read (the driver has nothing to view and sends
        // nothing); say so honestly instead of "request already in flight".
        let nothing_to_view = self.session().is_some_and(|session| {
            session
                .chats
                .get(&chat_id.0)
                .is_some_and(|c| c.unread_count > 0 && !c.is_marked_as_unread)
                && !session
                    .histories
                    .get(&chat_id.0)
                    .is_some_and(|h| !h.messages.is_empty())
        });
        if marked && nothing_to_view && self.live.is_some() {
            self.status_note = "open the chat to mark it as read".into();
            cx.notify();
            return;
        }
        if self.live.is_some() {
            let result = if marked {
                self.live
                    .as_mut()
                    .expect("live")
                    .driver
                    .mark_chat_as_read(chat_id)
            } else {
                self.live
                    .as_mut()
                    .expect("live")
                    .driver
                    .toggle_chat_marked_as_unread(chat_id, true)
            };
            self.status_note = match result {
                Ok(Some(_)) if marked => "marking as read…".into(),
                Ok(Some(_)) => "marking as unread…".into(),
                Ok(None) => "request already in flight".into(),
                Err(_) => "could not change read state".into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_marked_as_unread(chat_id, !marked);
            self.status_note = if marked {
                "marked as read".into()
            } else {
                "marked as unread".into()
            };
            cx.notify();
        }
    }

    // ------------------------------------------------------------------
    // Parity slice: folder management (create / edit / delete / reorder /
    // tags / per-chat membership).
    // ------------------------------------------------------------------

    pub(super) fn sidebar(
        &mut self,
        auth: &AuthView,
        show_phone: bool,
        show_code: bool,
        show_password: bool,
        show_qr: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mode = self.pane_mode();
        let mut list = div()
            .id("sidebar")
            .when(mode != PaneMode::Ready, |this| this.overflow_y_scroll())
            .track_focus(&self.focus_sidebar)
            .w(self.sidebar_width)
            .flex_none()
            .h_full()
            .p_3()
            .border_r_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .when(mode == PaneMode::Ready, |this| {
                        this.child(self.main_navigation_menu(cx))
                    })
                    .child(self.list_tabs(cx)),
            )
            .when_some(
                (!self.contacts_tab_open && !self.calls_tab_open)
                    .then(|| chat_list_caption(mode, self.session()))
                    .flatten(),
                |this, caption| {
                    this.child(
                        div()
                            .px_1()
                            .text_xs()
                            .font_medium()
                            .text_color(cx.theme().muted_foreground)
                            .child(caption),
                    )
                },
            );
        match mode {
            PaneMode::Synthetic => {
                list = list
                    .child(static_chat_row(
                        "Ada Lovelace",
                        "Mixed-height history",
                        true,
                        cx,
                    ))
                    .child(static_chat_row("RTL / emoji samples", "שלום 👨‍👩‍👧‍👦", false, cx));
            }
            PaneMode::Connecting => {
                list = list.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("No chat list until Ready."),
                );
            }
            PaneMode::Ready => {
                if self.contacts_tab_open {
                    list = list.child(
                        div()
                            .id("contacts-scroll")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .child(self.contacts_list(cx)),
                    );
                } else if self.calls_tab_open {
                    list = list.child(
                        div()
                            .id("calls-scroll")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .child(self.calls_list(cx)),
                    );
                } else {
                    // Searching spans every chat: the folder tabs step aside.
                    if !self.search_is_open() {
                        list = list.child(self.folder_tabs_with_community_banner(cx));
                    }
                    list = list.child(self.sidebar_search_field(cx));
                    if self.new_secret_picker_open {
                        list = list.child(self.new_secret_picker_panel(cx));
                    }
                    // Phase 9.1/9.3: tdesktop-style active-stories tray above
                    // the chat rows (leading "+" tile opens the story
                    // composer); omitted for the contacts tab.
                    if !self.search_is_open() {
                        list = list.child(self.story_tray(cx));
                    }
                    if self.search_is_open() {
                        list = list.child(self.search_results(cx));
                    } else {
                        let folder = self.folder_tab;
                        let filter = self.chat_filter;
                        // Parity slice: folder names + tags flag for chat-row
                        // chips.
                        let (folder_names, show_folder_tags) = self
                            .session()
                            .map(|s| {
                                (
                                    s.chat_folders
                                        .iter()
                                        .map(|f| (f.id, f.name.clone()))
                                        .collect::<Vec<_>>(),
                                    s.are_folder_tags_enabled,
                                )
                            })
                            .unwrap_or_default();
                        // Borrowed, never cloned: the list can hold thousands
                        // of chats and this runs every render.
                        let mut chats: Vec<&ChatSummary> = self
                            .session()
                            .map(|s| match folder {
                                Some(folder_id) => s.ordered_folder_chats(folder_id),
                                None => s.ordered_chats(),
                            })
                            .unwrap_or_default();
                        // Slice CL2: the Unread category filters the loaded
                        // model (TGX `ChatFilter` unread predicate:
                        // unread_count > 0 or marked-as-unread,
                        // `ChatFilter.java:166`) — never the server query.
                        if filter == ChatListFilter::Unread {
                            chats.retain(|c| c.is_unread());
                        }
                        // Slice CL2: the Archived category shows only the
                        // archive section.
                        let show_main_list = filter != ChatListFilter::Archived;
                        // Parity slice `parity:communities-chatlist-mode`: narrow
                        // `chats` to the community's chats when the mode is
                        // active. Membership is the cached
                        // `communityFullInfo.chats` pack (state keeps no
                        // per-chat `community_id`); `updateCommunityFullInfo`
                        // replaces the pack wholesale, so a chat leaving the
                        // community drops out on the next render. Hidden
                        // chats (`is_hidden`) stay included — see
                        // `community_mode::community_member_ids`.
                        if let (ChatListFilter::Community(community_id), Some(session)) =
                            (filter, self.session())
                        {
                            community_mode::retain_community_chats(
                                &mut chats,
                                Some(community_id),
                                &session.community_full_infos,
                            );
                        }
                        // Slice CL3: multi-select mode — rows toggle the
                        // check instead of opening the chat. Defined
                        // once here so the select bar, the main loop,
                        // and the archive loop all see it.
                        let selecting = !self.selected_chats.is_empty();
                        // Slice CL3: multi-select action bar (TGX
                        // `ChatsController` selection header): the
                        // selected count, the bulk actions, and cancel.
                        if selecting {
                            let count = self.selected_chats.len();
                            list = list.child(
                                div()
                                    .id("select-bar")
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .bg(cx.theme().accent.opacity(0.12))
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_semibold()
                                            .child(format!("{count} selected")),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .child(
                                                Button::new("select-pin")
                                                    .label("Pin")
                                                    .ghost()
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.toggle_selected_pins(cx);
                                                    })),
                                            )
                                            .child(
                                                Button::new("select-read")
                                                    .label("Read")
                                                    .ghost()
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.mark_selected_read(cx);
                                                    })),
                                            )
                                            .child(
                                                Button::new("select-mute")
                                                    .label("Mute")
                                                    .ghost()
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.toggle_selected_mute(cx);
                                                    })),
                                            )
                                            .child(
                                                Button::new("select-archive")
                                                    .label("Archive")
                                                    .ghost()
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.toggle_selected_archive(cx);
                                                    })),
                                            )
                                            .child(
                                                Button::new("select-unread")
                                                    .label("Select unread")
                                                    .ghost()
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.select_unread_chats(cx);
                                                    })),
                                            )
                                            .child(
                                                Button::new("select-delete")
                                                    .label("Delete")
                                                    .ghost()
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.delete_selected_chats(cx);
                                                    })),
                                            )
                                            .child(
                                                Button::new("select-cancel")
                                                    .label("✕")
                                                    .ghost()
                                                    .tooltip("Exit selection")
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.exit_select_mode(cx);
                                                    })),
                                            ),
                                    ),
                            );
                        }
                        if show_main_list && chats.is_empty() {
                            let loading = self.session().is_some_and(|s| !s.chats_exhausted);
                            list = if loading && folder.is_none() {
                                // kit Phase 9: skeleton rows while the first
                                // chat batch is still loading.
                                let mut loading_list = list;
                                for i in 0..4 {
                                    loading_list = loading_list.child(chat_list_skeleton_row(i));
                                }
                                loading_list
                            } else {
                                let (glyph, title, hint) =
                                    if let ChatListFilter::Community(community_id) = filter {
                                        // Parity slice
                                        // `parity:communities-chatlist-mode`: a
                                        // missing pack means the fetch hasn't
                                        // landed yet (entering the mode fires
                                        // `loadCommunityFullInfo` when live) —
                                        // "Loading…", like the community info
                                        // panel. An empty pack / no matching
                                        // loaded chats is the genuine empty
                                        // state, never a crash.
                                        if self.session().is_some_and(|s| {
                                            !s.community_full_infos.contains_key(&community_id)
                                        }) {
                                            (
                                                "⏳",
                                                "Loading community…",
                                                "Fetching the community's chats.",
                                            )
                                        } else {
                                            (
                                                "👥",
                                                "No chats in this community yet",
                                                "Chats added to the community show up here.",
                                            )
                                        }
                                    } else if filter == ChatListFilter::Unread {
                                        ("🔕", "No unread chats", "You are all caught up.")
                                    } else if folder.is_some() {
                                        (
                                            "📁",
                                            "No chats in this folder yet",
                                            "Add chats to the folder from its settings.",
                                        )
                                    } else {
                                        (
                                            "💬",
                                            "No chats yet",
                                            "Start a conversation to see it here.",
                                        )
                                    };
                                list.child(chat_list_empty_state(glyph, title, hint, cx))
                            };
                        }
                        // kit Phase 3: the chat rows (main list + archive
                        // section) render through a kit `VirtualList` — only
                        // the visible window builds elements. `open`,
                        // photos, pin-drag and multi-select state resolve
                        // per visible row in `chat_list_item_element`.
                        // Rebuilt every render; the list below only reads it.
                        let row_height = |chat: &ChatSummary| {
                            chat_row_height(
                                &chat_row_tags(chat, &folder_names, show_folder_tags),
                                self.appearance.preview_lines,
                            )
                        };
                        let mut items: Vec<ChatListItem> = Vec::with_capacity(chats.len() + 2);
                        if show_main_list {
                            for chat in chats {
                                items.push(ChatListItem::Chat {
                                    id: chat.id,
                                    archived: false,
                                    height: row_height(chat),
                                });
                            }
                        }
                        // Parity slice: folder chats page eagerly — the driver
                        // re-requests `loadChats(chatListFolder)` after each
                        // `ok` until a 404 marks the folder exhausted, the
                        // same pattern as the main list. No "Load more"
                        // button: paging is automatic, not user-triggered.
                        // Archive stays as-is under the main list; a folder
                        // tab shows only that folder's chats. kit Phase 3:
                        // the archive header + rows are items in the same
                        // virtual list so the whole chat list scrolls as one.
                        if folder.is_none() {
                            let mut archived: Vec<&ChatSummary> = self
                                .session()
                                .map(|s| s.ordered_archived_chats())
                                .unwrap_or_default();
                            if filter == ChatListFilter::Unread {
                                archived.retain(|c| c.is_unread());
                            }
                            if !archived.is_empty() || filter == ChatListFilter::Archived {
                                // Slice CL2: the archive header collapses
                                // the section, marks the archive read, and
                                // opens the auto-archive settings. The
                                // Archived category forces the section open.
                                let collapsed = filter != ChatListFilter::Archived
                                    && self.session().is_some_and(|s| s.archive_collapsed);
                                let any_unread = archived.iter().any(|c| c.is_unread());
                                let header = ChatListItem::ArchiveHeader {
                                    count: archived.len(),
                                    any_unread,
                                    collapsed,
                                };
                                items.push(header);
                                if !collapsed {
                                    // Collapsed keeps the rows hidden; the
                                    // header above still shows the count.
                                    if archived.is_empty() {
                                        items.push(ChatListItem::ArchiveEmpty);
                                    } else {
                                        for chat in archived {
                                            items.push(ChatListItem::Chat {
                                                id: chat.id,
                                                archived: true,
                                                height: row_height(chat),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                        // kit Phase 3: hand the flat item list to the kit
                        // `VirtualList`. Item heights are fixed by
                        // construction (see `chat_row_height`), so declared
                        // sizes always match the rendered rows.
                        let sizes: Rc<Vec<ItemSize<Pixels>>> = Rc::new(
                            items
                                .iter()
                                .map(|item| {
                                    let height = match item {
                                        ChatListItem::Chat { height, .. } => *height,
                                        ChatListItem::ArchiveHeader { .. } => px(32.),
                                        ChatListItem::ArchiveEmpty => px(24.),
                                    };
                                    ItemSize::new(px(0.), height)
                                })
                                .collect(),
                        );
                        self.chat_list_items = items;
                        list = list.child(
                            v_virtual_list(
                                cx.entity(),
                                "chat-list",
                                sizes,
                                |this: &mut QuillApp, range, _window, cx| {
                                    range
                                        .map(|ix| this.chat_list_item_element(ix, cx))
                                        .collect::<Vec<_>>()
                                },
                            )
                            // The app-owned handle keeps scroll position
                            // across re-renders (scroll restoration).
                            .track_scroll(&self.chat_list_scroll)
                            .flex_1()
                            .min_h_0()
                            .w_full(),
                        );
                    }
                }
            }
        }
        if mode == PaneMode::Ready {
            return list.into_any_element();
        }
        list = list
            .child(div().mt_4().font_semibold().child("Authorization"))
            .child(
                div()
                    .id("auth-title")
                    .role(Role::Heading)
                    .aria_label(auth.title)
                    .text_sm()
                    .child(auth.title),
            )
            .child(
                div()
                    .id("auth-explanation")
                    .role(Role::Label)
                    .aria_label(auth.body.clone())
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(auth.body.clone()),
            );
        list = list.when(
            matches!(auth.action, quill::auth::AuthAction::Register) && self.live.is_some(),
            |this| this.child(self.registration_form(cx)),
        );
        list.when(
            matches!(auth.action, quill::auth::AuthAction::EnterEmail) && self.live.is_some(),
            |this| {
                this.child(
                    Textarea::new(&self.email_input)
                        .aria_label("Email address")
                        .h(px(40.)),
                )
                .child(
                    Button::new("submit-login-email")
                        .label("Submit email")
                        .on_click(cx.listener(|this, _, window, cx| this.submit_email(window, cx))),
                )
            },
        )
        .when(show_phone, |this| {
            this.child(div().mt_2().font_semibold().text_sm().child("Phone"))
                .child(
                    Textarea::new(&self.phone_input)
                        .aria_label("Phone number")
                        .h(px(40.)),
                )
                .child(
                    Button::new("submit-phone")
                        .label("Submit phone")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.submit_phone(window, cx);
                        })),
                )
        })
        .when(
            quill::auth::can_request_qr_login(&self.current_auth())
                && (self.live.is_some() || self.demo_auth_inputs),
            |this| {
                this.child(
                    Button::new("qr-login")
                        .label("Sign in with QR code")
                        .ghost()
                        .disabled(self.session().is_some_and(|s| s.requests.has_auth_submit()))
                        .on_click(cx.listener(|this, _, _, cx| this.request_qr_login(cx))),
                )
            },
        )
        .when(show_code, |this| {
            this.child(div().mt_2().font_semibold().text_sm().child("Code"))
                .child(
                    Textarea::new(&self.code_input)
                        .aria_label("Sign-in code")
                        .h(px(40.)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            Button::new("submit-code")
                                .label("Submit code")
                                .ghost()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.submit_code(window, cx);
                                })),
                        )
                        .child(
                            Button::new("resend-code")
                                .label("Resend code")
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.resend_code(cx);
                                })),
                        ),
                )
        })
        .when(show_password, |this| {
            // Slice A10: password / recovery-code entry lives in
            // `ui/auth_recovery.rs` (kit-first, named module).
            this.child(self.auth_password_section(cx))
        })
        .when(show_qr, |this| {
            // Slice A1: the QR payload rides on the auth state (envelope.rs
            // carries `link` through); rendered here, never logged.
            let link = match self.current_auth() {
                AuthorizationState::WaitOtherDeviceConfirmation { link } => link,
                _ => String::new(),
            };
            let qr: AnyElement = match self.qr_login_image(&link) {
                Some(image) => img(ImageSource::from(image))
                    .w(px(200.))
                    .h(px(200.))
                    .aspect_ratio(px(200.) / px(200.))
                    .object_fit(ObjectFit::Contain)
                    .into_any_element(),
                None => div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Waiting for the QR payload from Telegram…")
                    .into_any_element(),
            };
            this.child(div().mt_2().font_semibold().text_sm().child("QR code"))
                .child(div().mt_1().child(qr))
                .child(
                    div()
                        .mt_1()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(
                            "Scan with a logged-in Telegram app. The QR payload is never logged.",
                        ),
                )
        })
        .into_any_element()
    }
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
            .chat_list_items
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
