//! group/channel management: create dialogs, info edit, admin helpers, group confirm.

use super::app::QuillApp;
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
use quill::state::GroupsPurpose;
use quill::state::{MemberListFilter, RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{ChannelMemberStatus, ChatAdminRights, ChatKind, ChatPermissions};
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
        RequestPurpose::Groups(GroupsPurpose::GetSupergroupMembers {
            filter: MemberListFilter::Recent,
        }),
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
                    self.connection.status_note = "could not refresh invite links".into();
                }
            }
        } else {
            self.connection.status_note = "invite links need a live connection (demo)".into();
        }
        cx.notify();
    }

    /// Phase D3a: copy an invite-link URL to the clipboard.
    pub(super) fn copy_invite_link(&mut self, invite_link: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(invite_link.to_string()));
        self.connection.status_note = "Invite link copied".into();
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
                    self.connection.status_note = "could not revoke invite link".into();
                }
            }
        } else {
            self.connection.status_note = "invite links need a live connection (demo)".into();
        }
        cx.notify();
    }

    /// Phase D3a: re-request the join-request list.
    pub(super) fn refresh_join_requests(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            match live.driver.refresh_chat_join_requests(chat_id) {
                Ok(_) => {}
                Err(_) => {
                    self.connection.status_note = "could not refresh join requests".into();
                }
            }
        } else {
            self.connection.status_note = "join requests need a live connection (demo)".into();
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
                    self.connection.status_note = "could not process join request".into();
                }
            }
        } else {
            self.connection.status_note = "join requests need a live connection (demo)".into();
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
                    self.connection.status_note = "could not refresh administrators".into();
                }
            }
        } else {
            self.connection.status_note = "administrators need a live connection (demo)".into();
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
                Ok(_) => self.connection.status_note = "signatures updated".into(),
                Err(_) => self.connection.status_note = "could not change signatures".into(),
            }
        } else {
            self.connection.status_note = "signatures need a live connection (demo)".into();
        }
        cx.notify();
    }

    /// Flip the channel's "Auto-translate messages" switch.
    pub(super) fn set_auto_translate(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let enabled = !self
            .session()
            .is_some_and(|session| session.chat_auto_translate(chat_id));
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.toggle_auto_translate(chat_id, enabled)
            {
                Ok(()) => "auto-translate updated".into(),
                Err(_) => "could not change auto-translate".into(),
            };
        } else {
            self.connection.status_note = "auto-translate needs a live connection (demo)".into();
        }
        cx.notify();
    }

    /// Slice G2: aggressive anti-spam toggle (info panel → Manage
    /// group). Gated on `supergroupFullInfo.can_toggle_aggressive_anti_spam`.
    pub(super) fn set_anti_spam(&mut self, chat_id: ChatId, enabled: bool, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            match live.driver.toggle_aggressive_anti_spam(chat_id, enabled) {
                Ok(_) => self.connection.status_note = "anti-spam updated".into(),
                Err(_) => self.connection.status_note = "could not change anti-spam".into(),
            }
        } else {
            self.connection.status_note = "anti-spam needs a live connection (demo)".into();
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
            self.session().is_some_and(|session| {
                session.groups.supergroup_join_by_request.get(&id) == Some(&true)
            })
        })
    }

    /// Slice G1: cached broadcast-group flag for a supergroup chat.
    pub(super) fn chat_is_broadcast(&self, chat_id: ChatId) -> bool {
        self.chat_supergroup_id(chat_id).is_some_and(|id| {
            self.session().is_some_and(|session| {
                session.groups.supergroup_is_broadcast.get(&id) == Some(&true)
            })
        })
    }

    pub(super) fn open_create_chat_dialog(
        &mut self,
        kind: CreateChatKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.admin.create_chat_dialog = Some(CreateChatDialog::new(window, cx, kind));
        cx.notify();
    }

    pub(super) fn close_create_chat_dialog(&mut self, cx: &mut Context<Self>) {
        self.admin.create_chat_dialog = None;
        cx.notify();
    }

    pub(super) fn toggle_create_chat_user(&mut self, user_id: i64, cx: &mut Context<Self>) {
        if let Some(dialog) = self.admin.create_chat_dialog.as_mut() {
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
        let Some(dialog) = self.admin.create_chat_dialog.take() else {
            return;
        };
        let title = dialog.title_input.read(cx).value().trim().to_string();
        let description = dialog.description_input.read(cx).value().trim().to_string();
        let kind = dialog.kind;
        let user_ids = dialog.selected_users.clone();
        if title.is_empty() {
            self.admin.create_chat_dialog = Some(dialog);
            self.connection.status_note = "Name cannot be empty".into();
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
                        self.admin.create_chat_dialog = Some(dialog);
                        format!("could not create {}", kind.title().to_lowercase())
                    }
                }
            }
            None => {
                self.admin.create_chat_dialog = Some(dialog);
                "creating chats needs a live connection (demo)".to_string()
            }
        };
        self.connection.status_note = note;
        cx.notify();
    }

    /// Slice G10: open the "New community" dialog (side-menu entry).
    pub(super) fn open_create_community_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.admin.community.create_dialog = Some(CreateCommunityDialog::new(window, cx));
        cx.notify();
    }

    pub(super) fn close_create_community_dialog(&mut self, cx: &mut Context<Self>) {
        self.admin.community.create_dialog = None;
        cx.notify();
    }

    /// Slice G10: single-select base-chat picker for the create dialog.
    pub(super) fn toggle_create_community_chat(&mut self, chat_id: i64, cx: &mut Context<Self>) {
        if let Some(dialog) = self.admin.community.create_dialog.as_mut() {
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
        let Some(dialog) = self.admin.community.create_dialog.take() else {
            return;
        };
        let name = dialog.name_input.read(cx).value().trim().to_string();
        let hide_chat = dialog.hide_chat;
        if name.is_empty() {
            self.admin.community.create_dialog = Some(dialog);
            self.connection.status_note = "Name cannot be empty".into();
            cx.notify();
            return;
        }
        let Some(chat_id) = dialog.chat_id else {
            self.admin.community.create_dialog = Some(dialog);
            self.connection.status_note = "Pick a chat for the community".into();
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
                        self.admin.community.create_dialog = Some(dialog);
                        "could not create community".to_string()
                    }
                }
            }
            None => {
                self.admin.community.create_dialog = Some(dialog);
                "creating communities needs a live connection (demo)".to_string()
            }
        };
        self.connection.status_note = note;
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
            .and_then(|s| s.groups.communities.get(&community_id))
            .map(|community| community.name.clone())
            .unwrap_or_default();
        // `chat_id` is unused for `CommunityName`; the community id
        // rides the prompt kind.
        self.admin.username_dialog = Some(UsernameDialog::new(
            window,
            cx,
            ChatId(community_id),
            TextPromptKind::CommunityName { community_id },
            &current,
            "Community name",
        ));
        cx.notify();
    }

    /// TDLib 1.8.68 `setCommunityPermissions`: the community info
    /// panel's "Members can edit the chat list" switch. Not optimistic —
    /// the switch follows `updateCommunity`; refusals surface through
    /// `Session::community_error`.
    pub(super) fn set_community_members_can_edit_chat_list(
        &mut self,
        community_id: i64,
        allowed: bool,
        cx: &mut Context<Self>,
    ) {
        self.connection.status_note = match self.live.as_mut() {
            Some(live) => match live.driver.set_community_permissions(community_id, allowed) {
                Ok(Some(_)) => "updating community permissions…".into(),
                Ok(None) => "you can't change this community's permissions".into(),
                Err(_) => "could not update community permissions".into(),
            },
            None => "community permissions need a live connection (demo)".into(),
        };
        cx.notify();
    }

    pub(super) fn open_username_dialog(
        &mut self,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = self.chat_username(chat_id);
        self.admin.username_dialog = Some(UsernameDialog::new(
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
        self.admin.username_dialog = Some(UsernameDialog::new(
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
                        .groups
                        .supergroup_full_infos
                        .get(&supergroup_id)
                        .map(|info| info.description.clone()),
                    _ => None,
                })
            })
            .unwrap_or_default();
        self.admin.username_dialog = Some(UsernameDialog::new(
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
        self.admin.username_dialog = Some(UsernameDialog::new(
            window,
            cx,
            chat_id,
            TextPromptKind::GroupPhoto,
            "",
            "Photo file path (empty = remove current photo)",
        ));
        cx.notify();
    }
}

crate::ui::shell::register_dialogs! {
    CreateChat => DialogSpec::new(
        4200,
        |app| app.admin.create_chat_dialog.is_some(),
        QuillApp::build_create_chat_dialog,
    ),

    Username => DialogSpec::new(
        4500,
        |app| app.admin.username_dialog.is_some(),
        QuillApp::build_username_dialog,
    ),

    GroupConfirm => DialogSpec::new(
        4800,
        |app| app.admin.group_confirm_dialog.is_some(),
        QuillApp::build_group_confirm_dialog,
    ),
}

mod build_group_confirm_dialog;
mod open_custom_title_dialog;
