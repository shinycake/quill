use super::*;
use crate::ids::ChatId;

#[test]
fn chat_member_status_constructors_parse() {
    for (ctor, expected) in [
        ("chatMemberStatusCreator", ChannelMemberStatus::Creator),
        (
            "chatMemberStatusAdministrator",
            ChannelMemberStatus::Administrator,
        ),
        ("chatMemberStatusMember", ChannelMemberStatus::Member),
        (
            "chatMemberStatusRestricted",
            ChannelMemberStatus::Restricted,
        ),
        ("chatMemberStatusLeft", ChannelMemberStatus::Left),
        ("chatMemberStatusBanned", ChannelMemberStatus::Banned),
    ] {
        let json = format!(
            r#"{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":777}},"status":{{"@type":"{ctor}"}}}}"#,
        );
        let env = parse_envelope(&json).unwrap();
        match env.payload {
            EnvelopePayload::ChatMember { member } => {
                assert_eq!(member.member_id, MessageSender::User { user_id: 777 });
                assert_eq!(member.status, expected);
                // Bare status constructors carry no rights block.
                assert_eq!(member.admin_can_post_messages, None);
            }
            other => panic!("{other:?}"),
        }
    }
    assert!(ChannelMemberStatus::Creator.is_admin());
    assert!(ChannelMemberStatus::Administrator.is_admin());
    assert!(!ChannelMemberStatus::Member.is_admin());
    assert!(ChannelMemberStatus::Member.is_joined());
    assert!(!ChannelMemberStatus::Left.is_joined());
}

#[test]
fn chat_member_administrator_rights_can_post_messages() {
    // `rights.can_post_messages` rides on `chatMemberStatusAdministrator`
    // (schema 1.8.67: `chatMemberStatusAdministrator can_be_edited:Bool
    // rights:chatAdministratorRights`), not on the status itself.
    for (can_post, expected) in [(true, Some(true)), (false, Some(false))] {
        let json = format!(
            r#"{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":777}},"status":{{"@type":"chatMemberStatusAdministrator","can_be_edited":true,"rights":{{"@type":"chatAdministratorRights","can_post_messages":{can_post}}}}}}}"#,
        );
        let env = parse_envelope(&json).unwrap();
        match env.payload {
            EnvelopePayload::ChatMember { member } => {
                assert_eq!(member.status, ChannelMemberStatus::Administrator);
                assert_eq!(member.admin_can_post_messages, expected);
            }
            other => panic!("{other:?}"),
        }
    }
    // Missing rights block: no posting-right claim either way.
    let json = r#"{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":777},"status":{"@type":"chatMemberStatusAdministrator"}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::ChatMember { member } => {
            assert_eq!(member.status, ChannelMemberStatus::Administrator);
            assert_eq!(member.admin_can_post_messages, None);
        }
        other => panic!("{other:?}"),
    }
    // Non-admin statuses never carry the right, even with a rights block.
    let json = r#"{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":777},"status":{"@type":"chatMemberStatusMember","rights":{"@type":"chatAdministratorRights","can_post_messages":true}}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::ChatMember { member } => {
            assert_eq!(member.status, ChannelMemberStatus::Member);
            assert_eq!(member.admin_can_post_messages, None);
        }
        other => panic!("{other:?}"),
    }
}

/// Phase D3a: `rights.can_invite_users` rides on
/// `chatMemberStatusAdministrator` (schema 1.8.67, line 1092),
/// mirroring the `can_post_messages` pattern above.
#[test]
fn chat_member_administrator_rights_can_invite_users() {
    for (can_invite, expected) in [(true, Some(true)), (false, Some(false))] {
        let json = format!(
            r#"{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":777}},"status":{{"@type":"chatMemberStatusAdministrator","can_be_edited":true,"rights":{{"@type":"chatAdministratorRights","can_invite_users":{can_invite}}}}}}}"#,
        );
        let env = parse_envelope(&json).unwrap();
        match env.payload {
            EnvelopePayload::ChatMember { member } => {
                assert_eq!(member.status, ChannelMemberStatus::Administrator);
                assert_eq!(member.admin_can_invite_users, expected);
            }
            other => panic!("{other:?}"),
        }
    }
    // Missing rights block: no invite-right claim either way.
    let json = r#"{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":777},"status":{"@type":"chatMemberStatusAdministrator"}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::ChatMember { member } => {
            assert_eq!(member.status, ChannelMemberStatus::Administrator);
            assert_eq!(member.admin_can_invite_users, None);
        }
        other => panic!("{other:?}"),
    }
    // Non-admin statuses never carry the right, even with a rights block.
    let json = r#"{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":777},"status":{"@type":"chatMemberStatusMember","rights":{"@type":"chatAdministratorRights","can_invite_users":true}}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::ChatMember { member } => {
            assert_eq!(member.status, ChannelMemberStatus::Member);
            assert_eq!(member.admin_can_invite_users, None);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn update_chat_member_keeps_new_member() {
    let json = r#"{"@type":"updateChatMember","chat_id":13,"actor_user_id":1,"date":1,"invite_link":null,"via_join_request":false,"via_chat_folder_invite_link":false,"old_chat_member":{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":777},"status":{"@type":"chatMemberStatusLeft"}},"new_chat_member":{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":777},"status":{"@type":"chatMemberStatusMember"}}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateChatMember { chat_id, member } => {
            assert_eq!(chat_id, ChatId(13));
            assert_eq!(member.status, ChannelMemberStatus::Member);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn join_chat_results_parse_without_invented_variants() {
    let success = parse_envelope(r#"{"@type":"chatJoinResultSuccess","chat_id":13}"#).unwrap();
    match success.payload {
        EnvelopePayload::JoinChatResult(ChatJoinResult::Success { chat_id }) => {
            assert_eq!(chat_id, ChatId(13));
        }
        other => panic!("{other:?}"),
    }
    for (ctor, expected) in [
        ("chatJoinResultRequestSent", ChatJoinResult::RequestSent),
        (
            "chatJoinResultGuardBotApprovalRequired",
            ChatJoinResult::GuardBotApprovalRequired,
        ),
        ("chatJoinResultDeclined", ChatJoinResult::Declined),
    ] {
        let json = format!(r#"{{"@type":"{ctor}"}}"#);
        let env = parse_envelope(&json).unwrap();
        match env.payload {
            EnvelopePayload::JoinChatResult(result) => assert_eq!(result, expected),
            other => panic!("{other:?}"),
        }
    }
}

#[test]
fn me_response_keeps_id_only() {
    let env = parse_envelope(r#"{"@type":"user","id":777,"is_bot":false}"#).unwrap();
    match env.payload {
        EnvelopePayload::Me { user_id } => assert_eq!(user_id, 777),
        other => panic!("{other:?}"),
    }
}
