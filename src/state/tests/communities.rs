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

/// A community whose `photo.small` is file 41, downloaded (`done`) or not.
fn community_with_photo(done: bool) -> String {
    format!(
        r#"{{"@type":"updateCommunity","community":{{"@type":"community","id":42,"have_access":true,"name":"Rustaceans","photo":{{"@type":"chatPhotoInfo","small":{{"@type":"file","id":41,"size":24,"expected_size":24,"local":{{"@type":"localFile","path":"{path}","can_be_downloaded":true,"is_downloading_active":false,"is_downloading_completed":{done},"downloaded_size":24}},"remote":{{"@type":"remoteFile","id":"r","is_uploading_active":false}}}},"big":null,"has_animation":false,"is_personal":false}},"date":1,"status":{{"@type":"communityMemberStatusMember"}},"permissions":{{"@type":"communityPermissions","can_edit_chat_list":false}},"are_chats_collapsed":true}}}}"#,
        path = if done { "/cache/41.jpg" } else { "" },
    )
}

#[test]
fn a_downloaded_community_photo_shows() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, &community_with_photo(true));
    assert_eq!(session.community_photo_path(42), Some("/cache/41.jpg"));
    assert!(session.take_avatar_claims().is_empty());
}

#[test]
fn a_missing_community_photo_is_claimed_for_download() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, &community_with_photo(false));
    assert_eq!(session.community_photo_path(42), None);
    assert_eq!(session.take_avatar_claims(), vec![crate::ids::FileId(41)]);
}

#[test]
fn owned_bots_come_from_our_own_ask() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"users","total_count":1,"user_ids":[5]}"#,
    );
    assert!(session.communities.owned_bots.is_idle());
    let purpose = RequestPurpose::Communities(CommunitiesPurpose::GetOwnedBots);
    let extra = session.request(purpose, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"users","total_count":2,"user_ids":[5,6],"@extra":"{}"}}"#,
            extra.0
        ),
    );
    assert_eq!(session.communities.owned_bots.data(), Some(&vec![5, 6]));
    let extra = session.request(purpose, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","code":400,"message":"BOTS_FAILED","@extra":"{}"}}"#,
            extra.0
        ),
    );
    let fetch = &session.communities.owned_bots;
    assert!(fetch.error().is_some());
    assert_eq!(fetch.data(), Some(&vec![5, 6]));
}

#[test]
fn add_all_counts_the_requests_left_once_single_answers_are_out() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateCommunityFullInfo","community_id":42,"community_full_info":{"@type":"communityFullInfo","chats":[],"administrator_count":1,"banned_count":0,"add_chat_request_count":3}}"#,
    );
    assert_eq!(session.community_add_chat_requests_left(42), 3);
    let extra = session.request_for_community(
        RequestPurpose::Communities(CommunitiesPurpose::ProcessAddChatRequest {
            chat_id: 7,
            approve: true,
        }),
        42,
    );
    assert_eq!(
        session.community_add_chat_requests_left(42),
        2,
        "the answered row is not counted"
    );
    // TDLib's reloaded count comes before the `ok`; then the row is out
    // of flight and counted once.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateCommunityFullInfo","community_id":42,"community_full_info":{"@type":"communityFullInfo","chats":[],"administrator_count":1,"banned_count":0,"add_chat_request_count":2}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert_eq!(session.community_add_chat_requests_left(42), 2);
    assert_eq!(session.community_add_chat_requests_left(404), 0);
}

#[test]
fn show_as_one_chat_is_busy_while_its_toggle_is_in_flight() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    assert!(!session.community_collapse_in_flight(42));
    let extra = session.request_for_community(
        RequestPurpose::Communities(CommunitiesPurpose::ToggleCollapsed { collapsed: true }),
        42,
    );
    assert!(session.community_collapse_in_flight(42));
    assert!(!session.community_collapse_in_flight(43));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert!(!session.community_collapse_in_flight(42));
}

fn ok_for(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
    purpose: CommunitiesPurpose,
) {
    let extra = session.request_for_community(RequestPurpose::Communities(purpose), 42);
    apply_json(
        session,
        seq,
        sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
}

/// TDLib reloads only the community's full info before the `ok` of a
/// Remove or an Add; the chat's cached `supergroupFullInfo.community_id`
/// would keep the old link, and Manage would offer Remove again.
#[test]
fn remove_and_add_ok_update_the_chats_cached_community() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    seed_collapsed(&mut session, &sink, &seq);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateSupergroupFullInfo","supergroup_id":7,"supergroup_full_info":{"@type":"supergroupFullInfo","description":"","community_id":42}}"#,
    );
    assert_eq!(session.chat_community_id(7), Some(42));

    ok_for(
        &mut session,
        &sink,
        &seq,
        CommunitiesPurpose::RemoveChat { chat_id: 7 },
    );
    assert_eq!(session.chat_community_id(7), None, "removed");

    ok_for(
        &mut session,
        &sink,
        &seq,
        CommunitiesPurpose::AddChat {
            chat_id: 7,
            is_hidden: false,
        },
    );
    assert_eq!(session.chat_community_id(7), Some(42), "added back");
}

/// Someone else deleted the community: its chats no longer map to it, and
/// its mute no longer applies to them.
#[test]
fn a_community_without_access_has_no_chats() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    seed_collapsed(&mut session, &sink, &seq);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateSupergroupFullInfo","supergroup_id":8,"supergroup_full_info":{"@type":"supergroupFullInfo","description":"","community_id":42}}"#,
    );
    assert_eq!(session.chat_community_id(7), Some(42));
    assert!(session.chat_community_notification_settings(7).is_some());
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateCommunity","community":{"@type":"community","id":42,"have_access":false,"name":"Rustaceans","date":1,"status":{"@type":"communityMemberStatusLeft"},"permissions":{"@type":"communityPermissions","can_edit_chat_list":false},"are_chats_collapsed":false}}"#,
    );
    assert_eq!(
        session.chat_community_id(7),
        None,
        "listed by its full info"
    );
    assert_eq!(
        session.chat_community_id(8),
        None,
        "linked by its own full info"
    );
    assert!(session.chat_community_notification_settings(7).is_none());
}

/// What finished for the screen an account left is not reported on the
/// screen it comes back to (`put_on_screen` calls `forget_outcomes`).
#[test]
fn outcomes_of_a_left_screen_are_forgotten() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    ok_for(
        &mut session,
        &sink,
        &seq,
        CommunitiesPurpose::ToggleMemberBanned {
            member: crate::state::CommunityMember::User(5),
            is_banned: true,
        },
    );
    session.communities.forget_outcomes();
    assert_eq!(session.communities.take_outcome(), None::<CommunityOutcome>);
}
