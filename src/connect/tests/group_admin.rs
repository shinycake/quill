//! Connect-driver tests: B7 group admin toggles (permission gating,
//! optimistic updates, chained steps).
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::ChatId;
use crate::platform::MemorySecretStore;
use crate::state::AdminFollowup;
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::ChatAvailableReactions;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const CREATOR: &str = r#"{"@type":"chatMemberStatusCreator","is_member":true}"#;
const MEMBER: &str = r#"{"@type":"chatMemberStatusMember"}"#;

fn ingest(
    driver: &mut ConnectDriver<Arc<RecordingSender>>,
    json: &str,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
) {
    driver
        .ingest(copy_and_parse(json, seq, sink).unwrap())
        .unwrap();
}

fn megagroup(
    driver: &mut ConnectDriver<Arc<RecordingSender>>,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
    chat_id: i64,
    status: &str,
    full_info_fields: &str,
) -> i64 {
    let sg = chat_id + 100;
    ingest(
        driver,
        &format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"G","type":{{"@type":"chatTypeSupergroup","supergroup_id":{sg},"is_channel":false}},"unread_count":0}}}}"#
        ),
        seq,
        sink,
    );
    ingest(
        driver,
        &format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{sg},"member_count":500,"status":{status}}}}}"#
        ),
        seq,
        sink,
    );
    ingest(
        driver,
        &format!(
            r#"{{"@type":"updateSupergroupFullInfo","supergroup_id":{sg},"supergroup_full_info":{{"description":"d","member_count":500{full_info_fields}}}}}"#
        ),
        seq,
        sink,
    );
    sg
}

fn count(recorder: &Arc<RecordingSender>, ty: &str) -> usize {
    recorder
        .snapshot()
        .iter()
        .filter(|j| j.contains(&format!(r#""@type":"{ty}""#)))
        .count()
}

#[test]
fn owner_toggles_go_out_and_update_optimistically() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let sg = megagroup(
        &mut driver,
        &seq,
        &sink,
        13,
        CREATOR,
        r#","can_hide_members":true,"is_all_history_available":false"#,
    );
    let history = driver
        .set_group_history_visible(ChatId(13), true)
        .unwrap()
        .expect("history toggle sent");
    assert!(
        driver.session.supergroup_full_infos[&sg]
            .admin
            .is_all_history_available
    );
    // A second tap while it is in flight is dropped.
    assert!(
        driver
            .set_group_history_visible(ChatId(13), false)
            .unwrap()
            .is_none()
    );
    // The server refuses: the value goes back and the error is reported.
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_NOT_MODIFIED"}}"#,
            history.0
        ),
        &seq,
        &sink,
    );
    assert!(
        !driver.session.supergroup_full_infos[&sg]
            .admin
            .is_all_history_available
    );
    assert!(driver.session.chat_action_error.is_some());

    driver
        .set_group_hidden_members(ChatId(13), true)
        .unwrap()
        .unwrap();
    assert!(
        driver.session.supergroup_full_infos[&sg]
            .admin
            .has_hidden_members
    );
    driver
        .set_chat_protected_content(ChatId(13), true)
        .unwrap()
        .unwrap();
    assert!(driver.session.chat_has_protected_content(ChatId(13)));
    driver
        .set_chat_reactions(ChatId(13), ChatAvailableReactions::none())
        .unwrap()
        .unwrap();
    assert!(
        driver
            .session
            .chat_available_reactions(ChatId(13))
            .unwrap()
            .is_none()
    );
    assert_eq!(count(&recorder, "toggleSupergroupHasHiddenMembers"), 1);
    assert_eq!(count(&recorder, "toggleChatHasProtectedContent"), 1);
    assert_eq!(count(&recorder, "setChatAvailableReactions"), 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn controls_the_viewer_lacks_are_never_sent() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    megagroup(
        &mut driver,
        &seq,
        &sink,
        13,
        MEMBER,
        r#","can_hide_members":false,"linked_chat_id":-1009"#,
    );
    assert!(
        driver
            .set_group_topics(ChatId(13), true, true)
            .unwrap()
            .is_none()
    );
    assert!(
        driver
            .set_group_history_visible(ChatId(13), true)
            .unwrap()
            .is_none()
    );
    assert!(
        driver
            .set_group_join_to_send(ChatId(13), true)
            .unwrap()
            .is_none()
    );
    assert!(
        driver
            .set_group_hidden_members(ChatId(13), true)
            .unwrap()
            .is_none()
    );
    assert!(
        driver
            .set_chat_protected_content(ChatId(13), true)
            .unwrap()
            .is_none()
    );
    assert!(
        driver
            .set_chat_reactions(ChatId(13), ChatAvailableReactions::none())
            .unwrap()
            .is_none()
    );
    assert!(
        driver
            .unlink_discussion_group(ChatId(13))
            .unwrap()
            .is_none()
    );
    assert!(driver.upgrade_basic_group(ChatId(13)).unwrap().is_none());
    assert!(
        recorder
            .snapshot()
            .iter()
            .all(|j| { !j.contains("toggleSupergroup") && !j.contains("toggleChatHasProtected") })
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn topics_respect_the_member_floor_and_the_linked_channel() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    megagroup(&mut driver, &seq, &sink, 13, CREATOR, "");
    // 500 members, not linked: allowed.
    assert!(
        driver
            .set_group_topics(ChatId(13), true, false)
            .unwrap()
            .is_some()
    );
    assert_eq!(count(&recorder, "toggleSupergroupIsForum"), 1);
    // A discussion group stays locked.
    megagroup(
        &mut driver,
        &seq,
        &sink,
        14,
        CREATOR,
        r#","linked_chat_id":-1002"#,
    );
    assert!(
        driver
            .set_group_topics(ChatId(14), true, false)
            .unwrap()
            .is_none()
    );
    assert_eq!(count(&recorder, "toggleSupergroupIsForum"), 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn basic_group_topics_upgrade_first_then_enable() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    ingest(
        &mut driver,
        r#"{"@type":"updateNewChat","chat":{"id":30,"title":"Basic","type":{"@type":"chatTypeBasicGroup","basic_group_id":7},"unread_count":0}}"#,
        &seq,
        &sink,
    );
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"updateBasicGroup","basic_group":{{"@type":"basicGroup","id":7,"member_count":250,"status":{CREATOR},"is_active":true}}}}"#
        ),
        &seq,
        &sink,
    );
    let upgrade = driver
        .set_group_topics(ChatId(30), true, true)
        .unwrap()
        .expect("upgrade sent");
    assert_eq!(count(&recorder, "upgradeBasicGroupChatToSupergroupChat"), 1);
    assert_eq!(count(&recorder, "toggleSupergroupIsForum"), 0);
    // The new supergroup arrives (its owner status first), then the chat
    // answer; topics are enabled on the new group.
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":777,"member_count":250,"status":{CREATOR}}}}}"#
        ),
        &seq,
        &sink,
    );
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"chat","@extra":"{}","id":-100777,"title":"Basic","type":{{"@type":"chatTypeSupergroup","supergroup_id":777,"is_channel":false}},"unread_count":0}}"#,
            upgrade.0
        ),
        &seq,
        &sink,
    );
    let forum = recorder
        .snapshot()
        .into_iter()
        .find(|j| j.contains("toggleSupergroupIsForum"))
        .expect("topics enabled after the upgrade");
    assert!(forum.contains(r#""supergroup_id":777"#));
    assert!(forum.contains(r#""is_forum":true"#));
    assert!(forum.contains(r#""has_forum_tabs":true"#));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_failed_upgrade_drops_its_followup() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    ingest(
        &mut driver,
        r#"{"@type":"updateNewChat","chat":{"id":30,"title":"Basic","type":{"@type":"chatTypeBasicGroup","basic_group_id":7},"unread_count":0}}"#,
        &seq,
        &sink,
    );
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"updateBasicGroup","basic_group":{{"@type":"basicGroup","id":7,"member_count":12,"status":{CREATOR},"is_active":true}}}}"#
        ),
        &seq,
        &sink,
    );
    // Twelve members are too few for topics: no upgrade is started.
    assert!(
        driver
            .set_group_topics(ChatId(30), true, true)
            .unwrap()
            .is_none()
    );
    assert_eq!(count(&recorder, "upgradeBasicGroupChatToSupergroupChat"), 0);
    let upgrade = driver
        .set_group_history_visible(ChatId(30), true)
        .unwrap()
        .expect("upgrade sent");
    assert_eq!(driver.session.admin_followups.len(), 1);
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"X"}}"#,
            upgrade.0
        ),
        &seq,
        &sink,
    );
    assert!(driver.session.admin_followups.is_empty());
    assert_eq!(count(&recorder, "toggleSupergroupIsAllHistoryAvailable"), 0);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn linking_a_hidden_history_group_toggles_history_first() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    // Channel 20 (admin with can_change_info) and group 13.
    ingest(
        &mut driver,
        r#"{"@type":"updateNewChat","chat":{"id":20,"title":"C","type":{"@type":"chatTypeSupergroup","supergroup_id":120,"is_channel":true},"unread_count":0}}"#,
        &seq,
        &sink,
    );
    ingest(
        &mut driver,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":120,"is_channel":true,"status":{"@type":"chatMemberStatusAdministrator","rights":{"@type":"chatAdministratorRights","can_change_info":true}}}}"#,
        &seq,
        &sink,
    );
    let sg = megagroup(
        &mut driver,
        &seq,
        &sink,
        13,
        CREATOR,
        r#","is_all_history_available":false"#,
    );
    let history = driver
        .link_discussion_group(ChatId(20), ChatId(13))
        .unwrap()
        .expect("history toggle first");
    assert_eq!(count(&recorder, "setChatDiscussionGroup"), 0);
    let toggle = recorder
        .snapshot()
        .into_iter()
        .find(|j| j.contains("toggleSupergroupIsAllHistoryAvailable"))
        .unwrap();
    assert!(toggle.contains(&format!(r#""supergroup_id":{sg}"#)));
    ingest(
        &mut driver,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, history.0),
        &seq,
        &sink,
    );
    let link = recorder
        .snapshot()
        .into_iter()
        .find(|j| j.contains("setChatDiscussionGroup"))
        .expect("link follows the history toggle");
    assert!(link.contains(r#""chat_id":20"#));
    assert!(link.contains(r#""discussion_chat_id":13"#));
    // The server answers the first link, then a group that is already
    // visible links straight away.
    let link_json: serde_json::Value = serde_json::from_str(&link).unwrap();
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"ok","@extra":"{}"}}"#,
            link_json["@extra"].as_str().unwrap()
        ),
        &seq,
        &sink,
    );
    megagroup(
        &mut driver,
        &seq,
        &sink,
        15,
        CREATOR,
        r#","is_all_history_available":true"#,
    );
    driver
        .link_discussion_group(ChatId(20), ChatId(15))
        .unwrap()
        .unwrap();
    assert_eq!(count(&recorder, "setChatDiscussionGroup"), 2);
    // A non-admin of the channel can't link.
    assert!(
        driver
            .link_discussion_group(ChatId(13), ChatId(15))
            .unwrap()
            .is_none()
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn followups_with_a_failed_history_step_still_try_the_link() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    ingest(
        &mut driver,
        r#"{"@type":"updateNewChat","chat":{"id":20,"title":"C","type":{"@type":"chatTypeSupergroup","supergroup_id":120,"is_channel":true},"unread_count":0}}"#,
        &seq,
        &sink,
    );
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":120,"is_channel":true,"status":{CREATOR}}}}}"#
        ),
        &seq,
        &sink,
    );
    megagroup(&mut driver, &seq, &sink, 13, MEMBER, "");
    let extra = driver
        .link_discussion_group(ChatId(20), ChatId(13))
        .unwrap()
        .unwrap();
    assert!(matches!(
        driver.session.admin_followups.first(),
        Some((_, AdminFollowup::LinkAfterHistory { .. }))
    ));
    // Not an admin of the group: the history toggle fails, the link is
    // still attempted so the server gives the real answer.
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_ADMIN_REQUIRED"}}"#,
            extra.0
        ),
        &seq,
        &sink,
    );
    assert_eq!(count(&recorder, "setChatDiscussionGroup"), 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn unlinking_works_from_the_channel_and_from_the_group() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    // Channel owner with a linked group.
    ingest(
        &mut driver,
        r#"{"@type":"updateNewChat","chat":{"id":20,"title":"C","type":{"@type":"chatTypeSupergroup","supergroup_id":120,"is_channel":true},"unread_count":0}}"#,
        &seq,
        &sink,
    );
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":120,"is_channel":true,"status":{CREATOR}}}}}"#
        ),
        &seq,
        &sink,
    );
    ingest(
        &mut driver,
        r#"{"@type":"updateSupergroupFullInfo","supergroup_id":120,"supergroup_full_info":{"description":"","member_count":10,"linked_chat_id":13}}"#,
        &seq,
        &sink,
    );
    driver.unlink_discussion_group(ChatId(20)).unwrap().unwrap();
    // A group owner unlinks from the group side.
    megagroup(
        &mut driver,
        &seq,
        &sink,
        13,
        CREATOR,
        r#","linked_chat_id":20"#,
    );
    driver.unlink_discussion_group(ChatId(13)).unwrap().unwrap();
    let sent: Vec<String> = recorder
        .snapshot()
        .into_iter()
        .filter(|j| j.contains("setChatDiscussionGroup"))
        .collect();
    assert_eq!(sent.len(), 2);
    assert!(sent[0].contains(r#""chat_id":20"#) && sent[0].contains(r#""discussion_chat_id":0"#));
    assert!(sent[1].contains(r#""chat_id":0"#) && sent[1].contains(r#""discussion_chat_id":13"#));
    let _ = std::fs::remove_dir_all(dir);
}
