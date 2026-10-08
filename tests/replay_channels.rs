//! Channels and broadcasts replay tests.
//! Split from `tests/replay.rs` — pure code motion.
mod replay_common;
use replay_common::*;

/// Phase 2.2: broadcast channels appear in the chat list with their title
/// (ungated) and no gate-reason preview.
#[test]
fn replay_channel_ungated_in_chat_list() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"Demo channel","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
            r#"{"@type":"updateChatPosition","chat_id":13,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"10","is_pinned":false}}"#,
        ],
    );
    let ids: Vec<i64> = session.ordered_chats().iter().map(|c| c.id.0).collect();
    assert_eq!(ids, vec![13]);
    let chat = session.chats.get(&13).unwrap();
    assert!(chat.supported());
    assert!(chat.is_channel());
    assert!(chat.kind.gate_reason().is_none());
    assert_eq!(chat.title, "Demo channel");
    assert_eq!(chat.sidebar_preview(), "No messages yet");
}

/// Phase 2.2: broadcast posts render with the channel as author and live
/// `interaction_info.view_count` (`updateMessageInteractionInfo` included).
#[test]
fn replay_broadcast_posts_with_view_counts() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"Demo channel","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":201,"chat_id":13,"sender_id":{"@type":"messageSenderChat","chat_id":13},"is_outgoing":false,"is_channel_post":true,"interaction_info":{"@type":"messageInteractionInfo","view_count":12345,"forward_count":7,"reply_info":null,"reactions":null},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"post one","entities":[]}}}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":202,"chat_id":13,"sender_id":{"@type":"messageSenderChat","chat_id":13},"is_outgoing":false,"is_channel_post":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"no views yet","entities":[]}}}}"#,
        ],
    );
    let history = session.histories.get(&13).unwrap();
    assert_eq!(history.messages.len(), 2);
    let first = &history.messages[&201];
    assert!(!first.is_outgoing);
    assert_eq!(
        first.interaction_info.as_ref().map(|info| info.view_count),
        Some(12345)
    );
    let second = &history.messages[&202];
    assert!(second.interaction_info.is_none());
    // Live bump via `updateMessageInteractionInfo`.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateMessageInteractionInfo","chat_id":13,"message_id":201,"interaction_info":{"@type":"messageInteractionInfo","view_count":12402,"forward_count":7,"reply_info":null,"reactions":null}}"#,
        ],
    );
    let first = &session.histories.get(&13).unwrap().messages[&201];
    assert_eq!(
        first.interaction_info.as_ref().map(|info| info.view_count),
        Some(12402)
    );
}

/// Phase 2.2/2.3: own membership flows through `getMe` / `getChatMember` /
/// `joinChat` / `updateChatMember` / `leaveChat`; the composer stays hidden
/// for non-admins and appears for admins (2.3).
#[test]
fn replay_channel_membership_and_join_leave() {
    use quill::telegram::envelope::ChannelMemberStatus;

    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    let chat_id = quill::ids::ChatId(13);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"Demo channel","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
        ],
    );
    let chat = session.chats.get(&13).unwrap();
    // Composer hidden in channels (admin posting is 2.3).
    assert!(!chat.can_post());
    assert_eq!(chat.my_member_status, None);

    let me_extra = session.request(RequestPurpose::GetMe, None);
    let member_extra = session.request(RequestPurpose::GetChatMember, Some(chat_id));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            &format!(r#"{{"@type":"user","@extra":"{}","id":777}}"#, me_extra.0),
            &format!(
                r#"{{"@type":"chatMember","@extra":"{}","member_id":{{"@type":"messageSenderUser","user_id":777}},"status":{{"@type":"chatMemberStatusLeft"}}}}"#,
                member_extra.0
            ),
        ],
    );
    assert_eq!(session.my_user_id, Some(777));
    let chat = session.chats.get(&13).unwrap();
    assert_eq!(chat.my_member_status, Some(ChannelMemberStatus::Left));

    // joinChat → success flips to Member optimistically.
    let join_extra = session.request(RequestPurpose::JoinChat, Some(chat_id));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"chatJoinResultSuccess","@extra":"{}","chat_id":13}}"#,
            join_extra.0
        )],
    );
    assert_eq!(
        session.chats.get(&13).unwrap().my_member_status,
        Some(ChannelMemberStatus::Member)
    );

    // updateChatMember confirms the admin promotion; in 2.3 a bare admin
    // (no rights block → no explicit restriction) gets the composer.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateChatMember","chat_id":13,"actor_user_id":1,"date":1,"invite_link":null,"via_join_request":false,"via_chat_folder_invite_link":false,"old_chat_member":{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":777},"status":{"@type":"chatMemberStatusMember"}},"new_chat_member":{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":777},"status":{"@type":"chatMemberStatusAdministrator"}}}"#,
        ],
    );
    let chat = session.chats.get(&13).unwrap();
    assert_eq!(
        chat.my_member_status,
        Some(ChannelMemberStatus::Administrator)
    );
    assert!(chat.my_member_status.unwrap().is_admin());
    assert!(chat.can_post());

    // leaveChat → ok flips to Left optimistically.
    let leave_extra = session.request(RequestPurpose::LeaveChat, Some(chat_id));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(r#"{{"@type":"ok","@extra":"{}"}}"#, leave_extra.0)],
    );
    assert_eq!(
        session.chats.get(&13).unwrap().my_member_status,
        Some(ChannelMemberStatus::Left)
    );
}

/// Phase 2.2: non-success `joinChat` results keep the old status.
#[test]
fn replay_join_chat_non_success_keeps_status() {
    use quill::telegram::envelope::ChannelMemberStatus;

    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    let chat_id = quill::ids::ChatId(13);
    session.my_user_id = Some(777);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"Demo channel","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
        ],
    );
    let chat = session.chats.get_mut(&13).unwrap();
    chat.set_member_status(ChannelMemberStatus::Left, None);
    for ctor in [
        "chatJoinResultRequestSent",
        "chatJoinResultGuardBotApprovalRequired",
        "chatJoinResultDeclined",
    ] {
        let extra = session.request(RequestPurpose::JoinChat, Some(chat_id));
        apply_all_seq(
            &mut session,
            &sink,
            &seq,
            &[&format!(r#"{{"@type":"{}","@extra":"{}"}}"#, ctor, extra.0)],
        );
        assert_eq!(
            session.chats.get(&13).unwrap().my_member_status,
            Some(ChannelMemberStatus::Left),
            "{ctor} must not flip status"
        );
    }
    // A foreign member update is ignored.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateChatMember","chat_id":13,"actor_user_id":1,"date":1,"invite_link":null,"via_join_request":false,"via_chat_folder_invite_link":false,"old_chat_member":{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":999},"status":{"@type":"chatMemberStatusMember"}},"new_chat_member":{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":999},"status":{"@type":"chatMemberStatusBanned"}}}"#,
        ],
    );
    assert_eq!(
        session.chats.get(&13).unwrap().my_member_status,
        Some(ChannelMemberStatus::Left)
    );
}

/// Phase 2.3: an administrator with `rights.can_post_messages: true` gets the
/// composer (`ChatSummary::can_post()` true — the exact predicate the
/// composer gate in `src/ui/mod.rs` reads).
#[test]
fn replay_channel_admin_sees_composer() {
    use quill::telegram::envelope::ChannelMemberStatus;

    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    let chat_id = quill::ids::ChatId(13);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"Demo channel","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
        ],
    );
    // No membership yet: composer stays hidden.
    assert!(!session.chats.get(&13).unwrap().can_post());

    let me_extra = session.request(RequestPurpose::GetMe, None);
    let member_extra = session.request(RequestPurpose::GetChatMember, Some(chat_id));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            &format!(r#"{{"@type":"user","@extra":"{}","id":777}}"#, me_extra.0),
            &format!(
                r#"{{"@type":"chatMember","@extra":"{}","member_id":{{"@type":"messageSenderUser","user_id":777}},"status":{{"@type":"chatMemberStatusAdministrator","can_be_edited":true,"rights":{{"@type":"chatAdministratorRights","can_post_messages":true}}}}}}"#,
                member_extra.0
            ),
        ],
    );
    let chat = session.chats.get(&13).unwrap();
    assert_eq!(
        chat.my_member_status,
        Some(ChannelMemberStatus::Administrator)
    );
    assert_eq!(chat.my_admin_can_post_messages, Some(true));
    assert!(chat.can_post());

    // A channel post sent through the normal pipeline still renders.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateNewMessage","message":{"id":301,"chat_id":13,"sender_id":{"@type":"messageSenderChat","chat_id":13},"is_outgoing":false,"is_channel_post":true,"interaction_info":{"@type":"messageInteractionInfo","view_count":5,"forward_count":0,"reply_info":null,"reactions":null},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"admin post echo","entities":[]}}}}"#,
        ],
    );
    assert!(
        session
            .histories
            .get(&13)
            .unwrap()
            .messages
            .contains_key(&301)
    );
}

/// Phase 2.3: non-admins keep the hidden composer; so does an administrator
/// whose `rights.can_post_messages` is explicitly false.
#[test]
fn replay_channel_non_admin_composer_hidden() {
    use quill::telegram::envelope::ChannelMemberStatus;

    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    let chat_id = quill::ids::ChatId(13);
    session.my_user_id = Some(777);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"Demo channel","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
        ],
    );

    // Plain member: composer hidden.
    let member_extra = session.request(RequestPurpose::GetChatMember, Some(chat_id));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"chatMember","@extra":"{}","member_id":{{"@type":"messageSenderUser","user_id":777}},"status":{{"@type":"chatMemberStatusMember"}}}}"#,
            member_extra.0
        )],
    );
    let chat = session.chats.get(&13).unwrap();
    assert_eq!(chat.my_member_status, Some(ChannelMemberStatus::Member));
    assert!(!chat.can_post());

    // Administrator with the posting right revoked: composer hidden too.
    let member_extra = session.request(RequestPurpose::GetChatMember, Some(chat_id));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"chatMember","@extra":"{}","member_id":{{"@type":"messageSenderUser","user_id":777}},"status":{{"@type":"chatMemberStatusAdministrator","can_be_edited":true,"rights":{{"@type":"chatAdministratorRights","can_post_messages":false}}}}}}"#,
            member_extra.0
        )],
    );
    let chat = session.chats.get(&13).unwrap();
    assert_eq!(
        chat.my_member_status,
        Some(ChannelMemberStatus::Administrator)
    );
    assert_eq!(chat.my_admin_can_post_messages, Some(false));
    assert!(!chat.can_post());
}

/// Phase 2.3: `updateChatMember` flips the composer gate both ways —
/// member → admin shows it, admin → left hides it, creator shows it.
#[test]
fn replay_channel_admin_status_change_flips_composer() {
    use quill::telegram::envelope::ChannelMemberStatus;

    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    session.my_user_id = Some(777);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"Demo channel","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
        ],
    );
    let chat = session.chats.get_mut(&13).unwrap();
    chat.set_member_status(ChannelMemberStatus::Member, None);
    assert!(!session.chats.get(&13).unwrap().can_post());

    let promote = |status_json: &str| {
        format!(
            r#"{{"@type":"updateChatMember","chat_id":13,"actor_user_id":1,"date":1,"invite_link":null,"via_join_request":false,"via_chat_folder_invite_link":false,"old_chat_member":{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":777}},"status":{{"@type":"chatMemberStatusMember"}}}},"new_chat_member":{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":777}},"status":{}}}}}"#,
            status_json
        )
    };
    let admin_rights = |can_post: bool| {
        format!(
            r#"{{"@type":"chatMemberStatusAdministrator","can_be_edited":true,"rights":{{"@type":"chatAdministratorRights","can_post_messages":{can_post}}}}}"#
        )
    };

    // Promoted to admin with the posting right: composer appears.
    apply_all_seq(&mut session, &sink, &seq, &[&promote(&admin_rights(true))]);
    let chat = session.chats.get(&13).unwrap();
    assert_eq!(
        chat.my_member_status,
        Some(ChannelMemberStatus::Administrator)
    );
    assert!(chat.can_post());

    // Right revoked server-side: composer hides again.
    apply_all_seq(&mut session, &sink, &seq, &[&promote(&admin_rights(false))]);
    let chat = session.chats.get(&13).unwrap();
    assert_eq!(chat.my_admin_can_post_messages, Some(false));
    assert!(!chat.can_post());

    // Left the channel: still hidden.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&promote(r#"{"@type":"chatMemberStatusLeft"}"#)],
    );
    assert_eq!(
        session.chats.get(&13).unwrap().my_member_status,
        Some(ChannelMemberStatus::Left)
    );
    assert!(!session.chats.get(&13).unwrap().can_post());

    // Became the creator: composer appears.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&promote(r#"{"@type":"chatMemberStatusCreator"}"#)],
    );
    let chat = session.chats.get(&13).unwrap();
    assert_eq!(chat.my_member_status, Some(ChannelMemberStatus::Creator));
    assert!(chat.can_post());
}

/// A public channel opened from search / a `t.me` link is not in the chat
/// list and the viewer never joined it. TDLib sends `updateSupergroup` with
/// `chatMemberStatusLeft` before `updateNewChat`; `getChatMember(me)` fails
/// with "Member not found". The bar must resolve to Join from the supergroup
/// status instead of waiting forever.
#[test]
fn replay_non_member_channel_resolves_left_without_get_chat_member() {
    use quill::telegram::envelope::ChannelMemberStatus;

    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    let chat_id = quill::ids::ChatId(-1001006503122);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":1006503122,"usernames":{"@type":"usernames","active_usernames":["durov"],"disabled_usernames":[],"editable_username":"durov"},"status":{"@type":"chatMemberStatusLeft"},"member_count":0,"is_channel":true,"is_broadcast_group":false}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":-1001006503122,"title":"Durov's Channel","type":{"@type":"chatTypeSupergroup","supergroup_id":1006503122,"is_channel":true},"unread_count":0}}"#,
        ],
    );
    let chat = session.chats.get(&chat_id.0).unwrap();
    assert_eq!(chat.my_member_status, Some(ChannelMemberStatus::Left));
    assert!(!chat.can_post());

    // A failing getChatMember must not undo the resolved state.
    let me_extra = session.request(RequestPurpose::GetMe, None);
    let member_extra = session.request(RequestPurpose::GetChatMember, Some(chat_id));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            &format!(r#"{{"@type":"user","@extra":"{}","id":777}}"#, me_extra.0),
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"Member not found"}}"#,
                member_extra.0
            ),
        ],
    );
    assert_eq!(
        session.chats.get(&chat_id.0).unwrap().my_member_status,
        Some(ChannelMemberStatus::Left)
    );

    // Joined later: `updateSupergroup` flips Left -> Member.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":1006503122,"status":{"@type":"chatMemberStatusMember"},"is_channel":true}}"#,
        ],
    );
    assert_eq!(
        session.chats.get(&chat_id.0).unwrap().my_member_status,
        Some(ChannelMemberStatus::Member)
    );
}

/// The chat can arrive first and `getChatMember` can fail afterwards: the
/// `getSupergroup` answer still resolves the bar.
#[test]
fn replay_non_member_channel_get_supergroup_resolves_left() {
    use quill::telegram::envelope::ChannelMemberStatus;

    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    let chat_id = quill::ids::ChatId(-1001006503122);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateNewChat","chat":{"id":-1001006503122,"title":"Durov's Channel","type":{"@type":"chatTypeSupergroup","supergroup_id":1006503122,"is_channel":true},"unread_count":0}}"#,
        ],
    );
    assert_eq!(
        session.chats.get(&chat_id.0).unwrap().my_member_status,
        None
    );
    let sg_extra = session.request(RequestPurpose::GetSupergroup, Some(chat_id));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"supergroup","@extra":"{}","id":1006503122,"status":{{"@type":"chatMemberStatusLeft"}},"is_channel":true}}"#,
            sg_extra.0
        )],
    );
    assert_eq!(
        session.chats.get(&chat_id.0).unwrap().my_member_status,
        Some(ChannelMemberStatus::Left)
    );
}
