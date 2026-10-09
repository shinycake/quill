//! group/channel management: create dialogs, info edit, admin helpers, group confirm.

use super::app::QuillApp;
use super::group_invites::apply_ready_admin_log;
use super::message_text::looks_like_emoji;
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, RequestId};
use quill::state::{MemberListFilter, RequestPurpose, Session, WelcomeMessagesFetch};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{
    ChannelMemberStatus, ChatAdminRights, ChatKind, ChatPermissions, MessageContent,
    ParsedWelcomeMessage,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
/// Phase D3b: human labels for the 18 `chatAdministratorRights` fields
/// (TDLib 1.8.67, `schema/td_api.tl:1092`), in schema field order. The
/// checkbox dialogs index this array together with
/// [`admin_right_get`]/[`admin_right_set`].
pub const ADMIN_RIGHT_LABELS: [&str; 18] = [
    "Manage chat",
    "Change info",
    "Post messages",
    "Edit messages",
    "Delete messages",
    "Invite users",
    "Restrict members",
    "Pin messages",
    "Manage topics",
    "Promote members",
    "Manage video chats",
    "Post stories",
    "Edit stories",
    "Delete stories",
    "Manage direct messages",
    "Manage tags",
    "Send welcome messages",
    "Remain anonymous",
];

/// Phase D3b: read one right of a [`ChatAdminRights`] by schema-field
/// index (see [`ADMIN_RIGHT_LABELS`]).
pub fn admin_right_get(rights: &ChatAdminRights, index: usize) -> bool {
    match index {
        0 => rights.can_manage_chat,
        1 => rights.can_change_info,
        2 => rights.can_post_messages,
        3 => rights.can_edit_messages,
        4 => rights.can_delete_messages,
        5 => rights.can_invite_users,
        6 => rights.can_restrict_members,
        7 => rights.can_pin_messages,
        8 => rights.can_manage_topics,
        9 => rights.can_promote_members,
        10 => rights.can_manage_video_chats,
        11 => rights.can_post_stories,
        12 => rights.can_edit_stories,
        13 => rights.can_delete_stories,
        14 => rights.can_manage_direct_messages,
        15 => rights.can_manage_tags,
        16 => rights.can_send_welcome_messages,
        17 => rights.is_anonymous,
        _ => false,
    }
}

/// Phase D3b: write one right of a [`ChatAdminRights`] by schema-field
/// index (see [`ADMIN_RIGHT_LABELS`]).
pub fn admin_right_set(rights: &mut ChatAdminRights, index: usize, value: bool) {
    match index {
        0 => rights.can_manage_chat = value,
        1 => rights.can_change_info = value,
        2 => rights.can_post_messages = value,
        3 => rights.can_edit_messages = value,
        4 => rights.can_delete_messages = value,
        5 => rights.can_invite_users = value,
        6 => rights.can_restrict_members = value,
        7 => rights.can_pin_messages = value,
        8 => rights.can_manage_topics = value,
        9 => rights.can_promote_members = value,
        10 => rights.can_manage_video_chats = value,
        11 => rights.can_post_stories = value,
        12 => rights.can_edit_stories = value,
        13 => rights.can_delete_stories = value,
        14 => rights.can_manage_direct_messages = value,
        15 => rights.can_manage_tags = value,
        16 => rights.can_send_welcome_messages = value,
        17 => rights.is_anonymous = value,
        _ => {}
    }
}

/// Phase D3b: short human summary of a rights set, e.g. "7 of 18
/// rights". Used on the promote picker's selected member and the
/// edit-rights dialog title.
pub fn admin_rights_summary(rights: &ChatAdminRights) -> String {
    let enabled = (0..ADMIN_RIGHT_LABELS.len())
        .filter(|&i| admin_right_get(rights, i))
        .count();
    format!("{enabled} of {} rights", ADMIN_RIGHT_LABELS.len())
}

/// Slice G1: the 16 `chatPermissions` fields (schema 1.8.67, line 1070)
/// with checkbox labels, in schema order.
pub const CHAT_PERMISSION_LABELS: [&str; 16] = [
    "Send messages",
    "Send audios",
    "Send documents",
    "Send photos",
    "Send videos",
    "Send video notes",
    "Send voice notes",
    "Send polls",
    "Send other (stickers/GIFs)",
    "Add link previews",
    "React to messages",
    "Edit tag",
    "Change info",
    "Invite users",
    "Pin messages",
    "Create topics",
];

/// Slice G1: read one permission of a [`ChatPermissions`] by
/// [`CHAT_PERMISSION_LABELS`] index.
pub fn chat_permission_get(permissions: &ChatPermissions, index: usize) -> bool {
    match index {
        0 => permissions.can_send_basic_messages,
        1 => permissions.can_send_audios,
        2 => permissions.can_send_documents,
        3 => permissions.can_send_photos,
        4 => permissions.can_send_videos,
        5 => permissions.can_send_video_notes,
        6 => permissions.can_send_voice_notes,
        7 => permissions.can_send_polls,
        8 => permissions.can_send_other_messages,
        9 => permissions.can_add_link_previews,
        10 => permissions.can_react_to_messages,
        11 => permissions.can_edit_tag,
        12 => permissions.can_change_info,
        13 => permissions.can_invite_users,
        14 => permissions.can_pin_messages,
        15 => permissions.can_create_topics,
        _ => false,
    }
}

/// Slice G1: write one permission of a [`ChatPermissions`] by
/// [`CHAT_PERMISSION_LABELS`] index.
pub fn chat_permission_set(permissions: &mut ChatPermissions, index: usize, value: bool) {
    match index {
        0 => permissions.can_send_basic_messages = value,
        1 => permissions.can_send_audios = value,
        2 => permissions.can_send_documents = value,
        3 => permissions.can_send_photos = value,
        4 => permissions.can_send_videos = value,
        5 => permissions.can_send_video_notes = value,
        6 => permissions.can_send_voice_notes = value,
        7 => permissions.can_send_polls = value,
        8 => permissions.can_send_other_messages = value,
        9 => permissions.can_add_link_previews = value,
        10 => permissions.can_react_to_messages = value,
        11 => permissions.can_edit_tag = value,
        12 => permissions.can_change_info = value,
        13 => permissions.can_invite_users = value,
        14 => permissions.can_pin_messages = value,
        15 => permissions.can_create_topics = value,
        _ => {}
    }
}

/// `ReadyChannels` fixture: open the demo channel (id 13) and inject broadcast
/// posts through the normal reducer — `sender_id: messageSenderChat`,
/// `is_channel_post: true`, `interaction_info.view_count` — plus the
/// `getMe`/`getChatMember` pair that leaves the viewer as a non-member, so
/// the composer is hidden and the Join footer shows. A later
/// `updateMessageInteractionInfo` proves view counts update live.
pub(super) fn apply_ready_channels(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    session.open_chat(ChatId(13));
    let me_extra = session.request(RequestPurpose::GetMe, None);
    let member_extra = session.request(RequestPurpose::GetChatMember, Some(ChatId(13)));
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
        format!(
            r#"{{"@type":"user","@extra":"{}","id":777,"first_name":"Demo","last_name":"Viewer","usernames":null,"phone_number":"","status":null,"profile_photo":null,"is_contact":false,"is_mutual_contact":false,"is_close_friend":false,"is_verified":false,"is_premium":false,"is_support":false,"restriction_reason":"","is_scam":false,"is_fake":false,"is_bot":false,"type":{{"@type":"userTypeRegular"}}}}"#,
            me_extra.0,
        ),
        format!(
            r#"{{"@type":"chatMember","@extra":"{}","member_id":{{"@type":"messageSenderUser","user_id":777}},"tag":"","inviter_user_id":0,"joined_chat_date":0,"status":{{"@type":"chatMemberStatusLeft"}}}}"#,
            member_extra.0,
        ),
        post(201, "Broadcast one — channel post from the channel itself.", 12345),
        post(202, "Broadcast two — a second post with fewer views.", 987),
        // Live view-count bump on the first post.
        r#"{"@type":"updateMessageInteractionInfo","chat_id":13,"message_id":201,"interaction_info":{"@type":"messageInteractionInfo","view_count":12402,"forward_count":7,"reply_info":null,"reactions":null}}"#
            .to_string(),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// `ReadyChannelsAdmin` fixture (Phase 2.3): like `apply_ready_channels`,
/// but the `getMe`/`getChatMember` pair leaves the viewer as an administrator
/// with `rights.can_post_messages: true` (full `chatAdministratorRights`
/// block per schema 1.8.67), so the composer is visible above the broadcast
/// posts. A later `updateMessageInteractionInfo` proves view counts update
/// live.
pub(super) fn apply_ready_channels_admin(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    session.open_chat(ChatId(13));
    let me_extra = session.request(RequestPurpose::GetMe, None);
    let member_extra = session.request(RequestPurpose::GetChatMember, Some(ChatId(13)));
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
    // `chatMemberStatusAdministrator can_be_edited:Bool
    // rights:chatAdministratorRights` — full rights block, `can_post_messages`
    // true (TDLib 1.8.67 `chatAdministratorRights` field order).
    let admin_status = r#"{"@type":"chatMemberStatusAdministrator","can_be_edited":true,"rights":{"@type":"chatAdministratorRights","can_manage_chat":true,"can_change_info":true,"can_post_messages":true,"can_edit_messages":true,"can_delete_messages":true,"can_invite_users":true,"can_restrict_members":true,"can_pin_messages":true,"can_manage_topics":true,"can_promote_members":true,"can_manage_video_chats":true,"can_post_stories":false,"can_edit_stories":false,"can_delete_stories":false,"can_manage_direct_messages":true,"can_manage_tags":false,"can_send_welcome_messages":false,"is_anonymous":false}}"#;
    let jsons = [
        format!(
            r#"{{"@type":"user","@extra":"{}","id":777,"first_name":"Demo","last_name":"Viewer","usernames":null,"phone_number":"","status":null,"profile_photo":null,"is_contact":false,"is_mutual_contact":false,"is_close_friend":false,"is_verified":false,"is_premium":false,"is_support":false,"restriction_reason":"","is_scam":false,"is_fake":false,"is_bot":false,"type":{{"@type":"userTypeRegular"}}}}"#,
            me_extra.0,
        ),
        format!(
            r#"{{"@type":"chatMember","@extra":"{}","member_id":{{"@type":"messageSenderUser","user_id":777}},"tag":"","inviter_user_id":0,"joined_chat_date":0,"status":{}}}"#,
            member_extra.0, admin_status,
        ),
        post(201, "Broadcast one — channel post from the channel itself.", 12345),
        post(202, "Broadcast two — a second post with fewer views.", 987),
        // Live view-count bump on the first post.
        r#"{"@type":"updateMessageInteractionInfo","chat_id":13,"message_id":201,"interaction_info":{"@type":"messageInteractionInfo","view_count":12402,"forward_count":7,"reply_info":null,"reactions":null}}"#
            .to_string(),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// `ReadyGroups2` fixture (Slice G2): on top of `apply_ready_admin_log`
/// (channel 13, viewer 777 admin, loaded event log), flips the channel
/// signature flags on via `updateSupergroup`, grants
/// `can_send_welcome_messages` via `updateSupergroup` admin status, and
/// seeds boost status + a loaded one-message welcome pack directly.
pub(super) fn apply_ready_groups2(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    apply_ready_admin_log(session, sink, seq);
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let jsons = [
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":13,"sign_messages":true,"show_message_sender":false,"status":{"@type":"chatMemberStatusAdministrator","can_be_edited":true}}}"#.to_string(),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.supergroup_send_welcome_right.insert(13, true);
    session.chat_boost_status.insert(13, (4, 38));
    session
        .welcome_message_fetches
        .insert(13, WelcomeMessagesFetch::Loaded);
    session.welcome_messages.insert(
        13,
        vec![ParsedWelcomeMessage {
            id: 5,
            content: MessageContent::Text(quill::telegram::envelope::TextContent {
                text: "Welcome to Demo channel! Read the pinned post first.".to_string(),
                entities: Vec::new(),
                link_preview: None,
            }),
        }],
    );
    session.chat_has_welcome_messages.insert(13, true);
}

/// `ReadyGroupManage` fixture (Slice G1): a demo supergroup ("Demo
/// supergroup", chat id 61, not a channel) with the viewer (777) as an
/// administrator holding `can_restrict_members`, `can_invite_users`,
/// and `can_manage_tags`, plus a loaded `chatMembers` (Recent filter)
/// page: the viewer, an admin with a custom title, a restricted member,
/// and two plain members — all through the real reducer paths, no live
/// Telegram. The caller opens the member dialog on the All tab.
pub(super) fn apply_ready_group_manage(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let chat_id = 61i64;
    let me_extra = session.request(RequestPurpose::GetMe, None);
    let member_extra = session.request(RequestPurpose::GetChatMember, Some(ChatId(chat_id)));
    let members_extra = session.request(
        RequestPurpose::GetSupergroupMembers {
            filter: MemberListFilter::Recent,
        },
        Some(ChatId(chat_id)),
    );
    // TDLib 1.8.67 `chatAdministratorRights` field order.
    let admin_rights = |manage_tags: bool| {
        format!(
            r#"{{"@type":"chatAdministratorRights","can_manage_chat":true,"can_change_info":true,"can_post_messages":true,"can_edit_messages":true,"can_delete_messages":true,"can_invite_users":true,"can_restrict_members":true,"can_pin_messages":true,"can_manage_topics":false,"can_promote_members":false,"can_manage_video_chats":false,"can_post_stories":false,"can_edit_stories":false,"can_delete_stories":false,"can_manage_direct_messages":false,"can_manage_tags":{manage_tags},"can_send_welcome_messages":false,"is_anonymous":false}}"#
        )
    };
    let user = |id: i64, first: &str, last: &str| {
        format!(
            r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":"{first}","last_name":"{last}","usernames":null,"phone_number":"","status":null,"profile_photo":null,"is_contact":false,"is_mutual_contact":false,"is_close_friend":false,"is_verified":false,"is_premium":false,"is_scam":false,"is_fake":false,"is_bot":false,"type":{{"@type":"userTypeRegular"}}}}}}"#
        )
    };
    let member = |id: i64, tag: &str, status: &str| {
        format!(
            r#"{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":{id}}},"tag":"{tag}","inviter_user_id":0,"joined_chat_date":0,"status":{status}}}"#
        )
    };
    let jsons = [
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"Demo supergroup","type":{{"@type":"chatTypeSupergroup","supergroup_id":{chat_id},"is_channel":false}},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{chat_id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"80","is_pinned":false}}}}"#
        ),
        // The viewer's own status: administrator with the rights the
        // gates read (`updateSupergroup` derives rights from the status
        // admin block — the `supergroup` object carries no top-level
        // rights fields, schema 1.8.67 line 2746).
        format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{chat_id},"is_forum":false,"status":{{"@type":"chatMemberStatusAdministrator","can_be_edited":true,"rights":{}}}}}}}"#,
            admin_rights(true),
        ),
        user(777, "Demo", "Viewer"),
        format!(
            r#"{{"@type":"user","@extra":"{}","id":777,"first_name":"Demo","last_name":"Viewer","usernames":null,"phone_number":"","status":null,"profile_photo":null,"is_contact":false,"is_mutual_contact":false,"is_close_friend":false,"is_verified":false,"is_premium":false,"is_support":false,"restriction_reason":"","is_scam":false,"is_fake":false,"is_bot":false,"type":{{"@type":"userTypeRegular"}}}}"#,
            me_extra.0,
        ),
        format!(
            r#"{{"@type":"chatMember","@extra":"{}","member_id":{{"@type":"messageSenderUser","user_id":777}},"tag":"","inviter_user_id":0,"joined_chat_date":0,"status":{{"@type":"chatMemberStatusAdministrator","can_be_edited":true,"rights":{}}}}}"#,
            member_extra.0,
            admin_rights(true),
        ),
        user(1, "Idan", "Founder"),
        user(2, "Dana", "Levi"),
        user(5, "Omar", "Haddad"),
        user(6, "Maya", "Sharon"),
        format!(
            r#"{{"@type":"chatMembers","@extra":"{}","total_count":5,"members":[{},{},{},{},{}]}}"#,
            members_extra.0,
            member(
                777,
                "",
                &format!(
                    r#"{{"@type":"chatMemberStatusAdministrator","can_be_edited":true,"rights":{}}}"#,
                    admin_rights(true)
                ),
            ),
            member(
                1,
                "Founder",
                &format!(
                    r#"{{"@type":"chatMemberStatusAdministrator","can_be_edited":false,"rights":{}}}"#,
                    admin_rights(false)
                ),
            ),
            member(2, "", r#"{"@type":"chatMemberStatusRestricted"}"#),
            member(5, "", r#"{"@type":"chatMemberStatusMember"}"#),
            member(6, "", r#"{"@type":"chatMemberStatusMember"}"#),
        ),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(ChatId(chat_id));
}

impl QuillApp {
    /// Phase D3a: re-request the invite-link list (bypasses the
    /// dedupe cache so the Refresh button always hits the server).
    pub(super) fn refresh_invite_links(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            match live.driver.refresh_chat_invite_links(chat_id) {
                Ok(_) => {}
                Err(_) => {
                    self.status_note = "could not refresh invite links".into();
                }
            }
        } else {
            self.status_note = "invite links need a live connection (demo)".into();
        }
        cx.notify();
    }

    /// Phase D3a: copy an invite-link URL to the clipboard.
    pub(super) fn copy_invite_link(&mut self, invite_link: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(invite_link.to_string()));
        self.status_note = "Invite link copied".into();
        cx.notify();
    }

    /// Phase D3a: revoke an invite link (`revokeChatInviteLink`; TDLib
    /// 1.8.67 has no `deleteChatInviteLink`, so revocation is the only
    /// removal path).
    pub(super) fn revoke_invite_link(
        &mut self,
        chat_id: ChatId,
        invite_link: &str,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            match live.driver.revoke_chat_invite_link(chat_id, invite_link) {
                Ok(_) => {}
                Err(_) => {
                    self.status_note = "could not revoke invite link".into();
                }
            }
        } else {
            self.status_note = "invite links need a live connection (demo)".into();
        }
        cx.notify();
    }

    /// Phase D3a: re-request the join-request list.
    pub(super) fn refresh_join_requests(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            match live.driver.refresh_chat_join_requests(chat_id) {
                Ok(_) => {}
                Err(_) => {
                    self.status_note = "could not refresh join requests".into();
                }
            }
        } else {
            self.status_note = "join requests need a live connection (demo)".into();
        }
        cx.notify();
    }

    /// Phase D3a: approve (`true`) or decline (`false`) a join request.
    pub(super) fn process_join_request(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
        approve: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            match live
                .driver
                .process_chat_join_request(chat_id, user_id, approve)
            {
                Ok(_) => {}
                Err(_) => {
                    self.status_note = "could not process join request".into();
                }
            }
        } else {
            self.status_note = "join requests need a live connection (demo)".into();
        }
        cx.notify();
    }

    /// Phase D3b: re-request the admin list (bypasses the dedupe cache so
    /// the Refresh button always hits the server). No-ops when the
    /// viewer may not manage admins — the gate is deny-by-default.
    pub(super) fn refresh_administrators(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            match live.driver.refresh_chat_administrators(chat_id) {
                Ok(_) => {}
                Err(_) => {
                    self.status_note = "could not refresh administrators".into();
                }
            }
        } else {
            self.status_note = "administrators need a live connection (demo)".into();
        }
        cx.notify();
    }

    /// Phase D3c: re-request the event log (bypasses the dedupe cache so
    /// the Refresh button always hits the server).
    /// Slice G2: channel signature toggles (info panel → Manage
    /// channel). Gated on `chat_can_change_info`; the driver no-ops
    /// (quiet `Ok(None)`) otherwise.
    pub(super) fn set_sign_messages(
        &mut self,
        chat_id: ChatId,
        sign_messages: bool,
        show_message_sender: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            match live
                .driver
                .toggle_sign_messages(chat_id, sign_messages, show_message_sender)
            {
                Ok(_) => self.status_note = "signatures updated".into(),
                Err(_) => self.status_note = "could not change signatures".into(),
            }
        } else {
            self.status_note = "signatures need a live connection (demo)".into();
        }
        cx.notify();
    }

    /// Slice G2: aggressive anti-spam toggle (info panel → Manage
    /// group). Gated on `supergroupFullInfo.can_toggle_aggressive_anti_spam`.
    pub(super) fn set_anti_spam(&mut self, chat_id: ChatId, enabled: bool, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            match live.driver.toggle_aggressive_anti_spam(chat_id, enabled) {
                Ok(_) => self.status_note = "anti-spam updated".into(),
                Err(_) => self.status_note = "could not change anti-spam".into(),
            }
        } else {
            self.status_note = "anti-spam needs a live connection (demo)".into();
        }
        cx.notify();
    }

    /// Slice G1: supergroup id for a chat, if it is one.
    pub(super) fn chat_supergroup_id(&self, chat_id: ChatId) -> Option<i64> {
        self.session()
            .and_then(|session| session.chats.get(&chat_id.0))
            .and_then(|chat| match chat.kind {
                ChatKind::Supergroup { supergroup_id, .. } => Some(supergroup_id),
                _ => None,
            })
    }

    /// Slice G1: cached public username for a supergroup/channel chat.
    pub(super) fn chat_username(&self, chat_id: ChatId) -> String {
        self.chat_supergroup_id(chat_id)
            .and_then(|id| {
                self.session()
                    .and_then(|session| session.supergroup_username(id))
                    .map(str::to_string)
            })
            .unwrap_or_default()
    }

    /// Slice G1: cached join-by-request flag for a supergroup chat.
    pub(super) fn chat_join_by_request(&self, chat_id: ChatId) -> bool {
        self.chat_supergroup_id(chat_id).is_some_and(|id| {
            self.session()
                .is_some_and(|session| session.supergroup_join_by_request.get(&id) == Some(&true))
        })
    }

    /// Slice G1: cached broadcast-group flag for a supergroup chat.
    pub(super) fn chat_is_broadcast(&self, chat_id: ChatId) -> bool {
        self.chat_supergroup_id(chat_id).is_some_and(|id| {
            self.session()
                .is_some_and(|session| session.supergroup_is_broadcast.get(&id) == Some(&true))
        })
    }

    pub(super) fn open_create_chat_dialog(
        &mut self,
        kind: CreateChatKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.create_chat_dialog = Some(CreateChatDialog::new(window, cx, kind));
        cx.notify();
    }

    pub(super) fn close_create_chat_dialog(&mut self, cx: &mut Context<Self>) {
        self.create_chat_dialog = None;
        cx.notify();
    }

    pub(super) fn toggle_create_chat_user(&mut self, user_id: i64, cx: &mut Context<Self>) {
        if let Some(dialog) = self.create_chat_dialog.as_mut() {
            if let Some(position) = dialog.selected_users.iter().position(|id| *id == user_id) {
                dialog.selected_users.remove(position);
            } else {
                dialog.selected_users.push(user_id);
            }
            cx.notify();
        }
    }

    /// Slice G1: submit the creation dialog. Basic groups send their
    /// picked members with the create call; supergroups/channels are
    /// created first and members are added from the member dialog
    /// afterwards (`createNewSupergroupChat` takes no members, schema
    /// 1.8.67 line 13337).
    pub(super) fn submit_create_chat_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.create_chat_dialog.take() else {
            return;
        };
        let title = dialog.title_input.read(cx).value().trim().to_string();
        let description = dialog.description_input.read(cx).value().trim().to_string();
        let kind = dialog.kind;
        let user_ids = dialog.selected_users.clone();
        if title.is_empty() {
            self.create_chat_dialog = Some(dialog);
            self.status_note = "Name cannot be empty".into();
            cx.notify();
            return;
        }
        let note = match self.live.as_mut() {
            Some(live) => {
                let result = match kind {
                    CreateChatKind::BasicGroup => live
                        .driver
                        .create_basic_group(&title, &user_ids)
                        .map(|_| ()),
                    CreateChatKind::Supergroup => live
                        .driver
                        .create_supergroup_channel(&title, false, &description)
                        .map(|_| ()),
                    CreateChatKind::Channel => live
                        .driver
                        .create_supergroup_channel(&title, true, &description)
                        .map(|_| ()),
                };
                match result {
                    Ok(()) => format!("{} created", kind.title()),
                    Err(_) => {
                        self.create_chat_dialog = Some(dialog);
                        format!("could not create {}", kind.title().to_lowercase())
                    }
                }
            }
            None => {
                self.create_chat_dialog = Some(dialog);
                "creating chats needs a live connection (demo)".to_string()
            }
        };
        self.status_note = note;
        cx.notify();
    }

    /// Slice G10: open the "New community" dialog (side-menu entry).
    pub(super) fn open_create_community_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.community_ui.create_dialog = Some(CreateCommunityDialog::new(window, cx));
        cx.notify();
    }

    pub(super) fn close_create_community_dialog(&mut self, cx: &mut Context<Self>) {
        self.community_ui.create_dialog = None;
        cx.notify();
    }

    /// Slice G10: single-select base-chat picker for the create dialog.
    pub(super) fn toggle_create_community_chat(&mut self, chat_id: i64, cx: &mut Context<Self>) {
        if let Some(dialog) = self.community_ui.create_dialog.as_mut() {
            dialog.chat_id = if dialog.chat_id == Some(chat_id) {
                None
            } else {
                Some(chat_id)
            };
            cx.notify();
        }
    }

    /// Slice G10: submit the create-community dialog. Guards mirror the
    /// driver (`create_community` refuses empty names and unknown chats
    /// client-side); the new community arrives via `updateCommunity`.
    pub(super) fn submit_create_community_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.community_ui.create_dialog.take() else {
            return;
        };
        let name = dialog.name_input.read(cx).value().trim().to_string();
        let hide_chat = dialog.hide_chat;
        if name.is_empty() {
            self.community_ui.create_dialog = Some(dialog);
            self.status_note = "Name cannot be empty".into();
            cx.notify();
            return;
        }
        let Some(chat_id) = dialog.chat_id else {
            self.community_ui.create_dialog = Some(dialog);
            self.status_note = "Pick a chat for the community".into();
            cx.notify();
            return;
        };
        let note = match self.live.as_mut() {
            Some(live) => {
                match live
                    .driver
                    .create_community(ChatId(chat_id), &name, hide_chat)
                {
                    Ok(Some(_)) => "Community created".to_string(),
                    _ => {
                        self.community_ui.create_dialog = Some(dialog);
                        "could not create community".to_string()
                    }
                }
            }
            None => {
                self.community_ui.create_dialog = Some(dialog);
                "creating communities needs a live connection (demo)".to_string()
            }
        };
        self.status_note = note;
        cx.notify();
    }

    /// Slice G10: community name edit prompt (`setCommunityName`,
    /// schema 1.8.67 line 11811). Prefilled with the current name.
    pub(super) fn open_community_name_dialog(
        &mut self,
        community_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = self
            .session()
            .and_then(|s| s.communities.get(&community_id))
            .map(|community| community.name.clone())
            .unwrap_or_default();
        // `chat_id` is unused for `CommunityName`; the community id
        // rides the prompt kind.
        self.username_dialog = Some(UsernameDialog::new(
            window,
            cx,
            ChatId(community_id),
            TextPromptKind::CommunityName { community_id },
            &current,
            "Community name",
        ));
        cx.notify();
    }

    pub(super) fn open_username_dialog(
        &mut self,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = self.chat_username(chat_id);
        self.username_dialog = Some(UsernameDialog::new(
            window,
            cx,
            chat_id,
            TextPromptKind::Username,
            &current,
            "Public username (empty = remove)",
        ));
        cx.notify();
    }

    /// Slice G8: group/channel title edit prompt (`setChatTitle`,
    /// schema 1.8.67, line 13430 — 1–128 chars). Prefilled with the
    /// current title.
    pub(super) fn open_group_title_dialog(
        &mut self,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = self
            .session()
            .and_then(|s| s.chats.get(&chat_id.0))
            .map(|chat| chat.title.clone())
            .unwrap_or_default();
        self.username_dialog = Some(UsernameDialog::new(
            window,
            cx,
            chat_id,
            TextPromptKind::GroupTitle,
            &current,
            "Group title (1–128 characters)",
        ));
        cx.notify();
    }

    /// Slice G8: group/channel description edit prompt
    /// (`setChatDescription`, schema 1.8.67, line 13533 — 0–255 chars,
    /// empty clears). Prefilled from the supergroup full info; basic
    /// groups keep no description in state, so the field starts empty
    /// there.
    pub(super) fn open_group_description_dialog(
        &mut self,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = self
            .session()
            .and_then(|s| {
                s.chats.get(&chat_id.0).and_then(|chat| match chat.kind {
                    ChatKind::Supergroup { supergroup_id, .. } => s
                        .supergroup_full_infos
                        .get(&supergroup_id)
                        .map(|info| info.description.clone()),
                    _ => None,
                })
            })
            .unwrap_or_default();
        self.username_dialog = Some(UsernameDialog::new(
            window,
            cx,
            chat_id,
            TextPromptKind::GroupDescription,
            &current,
            "Description (0–255 characters, empty = clear)",
        ));
        cx.notify();
    }

    /// Slice G8: group/channel photo edit prompt (`setChatPhoto`,
    /// schema 1.8.67, line 13435) — a local file path; empty removes
    /// the current photo. No native file picker exists in the app, so
    /// the path is typed like every other text prompt.
    pub(super) fn open_group_photo_dialog(
        &mut self,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.username_dialog = Some(UsernameDialog::new(
            window,
            cx,
            chat_id,
            TextPromptKind::GroupPhoto,
            "",
            "Photo file path (empty = remove current photo)",
        ));
        cx.notify();
    }

    /// Slice G1: admin custom-title prompt (`setChatMemberTag`, schema
    /// 1.8.67, line 13598 — the setter Telegram X's `EditRightsController`
    /// drives for "Custom title"). Basic groups and supergroups only.
    pub(super) fn open_custom_title_dialog(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
        current: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.username_dialog = Some(UsernameDialog::new(
            window,
            cx,
            chat_id,
            TextPromptKind::CustomTitle { user_id },
            current,
            "Custom title (0-16 characters, no emoji; empty = remove)",
        ));
        cx.notify();
    }

    pub(super) fn close_username_dialog(&mut self, cx: &mut Context<Self>) {
        self.username_dialog = None;
        cx.notify();
    }

    pub(super) fn submit_username_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.username_dialog.take() else {
            return;
        };
        let value = dialog.input.read(cx).value().trim().to_string();
        let (kind, chat_id) = (dialog.kind, dialog.chat_id);
        let note = match kind {
            TextPromptKind::Username => match self.live.as_mut() {
                Some(live) => match live.driver.set_supergroup_username(chat_id, &value) {
                    Ok(_) => {
                        if value.is_empty() {
                            "username removed".into()
                        } else {
                            format!("username set to @{value}")
                        }
                    }
                    Err(_) => {
                        self.username_dialog = Some(dialog);
                        "could not set username".into()
                    }
                },
                None => {
                    self.username_dialog = Some(dialog);
                    "usernames need a live connection (demo)".into()
                }
            },
            TextPromptKind::CustomTitle { user_id } => {
                // 0-16 characters, no emoji — schema line 13597, TGX
                // `EditRightsController` enforces the same client-side.
                let too_long = value.chars().count() > 16;
                let has_emoji = value.chars().any(looks_like_emoji);
                if too_long || has_emoji {
                    self.username_dialog = Some(dialog);
                    self.status_note = if too_long {
                        "custom title must be at most 16 characters".into()
                    } else {
                        "custom title cannot contain emoji".into()
                    };
                    cx.notify();
                    return;
                }
                match self.live.as_mut() {
                    Some(live) => match live.driver.set_chat_member_tag(chat_id, user_id, &value) {
                        Ok(_) => {
                            if value.is_empty() {
                                "custom title removed".into()
                            } else {
                                "custom title updated".into()
                            }
                        }
                        Err(_) => {
                            self.username_dialog = Some(dialog);
                            "could not set custom title".into()
                        }
                    },
                    None => {
                        self.username_dialog = Some(dialog);
                        "custom titles need a live connection (demo)".into()
                    }
                }
            }
            TextPromptKind::GroupTitle => {
                // 1–128 chars per the schema (line 13430); the driver
                // re-validates before sending.
                if value.is_empty() || value.chars().count() > 128 {
                    self.username_dialog = Some(dialog);
                    self.status_note = "title must be 1–128 characters".into();
                    cx.notify();
                    return;
                }
                match self.live.as_mut() {
                    Some(live) => match live.driver.set_group_title(chat_id, &value) {
                        Ok(Some(_)) => "title updated".into(),
                        Ok(None) => {
                            self.username_dialog = Some(dialog);
                            "you can't change this group's info".into()
                        }
                        Err(_) => {
                            self.username_dialog = Some(dialog);
                            "could not update title".into()
                        }
                    },
                    None => {
                        self.username_dialog = Some(dialog);
                        "titles need a live connection (demo)".into()
                    }
                }
            }
            // Slice G10: `setCommunityName` (schema 1.8.67, line 11811;
            // the driver refuses empty names client-side). Not
            // optimistic — the new name arrives via `updateCommunity`.
            TextPromptKind::CommunityName { community_id } => {
                if value.is_empty() {
                    self.username_dialog = Some(dialog);
                    self.status_note = "community name cannot be empty".into();
                    cx.notify();
                    return;
                }
                match self.live.as_mut() {
                    Some(live) => match live.driver.set_community_name(community_id, &value) {
                        Ok(_) => "community name updated".into(),
                        Err(_) => {
                            self.username_dialog = Some(dialog);
                            "could not update community name".into()
                        }
                    },
                    None => {
                        self.username_dialog = Some(dialog);
                        "renaming communities needs a live connection (demo)".into()
                    }
                }
            }
            TextPromptKind::GroupDescription => {
                // 0–255 chars per the schema (line 13533); empty clears.
                if value.chars().count() > 255 {
                    self.username_dialog = Some(dialog);
                    self.status_note = "description must be at most 255 characters".into();
                    cx.notify();
                    return;
                }
                match self.live.as_mut() {
                    Some(live) => match live.driver.set_group_description(chat_id, &value) {
                        Ok(Some(_)) => {
                            if value.is_empty() {
                                "description cleared".into()
                            } else {
                                "description updated".into()
                            }
                        }
                        Ok(None) => {
                            self.username_dialog = Some(dialog);
                            "you can't change this group's info".into()
                        }
                        Err(_) => {
                            self.username_dialog = Some(dialog);
                            "could not update description".into()
                        }
                    },
                    None => {
                        self.username_dialog = Some(dialog);
                        "descriptions need a live connection (demo)".into()
                    }
                }
            }
            TextPromptKind::GroupPhoto => {
                // Empty removes the photo; otherwise the path must be
                // a real file — TDLib would reject a missing one anyway.
                // `~` expands to the home dir.
                let value = if let Some(rest) = value.strip_prefix('~') {
                    format!("{}{}", std::env::var("HOME").unwrap_or_default(), rest)
                } else {
                    value
                };
                let photo = if value.is_empty() {
                    None
                } else if std::path::Path::new(&value).is_file() {
                    Some(value.as_str())
                } else {
                    self.username_dialog = Some(dialog);
                    self.status_note = format!("file not found: {value}");
                    cx.notify();
                    return;
                };
                match self.live.as_mut() {
                    Some(live) => match live.driver.set_group_photo(chat_id, photo) {
                        Ok(Some(_)) => {
                            if value.is_empty() {
                                "photo removed".into()
                            } else {
                                "photo updated".into()
                            }
                        }
                        Ok(None) => {
                            self.username_dialog = Some(dialog);
                            "you can't change this group's info".into()
                        }
                        Err(_) => {
                            self.username_dialog = Some(dialog);
                            "could not update photo".into()
                        }
                    },
                    None => {
                        self.username_dialog = Some(dialog);
                        "photos need a live connection (demo)".into()
                    }
                }
            }
        };
        self.status_note = note;
        cx.notify();
    }

    pub(super) fn open_group_confirm(
        &mut self,
        chat_id: ChatId,
        action: GroupConfirmAction,
        cx: &mut Context<Self>,
    ) {
        self.group_confirm_dialog = Some(GroupConfirmDialog { chat_id, action });
        cx.notify();
    }

    /// Slice A6: display name for the user-scoped confirm dialogs
    /// (`DeleteContact`, `BlockContact`) — falls back to "User {id}"
    /// like the info panel does.
    pub(super) fn contact_display_name(&self, user_id: i64) -> String {
        self.session()
            .and_then(|s| s.user(user_id))
            .map(|u| u.display_name())
            .unwrap_or_else(|| format!("User {user_id}"))
    }

    pub(super) fn close_group_confirm(&mut self, cx: &mut Context<Self>) {
        self.group_confirm_dialog = None;
        cx.notify();
    }

    /// Slice G1: run the confirmed action — `deleteChat` (schema
    /// 1.8.67, line 11850; driver checks
    /// `chat.can_be_deleted_for_all_users`), `leaveChat`, or the
    /// one-way `toggleSupergroupIsBroadcastGroup` upgrade (schema
    /// 1.8.67, line 15221).
    pub(super) fn submit_group_confirm(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.group_confirm_dialog.take() else {
            return;
        };
        // A driver's `Ok(None)` means the request was refused or is
        // already in flight — never report it as sent.
        fn sent_note(sent: Option<RequestId>, note: &str) -> String {
            if sent.is_some() {
                note.to_string()
            } else {
                "request already in flight".to_string()
            }
        }
        // Slice B2: read before the live borrow — the RestartBot arm
        // needs the session while `live` is mutably borrowed.
        let restart_bot_user_id = if matches!(dialog.action, GroupConfirmAction::RestartBot) {
            self.session()
                .and_then(|session| session.bot_user_id_for_chat(dialog.chat_id))
        } else {
            None
        };
        let note = match self.live.as_mut() {
            Some(live) => {
                let result = match dialog.action {
                    GroupConfirmAction::DeleteChat => live
                        .driver
                        .delete_chat(dialog.chat_id)
                        .map(|sent| sent_note(sent, "chat deleted")),
                    GroupConfirmAction::LeaveChat => live
                        .driver
                        .leave_channel(dialog.chat_id)
                        .map(|_| "left the chat".to_string()),
                    GroupConfirmAction::BroadcastUpgrade => live
                        .driver
                        .upgrade_to_broadcast_group(dialog.chat_id)
                        // Ongoing, not done: TDLib answers `ok`/`error`
                        // asynchronously; the error arm rolls the
                        // optimistic flag back.
                        .map(|sent| sent_note(sent, "converting to broadcast group…")),
                    // Slice CL1: `deleteChatHistory` (schema 1.8.67,
                    // line 11845). TDLib answers `ok`/`error`
                    // asynchronously; a refusal surfaces via
                    // `Session::chat_action_error`.
                    GroupConfirmAction::ClearHistory { revoke } => live
                        .driver
                        .clear_chat_history(dialog.chat_id, revoke)
                        .map(|sent| sent_note(sent, "clearing history…")),
                    // Slice B2: "Restart bot" — `deleteChatHistory` +
                    // `sendBotStartMessage` (schema lines 11845 / 12216).
                    // The bot user id is read from the session before the
                    // live borrow; a missing id fails closed with a status
                    // note, never silently.
                    GroupConfirmAction::RestartBot => match restart_bot_user_id {
                        Some(bot_user_id) => live
                            .driver
                            .restart_bot(dialog.chat_id, bot_user_id)
                            .map(|sent| sent_note(sent, "restarting bot…")),
                        None => Ok("This chat is no longer a bot chat.".to_string()),
                    },
                    // Slice CL1: chat-list "Delete chat" —
                    // `deleteChatHistory` with `remove_from_chat_list:
                    // true` (Telegram X `Tdlib.deleteChat`).
                    GroupConfirmAction::RemoveFromList => live
                        .driver
                        .remove_chat_from_list(dialog.chat_id)
                        .map(|sent| sent_note(sent, "deleting chat…")),
                    // Slice CL3: row-menu Report (`reportChat`, schema
                    // 1.8.67, line 15693). TDLib answers
                    // `ReportChatResult` asynchronously; the outcome
                    // surfaces via `Session::report_chat_outcome`.
                    GroupConfirmAction::ReportChat => live
                        .driver
                        .report_chat(dialog.chat_id)
                        .map(|sent| sent_note(sent, "reporting chat…")),
                    // Slice CL3: row-menu Block/Unblock
                    // (`setMessageSenderBlockList`, schema 1.8.67, line
                    // 14492). TDLib answers `ok`; the new state arrives
                    // via `updateChatBlockList`.
                    GroupConfirmAction::BlockUser { block } => live
                        .driver
                        .set_chat_user_blocked(dialog.chat_id, block)
                        .map(|sent| {
                            sent_note(
                                sent,
                                if block {
                                    "blocking user…"
                                } else {
                                    "unblocking user…"
                                },
                            )
                        }),
                    // Slice A6: user-panel Block/Unblock — the user-scoped
                    // twin (no chat to resolve through).
                    GroupConfirmAction::BlockContact { user_id, block } => {
                        live.driver.set_user_blocked(user_id, block).map(|sent| {
                            sent_note(
                                sent,
                                if block {
                                    "blocking user…"
                                } else {
                                    "unblocking user…"
                                },
                            )
                        })
                    }
                    // Slice A6: user-panel "Delete contact" —
                    // `removeContacts` (schema 1.8.67, line 14528).
                    GroupConfirmAction::DeleteContact { user_id } => live
                        .driver
                        .remove_contact(user_id)
                        .map(|sent| sent_note(sent, "deleting contact…")),
                    // Slice A6: "Delete synced contacts" —
                    // `clearImportedContacts` + `removeContacts`.
                    GroupConfirmAction::DeleteSyncedContacts => {
                        live.driver.delete_synced_contacts().map(|sent| {
                            if sent > 0 {
                                "deleting synced contacts…".to_string()
                            } else {
                                "nothing to delete".to_string()
                            }
                        })
                    }
                    // Slice CL3: multi-select bulk delete — one
                    // `deleteChatHistory(remove_from_chat_list:true)`
                    // per selected chat; the selection clears on
                    // confirm. The ids come from the live selection
                    // (the dialog blocks selection changes while
                    // open), so the enum stays `Copy`.
                    GroupConfirmAction::RemoveSelectedChats => {
                        let chat_ids: Vec<ChatId> =
                            self.selected_chats.iter().map(|id| ChatId(*id)).collect();
                        let mut sent = 0;
                        for id in &chat_ids {
                            if live
                                .driver
                                .remove_chat_from_list(*id)
                                .is_ok_and(|sent| sent.is_some())
                            {
                                sent += 1;
                            }
                        }
                        let total = chat_ids.len();
                        self.selected_chats.clear();
                        Ok(format!("deleting {sent} of {total} chats…"))
                    }
                    // Slice A2: abort the pending recovery-email setup
                    // (`cancelRecoveryEmailAddressVerification`, schema
                    // 1.8.67, line 11467). The dialog carries no chat, so
                    // `dialog.chat_id` is unused here.
                    GroupConfirmAction::AbortRecoveryEmailSetup => live
                        .driver
                        .cancel_recovery_email_setup()
                        .map(|_| "aborting email setup…".to_string()),
                    GroupConfirmAction::RemoveSavedGif { file_id } => live
                        .driver
                        .set_gif_saved(file_id, false)
                        .map(|id| sent_note(id, "removing saved GIF…")),
                    GroupConfirmAction::DeleteForumTopic { forum_topic_id } => live
                        .driver
                        .delete_forum_topic(dialog.chat_id, forum_topic_id)
                        .map(|_| "deleting topic…".to_string()),
                    GroupConfirmAction::RemoveInstalledStickerSets => {
                        let ids: Vec<_> = live
                            .driver
                            .session
                            .stickers
                            .sets
                            .iter()
                            .map(|set| set.id)
                            .collect();
                        live.driver
                            .manage_sticker_sets(&ids, false)
                            .map(|sent| format!("removing {sent} sticker sets…"))
                    }
                    GroupConfirmAction::RemoveEmojiSet { set_id } => live
                        .driver
                        .set_emoji_pack_installed(set_id, false)
                        .map(|id| sent_note(id, "removing emoji pack…")),
                    GroupConfirmAction::RemoveStickerSet { set_id } => live
                        .driver
                        .manage_sticker_set(set_id, false, false)
                        .map(|id| sent_note(id, "removing sticker set…")),
                    // Clearing stored payment information waits for TDLib confirmation.
                    GroupConfirmAction::ClearPaymentInfo => live
                        .driver
                        .clear_saved_payment_info()
                        .map(|_| "clearing saved payment info…".to_string()),
                    GroupConfirmAction::RemoveMember { user_id } => live
                        .driver
                        .remove_chat_member(dialog.chat_id, user_id)
                        .map(|id| sent_note(id, "removing member…")),
                };
                match result {
                    Ok(note) => note,
                    Err(_) => {
                        self.group_confirm_dialog = Some(dialog);
                        "action failed".to_string()
                    }
                }
            }
            None => {
                self.group_confirm_dialog = Some(dialog);
                "chat actions need a live connection (demo)".to_string()
            }
        };
        self.status_note = note;
        cx.notify();
    }

    /// kit Phase 2 (redo): create-chat hosted in a kit `Dialog` via
    /// `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    pub(super) fn build_create_chat_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::CreateChat, |this, _, cx| {
                this.close_create_chat_dialog(cx);
            });
        app.update(cx, |this, cx| {
            let dialog = dialog.overlay(true);
            let Some(dialog_state) = this.create_chat_dialog.as_ref() else {
                return dialog
                    .title(crate::ui::shell::dialog_title("New chat"))
                    .on_close(on_close);
            };
            let kind = dialog_state.kind;
            let picks_members = kind.picks_members();
            let query = dialog_state.search_input.read(cx).value();
            let rows = if picks_members {
                this.g1_contact_rows(&query, cx)
            } else {
                Vec::new()
            };
            let selected = dialog_state.selected_users.clone();
            let mut body = div().flex().flex_col().gap_2();
            body = body
                .child(
                    div().flex_1().child(
                        Textarea::new(&dialog_state.title_input)
                            .aria_label("Group or channel title")
                            .h(px(40.)),
                    ),
                )
                .when(kind != CreateChatKind::BasicGroup, |this| {
                    this.child(
                        div().flex_1().child(
                            Textarea::new(&dialog_state.description_input)
                                .aria_label("Group or channel description")
                                .h(px(64.)),
                        ),
                    )
                });
            if picks_members {
                body = body.child(
                    div().flex().items_center().gap_2().child(
                        div().flex_1().child(
                            Textarea::new(&dialog_state.search_input)
                                .aria_label("Search people to invite")
                                .h(px(40.)),
                        ),
                    ),
                );
                let mut list = div()
                    .id("g1-create-contacts")
                    .flex()
                    .flex_col()
                    .gap_1()
                    .max_h(px(220.))
                    .overflow_y_scroll();
                if rows.is_empty() {
                    list = list.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("No contacts found"),
                    );
                }
                for row in rows.iter().take(50) {
                    let is_selected = selected.contains(&row.user_id);
                    list = list.child(this.g1_contact_checkbox(
                        "g1-create".to_string(),
                        row,
                        is_selected,
                        row.user_id,
                        cx,
                    ));
                }
                body = body.child(list);
                if !selected.is_empty() {
                    body = body.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{} members selected", selected.len())),
                    );
                }
            }
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("g1-create-cancel")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_create_chat_dialog(cx);
                            this.close_kit_dialog_if_done(DialogKind::CreateChat, window, cx);
                        })),
                )
                .child(
                    Button::new("g1-create-submit")
                        .label(format!("Create {}", kind.title().to_lowercase()))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.submit_create_chat_dialog(cx);
                            this.close_kit_dialog_if_done(DialogKind::CreateChat, window, cx);
                        })),
                );
            let body = body.into_any_element();
            dialog
                .title(crate::ui::shell::dialog_title(kind.title()))
                .content(crate::ui::shell::scrollable_dialog_content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }

    /// kit Phase 2 (redo): username editor hosted in a kit `Dialog` via
    /// `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    pub(super) fn build_username_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::Username, |this, _, cx| {
                this.close_username_dialog(cx);
            });
        app.update(cx, |this, cx| {
            let dialog = dialog.overlay(true);
            let Some(dialog_state) = this.username_dialog.as_ref() else {
                return dialog
                    .title(crate::ui::shell::dialog_title("Public username"))
                    .on_close(on_close);
            };
            let (title, hint) = match dialog_state.kind {
                TextPromptKind::Username => (
                    "Public username",
                    "Public link t.me/username — empty removes it",
                ),
                TextPromptKind::CustomTitle { .. } => (
                    "Custom title",
                    "Admin title shown instead of \"admin\" — empty removes it",
                ),
                // Slice G8: group/channel info editing reuses the text
                // prompt — each kind gets its own title and hint.
                TextPromptKind::GroupTitle => ("Group title", "1–128 characters"),
                TextPromptKind::GroupDescription => (
                    "Group description",
                    "Up to 255 characters — empty clears it",
                ),
                TextPromptKind::GroupPhoto => (
                    "Group photo",
                    "Path to an image file — empty removes the photo",
                ),
                // Slice G10: community rename prompt.
                TextPromptKind::CommunityName { .. } => (
                    "Community name",
                    "Shown in the communities hub and info panel",
                ),
            };
            let body = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(hint),
                )
                .child(
                    div().flex_1().child(
                        Textarea::new(&dialog_state.input)
                            .aria_label("Public username")
                            .h(px(40.)),
                    ),
                )
                .into_any_element();
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("g1-username-cancel")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_username_dialog(cx);
                            this.close_kit_dialog_if_done(DialogKind::Username, window, cx);
                        })),
                )
                .child(
                    Button::new("g1-username-submit")
                        .label("Save")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.submit_username_dialog(cx);
                            this.close_kit_dialog_if_done(DialogKind::Username, window, cx);
                        })),
                );
            dialog
                .title(crate::ui::shell::dialog_title(title))
                .content(crate::ui::shell::scrollable_dialog_content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }

    /// kit Phase 2 (redo): group confirm hosted in a kit `Dialog` via
    /// `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    pub(super) fn build_group_confirm_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::GroupConfirm, |this, _, cx| {
                this.close_group_confirm(cx);
            });
        app.update(cx, |this, cx| {
            let dialog = dialog.overlay(true);
            let Some(dialog_state) = this.group_confirm_dialog.as_ref() else {
                return dialog.title(crate::ui::shell::dialog_title("Confirm")).on_close(on_close);
            };
            let (title, message, confirm_label): (String, String, String) =
                match dialog_state.action {
                    GroupConfirmAction::DeleteChat => (
                        "Delete group".to_string(),
                        "Delete this group for everyone? This cannot be undone.".to_string(),
                        "Delete".to_string(),
                    ),
                    GroupConfirmAction::RemoveFromList => (
                        "Delete chat".to_string(),
                        "Delete this chat and its history from your chat list?".to_string(),
                        "Delete".to_string(),
                    ),
                    GroupConfirmAction::ReportChat => (
                        "Report chat".to_string(),
                        "Report this chat to Telegram moderators as spam?".to_string(),
                        "Report".to_string(),
                    ),
                    GroupConfirmAction::BlockUser { block } => {
                        if block {
                            (
                                "Block user".to_string(),
                                "Block this user? They won't be able to send you messages."
                                    .to_string(),
                                "Block".to_string(),
                            )
                        } else {
                            (
                                "Unblock user".to_string(),
                                "Unblock this user?".to_string(),
                                "Unblock".to_string(),
                            )
                        }
                    }
                    GroupConfirmAction::RemoveSelectedChats => (
                        "Delete chats".to_string(),
                        "Delete the selected chats and their history from your chat list?"
                            .to_string(),
                        "Delete".to_string(),
                    ),
                    GroupConfirmAction::LeaveChat => (
                        "Leave chat".to_string(),
                        "Leave this chat? You can rejoin with an invite link.".to_string(),
                        "Leave".to_string(),
                    ),
                    GroupConfirmAction::BroadcastUpgrade => (
                        "Convert to broadcast group".to_string(),
                        "Only admins will be able to post. Non-admin members become \
                         subscribers. This cannot be undone."
                            .to_string(),
                        "Convert".to_string(),
                    ),
                    GroupConfirmAction::ClearHistory { revoke } => (
                        "Clear history".to_string(),
                        if revoke {
                            "Delete all messages in this chat for everyone? This cannot be undone."
                        } else {
                            "Delete all messages in this chat for you? This cannot be undone."
                        }
                        .to_string(),
                        "Clear".to_string(),
                    ),
                    GroupConfirmAction::RestartBot => (
                        "Restart bot".to_string(),
                        "Clear this bot's chat history and send /start again? This cannot be undone."
                            .to_string(),
                        "Restart".to_string(),
                    ),
                    GroupConfirmAction::AbortRecoveryEmailSetup => (
                        "Abort recovery email setup".to_string(),
                        "Are you sure you want to abort recovery email setup? The new address will not be activated."
                            .to_string(),
                        "Abort".to_string(),
                    ),
                    GroupConfirmAction::BlockContact { user_id, block } => {
                        let name = this.contact_display_name(user_id);
                        if block {
                            (
                                "Block user".to_string(),
                                format!("Are you sure you want to block {name}?"),
                                "Block".to_string(),
                            )
                        } else {
                            (
                                "Unblock user".to_string(),
                                format!("Unblock {name}?"),
                                "Unblock".to_string(),
                            )
                        }
                    }
                    GroupConfirmAction::DeleteContact { user_id } => {
                        let name = this.contact_display_name(user_id);
                        (
                            "Delete contact".to_string(),
                            format!("Delete {name} from contacts?"),
                            "Delete".to_string(),
                        )
                    }
                    GroupConfirmAction::DeleteSyncedContacts => (
                        "Delete synced contacts".to_string(),
                        "This will remove your contacts from the Telegram servers. If 'Sync contacts' is enabled, contacts will be re-synced.".to_string(),
                        "Delete".to_string(),
                    ),
                    GroupConfirmAction::DeleteForumTopic { forum_topic_id } => {
                        let name = this
                            .session()
                            .and_then(|s| {
                                s.forum_topics.get(&dialog_state.chat_id.0).and_then(|topics| {
                                    topics
                                        .iter()
                                        .find(|t| t.forum_topic_id == forum_topic_id)
                                        .map(|t| t.name.clone())
                                })
                            })
                            .unwrap_or_else(|| "this topic".to_string());
                        (
                            "Delete topic".to_string(),
                            format!("Delete {name} and all its messages? This cannot be undone."),
                            "Delete".to_string(),
                        )
                    }
                    GroupConfirmAction::RemoveSavedGif { .. } => (
                        "Remove saved GIF".to_string(),
                        "Remove this GIF from your saved GIFs?".to_string(),
                        "Remove".to_string(),
                    ),
                    GroupConfirmAction::RemoveInstalledStickerSets => (
                        "Remove installed sticker sets".to_string(),
                        format!("Remove all {} installed sticker sets? You can install them again later.",this.session().map(|s|s.stickers.sets.len()).unwrap_or(0)),
                        "Remove all".to_string(),
                    ),
                    GroupConfirmAction::RemoveEmojiSet { .. } => ("Remove emoji pack".to_string(),"Remove this emoji pack? You can install it again later.".to_string(),"Remove".to_string()),
                    GroupConfirmAction::RemoveStickerSet { .. } => (
                        "Remove sticker set".to_string(),
                        "Remove this sticker set from your installed stickers? You can install it again later.".to_string(),
                        "Remove".to_string(),
                    ),
                    GroupConfirmAction::RemoveMember { user_id } => {
                        let name = this.contact_display_name(user_id);
                        let place = if this.group_flavor(dialog_state.chat_id) == Some(quill::moderation::GroupFlavor::Channel) {
                            "channel"
                        } else {
                            "group"
                        };
                        (
                            "Remove member".to_string(),
                            format!("Remove {name} from the {place}?"),
                            "Remove".to_string(),
                        )
                    }
                    GroupConfirmAction::ClearPaymentInfo => (
                        "Clear saved payment info".to_string(),
                        "Delete the shipping info and payment credentials Telegram saved from past checkouts? This cannot be undone.".to_string(),
                        "Clear".to_string(),
                    ),
                };
            let destructive = matches!(
                dialog_state.action,
                GroupConfirmAction::DeleteContact { .. }
                    | GroupConfirmAction::BlockUser { block: true, .. }
                    | GroupConfirmAction::BlockContact { block: true, .. }
                    | GroupConfirmAction::DeleteSyncedContacts
                    | GroupConfirmAction::ClearPaymentInfo
                    | GroupConfirmAction::RemoveSavedGif { .. }
                    | GroupConfirmAction::DeleteForumTopic { .. }
                    | GroupConfirmAction::RemoveInstalledStickerSets
                    | GroupConfirmAction::RemoveStickerSet { .. }
                    | GroupConfirmAction::RemoveEmojiSet { .. }
                    | GroupConfirmAction::RemoveMember { .. }
            );
            let body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().text_sm().child(message))
                .into_any_element();
            let footer = div().flex().justify_end().gap_2().child(
                Button::new("g1-confirm-cancel")
                    .label("Cancel")
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_group_confirm(cx);
                        this.close_kit_dialog_if_done(DialogKind::GroupConfirm, window, cx);
                    })),
            ).child(
                Button::new("g1-confirm-submit")
                    .label(confirm_label)
                    .when(destructive, |button| button.danger())
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.submit_group_confirm(cx);
                        this.close_kit_dialog_if_done(DialogKind::GroupConfirm, window, cx);
                    })),
            );
            dialog
                .title(crate::ui::shell::dialog_title(title))
                .content(crate::ui::shell::scrollable_dialog_content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body.borrow_mut().take().unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }

    /// Join/leave footer for an open broadcast channel (Phase 2.3). Admins
    /// with posting rights see the composer; the footer keeps the leave
    /// affordance and notes the posting state. Non-admins keep the 2.2
    /// behavior (composer hidden).
    pub(super) fn channel_footer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let open = session.open_chat?;
        let chat = session.chats.get(&open.0)?;
        if !chat.is_channel() {
            return None;
        }
        let status = chat.my_member_status;
        let muted = chat.is_muted();
        let footer = div()
            .p_3()
            .border_t_1()
            .border_color(cx.theme().border)
            .flex()
            .items_center()
            .justify_center()
            .gap_3();
        match status {
            None => Some(
                footer
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Checking channel membership…"),
                    )
                    .into_any_element(),
            ),
            // Not subscribed: one clear action.
            Some(ChannelMemberStatus::Left) => Some(
                footer
                    .child(
                        Button::new("channel-join")
                            .label("Join channel")
                            .primary()
                            .w_full()
                            .max_w(px(360.))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.join_channel(open, cx);
                            })),
                    )
                    .into_any_element(),
            ),
            // Subscribers can't post: the bar mutes / unmutes the channel.
            // Leaving lives in the info panel (with confirmation).
            Some(ChannelMemberStatus::Member) => Some(
                footer
                    .child(
                        Button::new("channel-mute-toggle")
                            .label(if muted { "Unmute" } else { "Mute" })
                            .ghost()
                            .w_full()
                            .max_w(px(360.))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.apply_chat_mute(
                                    open,
                                    if muted {
                                        0
                                    } else {
                                        quill::telegram::envelope::MUTE_FOREVER
                                    },
                                    cx,
                                );
                            })),
                    )
                    .into_any_element(),
            ),
            Some(ChannelMemberStatus::Administrator) => {
                let can_post = chat.channel_admin_can_post();
                let note = if can_post {
                    format!("Posting as {}.", chat.title)
                } else {
                    "You are an admin, but posting is disabled for you.".to_string()
                };
                Some(
                    footer
                        .child(
                            Button::new("channel-leave")
                                .label("Leave channel")
                                .ghost()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.leave_channel(open, cx);
                                })),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(note),
                        )
                        .into_any_element(),
                )
            }
            Some(ChannelMemberStatus::Creator) => Some(
                footer
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("Posting as {}.", chat.title)),
                    )
                    .into_any_element(),
            ),
            Some(ChannelMemberStatus::Banned) => Some(
                footer
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("You are banned from this channel."),
                    )
                    .into_any_element(),
            ),
            Some(ChannelMemberStatus::Restricted) | Some(ChannelMemberStatus::Unknown) => Some(
                footer
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Channel membership is unknown."),
                    )
                    .into_any_element(),
            ),
        }
    }

    /// `joinChat` for the open public channel. Demo sessions flip the status
    /// locally (no live driver).
    pub(super) fn join_channel(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.join_channel(chat_id) {
                Ok(()) => "joining channel…".into(),
                Err(_) => "could not join channel".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some(chat) = session.chats.get_mut(&chat_id.0) {
                chat.set_member_status(ChannelMemberStatus::Member, None);
            }
            self.status_note = "joined channel (demo)".into();
        }
        cx.notify();
    }

    /// `leaveChat` for the open channel. Demo sessions flip the status locally.
    pub(super) fn leave_channel(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.leave_channel(chat_id) {
                Ok(()) => "leaving channel…".into(),
                Err(_) => "could not leave channel".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some(chat) = session.chats.get_mut(&chat_id.0) {
                chat.set_member_status(ChannelMemberStatus::Left, None);
            }
            self.status_note = "left channel (demo)".into();
        }
        cx.notify();
    }
}
