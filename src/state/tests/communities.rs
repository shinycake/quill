//! State reducer tests: the TDLib 1.8.70 community answers.
use super::common::*;
use super::*;
use crate::state::{CommunitiesPurpose, CommunityOutcome};

#[test]
fn limits_come_from_the_options() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateOption","name":"community_chat_count_max","value":{"@type":"optionValueInteger","value":"20"}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateOption","name":"community_bot_count_max","value":{"@type":"optionValueInteger","value":"5"}}"#,
    );
    assert_eq!(session.communities.chat_count_max, 20);
    assert_eq!(session.communities.bot_count_max, 5);
}

#[test]
fn answers_nobody_asked_for_change_nothing() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"communities","community_ids":[1,2]}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"communityAddChatRequests","total_count":1,"requests":[{"@type":"communityAddChatRequest","chat_id":7,"is_hidden":false,"sender_user_id":5,"date":1}],"next_offset":""}"#,
    );
    assert!(session.communities.joined.is_idle());
    assert!(session.communities.add_chat_requests.is_empty());
    assert!(session.communities.take_outcome().is_none());
}

#[test]
fn a_first_page_replaces_the_list() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    for (chat_id, offset) in [(7, "p2"), (8, "")] {
        let extra = session.request_for_community(
            RequestPurpose::Communities(CommunitiesPurpose::GetAddChatRequests {
                first_page: true,
            }),
            42,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"communityAddChatRequests","@extra":"{}","total_count":1,"requests":[{{"@type":"communityAddChatRequest","chat_id":{chat_id},"is_hidden":false,"sender_user_id":5,"date":1}}],"next_offset":"{offset}"}}"#,
                extra.0
            ),
        );
    }
    let list = session.communities.add_chat_requests(42).data().cloned();
    let list = list.expect("loaded");
    assert_eq!(
        list.requests.iter().map(|r| r.chat_id).collect::<Vec<_>>(),
        vec![8]
    );
    assert_eq!(list.next_offset, None);
}

#[test]
fn a_ban_that_unbans_reports_nothing() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request_for_community(
        RequestPurpose::Communities(CommunitiesPurpose::ToggleMemberBanned {
            member: crate::state::CommunityMember::User(5),
            is_banned: false,
        }),
        42,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert_eq!(session.communities.take_outcome(), None::<CommunityOutcome>);
}

fn seed_collapsed(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    for json in [
        r#"{"@type":"updateCommunity","community":{"@type":"community","id":42,"have_access":true,"name":"Rustaceans","date":1,"status":{"@type":"communityMemberStatusMember"},"permissions":{"@type":"communityPermissions","can_edit_chat_list":false},"are_chats_collapsed":true}}"#,
        r#"{"@type":"updateCommunityFullInfo","community_id":42,"community_full_info":{"@type":"communityFullInfo","chats":[{"@type":"communityChat","chat_id":7,"can_view_history":true,"is_hidden":false},{"@type":"communityChat","chat_id":8,"can_view_history":true,"is_hidden":false},{"@type":"communityChat","chat_id":9,"can_view_history":true,"is_hidden":false}],"administrator_count":0,"banned_count":0,"add_chat_request_count":0,"notification_settings":{"@type":"chatNotificationSettings","use_default_mute_for":false,"mute_for":2147483647}}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Rust News","type":{"@type":"chatTypeSupergroup","supergroup_id":7,"is_channel":true},"unread_count":3,"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"70","is_pinned":false}],"notification_settings":{"@type":"chatNotificationSettings","use_default_mute_for":true,"mute_for":0}}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":8,"title":"Rust Help","type":{"@type":"chatTypeSupergroup","supergroup_id":8,"is_channel":false},"unread_count":0,"positions":[{"@type":"chatPosition","list":{"@type":"chatListArchive"},"order":"80","is_pinned":false}]}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":9,"title":"Not joined","type":{"@type":"chatTypeSupergroup","supergroup_id":9,"is_channel":false},"unread_count":0,"positions":[]}}"#,
    ] {
        apply_json(session, seq, sink, json);
    }
}

#[test]
fn a_collapsed_community_gets_one_row_for_its_joined_chats() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    seed_collapsed(&mut session, &sink, &seq);
    let rows = session.collapsed_community_rows();
    assert_eq!(rows.len(), 1);
    let row = rows.first().expect("one row");
    assert_eq!(row.name, "Rustaceans");
    let mut members = row.member_chat_ids.clone();
    members.sort_unstable();
    assert_eq!(members, vec![7]);
    assert_eq!(row.not_joined, 2);
    assert_eq!(row.unread_chats, 1);
    assert!(row.badge_muted);
    let chat = session.chats.get(&7).cloned().expect("chat 7");
    assert!(session.effective_muted(&chat));

    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateCommunity","community":{"@type":"community","id":42,"have_access":true,"name":"Rustaceans","date":1,"status":{"@type":"communityMemberStatusMember"},"permissions":{"@type":"communityPermissions","can_edit_chat_list":false},"are_chats_collapsed":false}}"#,
    );
    assert!(session.collapsed_community_rows().is_empty());
}
