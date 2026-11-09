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
    assert_eq!(session.communities.chat_count_max, Some(20));
    assert_eq!(session.communities.bot_count_max, Some(5));
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
