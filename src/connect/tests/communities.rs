//! Connect-driver tests: the TDLib 1.8.70 community methods (adding and
//! removing chats, the pending requests list, "Show as One Chat",
//! notification settings, bans).
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
use crate::state::{CommunityError, CommunityMember, CommunityOutcome};
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

/// The viewer created this community.
const OWNED: i64 = 42;
/// The viewer is a plain member here.
const MEMBER: i64 = 43;

struct Rig {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    seq: AtomicU64,
    sink: Arc<dyn DiagnosticSink>,
    _dir: std::path::PathBuf,
}

impl Rig {
    fn new() -> Self {
        let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let recorder = Arc::new(RecordingSender::new());
        let seq = AtomicU64::new(0);
        let driver = ready_driver(&recorder, prepared, &sink, &seq);
        let mut rig = Self {
            driver,
            recorder,
            seq,
            sink,
            _dir: dir,
        };
        rig.ingest(
            r#"{"@type":"updateCommunity","community":{"@type":"community","id":42,"have_access":true,"name":"Rustaceans","date":1,"status":{"@type":"communityMemberStatusCreator"},"permissions":{"@type":"communityPermissions","can_edit_chat_list":false},"are_chats_collapsed":false}}"#,
        );
        rig.ingest(
            r#"{"@type":"updateCommunity","community":{"@type":"community","id":43,"have_access":true,"name":"Theirs","date":1,"status":{"@type":"communityMemberStatusMember"},"permissions":{"@type":"communityPermissions","can_edit_chat_list":false},"are_chats_collapsed":false}}"#,
        );
        rig
    }

    fn ingest(&mut self, json: &str) {
        self.driver
            .ingest(copy_and_parse(json, &self.seq, &self.sink).unwrap())
            .unwrap();
    }

    fn last(&self) -> Value {
        serde_json::from_str(&self.recorder.snapshot().last().cloned().unwrap()).unwrap()
    }

    fn sent_count(&self) -> usize {
        self.recorder.snapshot().len()
    }

    fn ok(&mut self, extra: crate::ids::RequestId) {
        self.ingest(&format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0));
    }

    fn error(&mut self, extra: crate::ids::RequestId, message: &str) {
        self.ingest(&format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"{message}"}}"#,
            extra.0
        ));
    }

    fn outcome(&mut self) -> Option<CommunityOutcome> {
        self.driver.session.communities.take_outcome()
    }
}

#[test]
fn add_chat_sends_once_and_reports_each_answer() {
    let mut rig = Rig::new();
    let extra = rig
        .driver
        .add_community_chat(OWNED, 7, true)
        .unwrap()
        .expect("sent");
    let v = rig.last();
    assert_eq!(v["@type"], "addCommunityChat");
    assert_eq!(v["community_id"], OWNED);
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["is_hidden"], true);
    assert!(rig.driver.session.community_add_in_flight(OWNED, 7));
    let before = rig.sent_count();
    assert!(
        rig.driver
            .add_community_chat(OWNED, 7, false)
            .unwrap()
            .is_none()
    );
    assert_eq!(rig.sent_count(), before);
    assert!(
        rig.driver
            .add_community_chat(404, 7, false)
            .unwrap()
            .is_none()
    );

    rig.ok(extra);
    assert!(!rig.driver.session.community_add_in_flight(OWNED, 7));
    assert_eq!(
        rig.outcome(),
        Some(CommunityOutcome::ChatAdded {
            community_id: OWNED,
            chat_id: 7
        })
    );

    let extra = rig
        .driver
        .add_community_chat(MEMBER, 8, false)
        .unwrap()
        .unwrap();
    rig.error(extra, "COMMUNITY_REQUEST_CREATED");
    assert_eq!(
        rig.outcome(),
        Some(CommunityOutcome::AddRequestSent {
            community_id: MEMBER,
            chat_id: 8
        })
    );

    let extra = rig
        .driver
        .add_community_chat(OWNED, 9, false)
        .unwrap()
        .unwrap();
    rig.error(extra, "COMMUNITY_PEERS_TOO_MUCH");
    assert_eq!(
        rig.outcome(),
        Some(CommunityOutcome::AddFailed {
            community_id: OWNED,
            chat_id: 9,
            error: CommunityError::PeersTooMuch
        })
    );
    assert_eq!(rig.outcome(), None);
}

#[test]
fn remove_chat_drops_it_from_the_full_info() {
    let mut rig = Rig::new();
    rig.ingest(
        r#"{"@type":"updateCommunityFullInfo","community_id":42,"community_full_info":{"@type":"communityFullInfo","chats":[{"@type":"communityChat","chat_id":7,"can_view_history":true,"is_hidden":false},{"@type":"communityChat","chat_id":8,"can_view_history":true,"is_hidden":false}],"administrator_count":1,"banned_count":0,"add_chat_request_count":0}}"#,
    );
    let extra = rig.driver.remove_community_chat(OWNED, 7).unwrap().unwrap();
    let v = rig.last();
    assert_eq!(v["@type"], "removeCommunityChat");
    assert_eq!(v["chat_id"], 7);
    rig.ok(extra);
    let chats: Vec<i64> = rig.driver.session.groups.community_full_infos[&OWNED]
        .chats
        .iter()
        .map(|chat| chat.chat_id)
        .collect();
    assert_eq!(chats, vec![8]);
    assert_eq!(
        rig.outcome(),
        Some(CommunityOutcome::ChatRemoved {
            community_id: OWNED,
            chat_id: 7
        })
    );
}

#[test]
fn add_chat_requests_page_and_process() {
    let mut rig = Rig::new();
    rig.ingest(
        r#"{"@type":"updateCommunityFullInfo","community_id":42,"community_full_info":{"@type":"communityFullInfo","chats":[],"administrator_count":1,"banned_count":0,"add_chat_request_count":3}}"#,
    );
    let extra = rig
        .driver
        .fetch_community_add_chat_requests(OWNED, true)
        .unwrap()
        .unwrap();
    let v = rig.last();
    assert_eq!(v["@type"], "getCommunityAddChatRequests");
    assert_eq!(v["offset"], "");
    assert_eq!(v["limit"], 100);
    assert!(
        rig.driver
            .session
            .communities
            .add_chat_requests(OWNED)
            .is_loading()
    );
    rig.ingest(&format!(
        r#"{{"@type":"communityAddChatRequests","@extra":"{}","total_count":3,"requests":[{{"@type":"communityAddChatRequest","chat_id":7,"is_hidden":false,"sender_user_id":5,"date":10}},{{"@type":"communityAddChatRequest","chat_id":8,"is_hidden":true,"sender_user_id":6,"date":11}}],"next_offset":"p2"}}"#,
        extra.0
    ));
    let list = rig
        .driver
        .session
        .communities
        .add_chat_requests(OWNED)
        .data()
        .cloned()
        .unwrap();
    assert_eq!(list.requests.len(), 2);
    assert_eq!(list.next_offset.as_deref(), Some("p2"));
    assert!(list.requests[1].is_hidden);

    let extra = rig
        .driver
        .fetch_community_add_chat_requests(OWNED, false)
        .unwrap()
        .unwrap();
    assert_eq!(rig.last()["offset"], "p2");
    rig.ingest(&format!(
        r#"{{"@type":"communityAddChatRequests","@extra":"{}","total_count":3,"requests":[{{"@type":"communityAddChatRequest","chat_id":9,"is_hidden":false,"sender_user_id":5,"date":12}}],"next_offset":""}}"#,
        extra.0
    ));
    let list = rig.driver.session.communities.add_chat_requests(OWNED);
    assert_eq!(list.data().map(|l| l.requests.len()), Some(3));
    assert!(
        rig.driver
            .fetch_community_add_chat_requests(OWNED, false)
            .unwrap()
            .is_none()
    );

    let extra = rig
        .driver
        .process_community_add_chat_request(OWNED, 8, false)
        .unwrap()
        .unwrap();
    let v = rig.last();
    assert_eq!(v["@type"], "processCommunityAddChatRequest");
    assert_eq!(v["approve"], false);
    assert!(rig.driver.session.community_requests_processing(OWNED));
    rig.ok(extra);
    let ids: Vec<i64> = rig
        .driver
        .session
        .communities
        .add_chat_requests(OWNED)
        .data()
        .unwrap()
        .requests
        .iter()
        .map(|r| r.chat_id)
        .collect();
    assert_eq!(ids, vec![7, 9]);
    assert_eq!(
        rig.driver.session.groups.community_full_infos[&OWNED].add_chat_request_count,
        2
    );
    assert_eq!(
        rig.outcome(),
        Some(CommunityOutcome::RequestProcessed {
            community_id: OWNED,
            chat_id: 8,
            approve: false
        })
    );

    let extra = rig
        .driver
        .process_community_add_chat_requests(OWNED, true, 2)
        .unwrap()
        .unwrap();
    assert_eq!(rig.last()["@type"], "processCommunityAddChatRequests");
    assert!(
        rig.driver
            .process_community_add_chat_requests(OWNED, true, 2)
            .unwrap()
            .is_none()
    );
    rig.ok(extra);
    assert_eq!(
        rig.outcome(),
        Some(CommunityOutcome::AllRequestsProcessed {
            community_id: OWNED,
            approve: true,
            count: 2
        })
    );
    assert_eq!(
        rig.driver.session.groups.community_full_infos[&OWNED].add_chat_request_count,
        0
    );

    let extra = rig
        .driver
        .process_community_add_chat_requests(OWNED, true, 2)
        .unwrap()
        .unwrap();
    rig.error(extra, "COMMUNITY_PEERS_TOO_MUCH");
    assert_eq!(
        rig.outcome(),
        Some(CommunityOutcome::ProcessFailed {
            community_id: OWNED,
            error: CommunityError::PeersTooMuch
        })
    );
}

#[test]
fn collapse_toggle_shows_the_asked_value_until_the_update() {
    let mut rig = Rig::new();
    assert!(!rig.driver.session.community_chats_collapsed(OWNED));
    let extra = rig
        .driver
        .toggle_community_chats_collapsed(OWNED, true)
        .unwrap()
        .unwrap();
    let v = rig.last();
    assert_eq!(v["@type"], "toggleCommunityAreChatsCollapsed");
    assert_eq!(v["are_chats_collapsed"], true);
    assert!(rig.driver.session.community_chats_collapsed(OWNED));
    rig.error(extra, "CHAT_ADMIN_REQUIRED");
    assert!(!rig.driver.session.community_chats_collapsed(OWNED));
    assert!(matches!(
        rig.outcome(),
        Some(CommunityOutcome::CollapseFailed {
            community_id: OWNED,
            ..
        })
    ));

    let extra = rig
        .driver
        .toggle_community_chats_collapsed(OWNED, true)
        .unwrap()
        .unwrap();
    rig.ingest(
        r#"{"@type":"updateCommunity","community":{"@type":"community","id":42,"have_access":true,"name":"Rustaceans","date":1,"status":{"@type":"communityMemberStatusCreator"},"permissions":{"@type":"communityPermissions","can_edit_chat_list":false},"are_chats_collapsed":true}}"#,
    );
    rig.ok(extra);
    assert!(rig.driver.session.community_chats_collapsed(OWNED));
    assert_eq!(rig.outcome(), None);
}

#[test]
fn notification_settings_show_at_once_and_revert_on_failure() {
    let mut rig = Rig::new();
    rig.ingest(
        r#"{"@type":"updateCommunityFullInfo","community_id":42,"community_full_info":{"@type":"communityFullInfo","chats":[],"administrator_count":1,"banned_count":0,"add_chat_request_count":0}}"#,
    );
    let base = rig
        .driver
        .session
        .community_notification_settings(OWNED)
        .unwrap();
    let mut muted = base.clone();
    muted.use_default_mute_for = false;
    muted.mute_for = i32::MAX;
    let extra = rig
        .driver
        .set_community_notification_settings(OWNED, muted.clone())
        .unwrap()
        .unwrap();
    let v = rig.last();
    assert_eq!(v["@type"], "setCommunityNotificationSettings");
    assert_eq!(
        v["notification_settings"]["@type"],
        "chatNotificationSettings"
    );
    assert_eq!(v["notification_settings"]["mute_for"], i32::MAX);
    assert_eq!(
        rig.driver.session.community_notification_settings(OWNED),
        Some(muted.clone())
    );
    rig.error(extra, "PEER_ID_INVALID");
    assert_eq!(
        rig.driver.session.community_notification_settings(OWNED),
        Some(base)
    );

    let extra = rig
        .driver
        .set_community_notification_settings(OWNED, muted.clone())
        .unwrap()
        .unwrap();
    rig.ok(extra);
    assert_eq!(
        rig.driver.session.community_notification_settings(OWNED),
        Some(muted)
    );
}

#[test]
fn ban_asks_member_chats_then_bans() {
    let mut rig = Rig::new();
    let member = CommunityMember::User(5);
    let extra = rig
        .driver
        .fetch_community_member_chats(OWNED, member)
        .unwrap()
        .unwrap();
    let v = rig.last();
    assert_eq!(v["@type"], "getCommunityMemberChats");
    assert_eq!(v["member_id"]["@type"], "messageSenderUser");
    assert_eq!(v["member_id"]["user_id"], 5);
    rig.ingest(&format!(
        r#"{{"@type":"communityMemberChats","@extra":"{}","owned_chat_ids":[7],"joined_chat_ids":[7,8]}}"#,
        extra.0
    ));
    let chats = rig
        .driver
        .session
        .communities
        .member_chats(OWNED, member)
        .data()
        .cloned()
        .unwrap();
    assert_eq!(chats.owned_chat_ids, vec![7]);
    assert_eq!(chats.joined_chat_ids, vec![7, 8]);

    let extra = rig
        .driver
        .ban_community_member(OWNED, member)
        .unwrap()
        .unwrap();
    let v = rig.last();
    assert_eq!(v["@type"], "toggleCommunityMemberIsBanned");
    assert_eq!(v["is_banned"], true);
    rig.ok(extra);
    assert_eq!(
        rig.outcome(),
        Some(CommunityOutcome::MemberBanned {
            community_id: OWNED,
            member
        })
    );

    let chat = CommunityMember::Chat(-1007);
    rig.driver
        .ban_community_member(OWNED, chat)
        .unwrap()
        .unwrap();
    let v = rig.last();
    assert_eq!(v["member_id"]["@type"], "messageSenderChat");
    assert_eq!(v["member_id"]["chat_id"], -1007);
}

#[test]
fn joined_communities_keep_the_last_list_while_reloading() {
    let mut rig = Rig::new();
    let extra = rig.driver.fetch_joined_communities().unwrap().unwrap();
    assert_eq!(rig.last()["@type"], "getJoinedCommunities");
    assert!(rig.driver.fetch_joined_communities().unwrap().is_none());
    rig.ingest(&format!(
        r#"{{"@type":"communities","@extra":"{}","community_ids":[42,43]}}"#,
        extra.0
    ));
    assert_eq!(
        rig.driver.session.communities.joined.data().cloned(),
        Some(vec![OWNED, MEMBER])
    );
    let extra = rig.driver.fetch_joined_communities().unwrap().unwrap();
    assert_eq!(
        rig.driver.session.communities.joined.data().cloned(),
        Some(vec![OWNED, MEMBER])
    );
    rig.error(extra, "NETWORK_ERROR");
    assert!(rig.driver.session.communities.joined.error().is_some());
    assert_eq!(
        rig.driver.session.communities.joined.data().cloned(),
        Some(vec![OWNED, MEMBER])
    );
}

#[test]
fn collapsed_communities_get_their_full_info_once() {
    let mut rig = Rig::new();
    rig.ingest(
        r#"{"@type":"updateCommunity","community":{"@type":"community","id":43,"have_access":true,"name":"Theirs","date":1,"status":{"@type":"communityMemberStatusMember"},"permissions":{"@type":"communityPermissions","can_edit_chat_list":false},"are_chats_collapsed":true}}"#,
    );
    rig.driver.sync_collapsed_communities().unwrap();
    let v = rig.last();
    assert_eq!(v["@type"], "getCommunityFullInfo");
    assert_eq!(v["community_id"], MEMBER);
    let before = rig.sent_count();
    rig.driver.sync_collapsed_communities().unwrap();
    assert_eq!(rig.sent_count(), before);
}

#[test]
fn owned_bots_are_asked_of_tdlib_once_at_a_time() {
    let mut rig = Rig::new();
    let extra = rig.driver.fetch_owned_bots().unwrap().expect("sent");
    assert_eq!(rig.last()["@type"], "getOwnedBots");
    assert_eq!(rig.driver.fetch_owned_bots().unwrap(), None);
    rig.ingest(&format!(
        r#"{{"@type":"users","total_count":1,"user_ids":[77],"@extra":"{}"}}"#,
        extra.0
    ));
    assert_eq!(
        rig.driver.session.communities.owned_bots.data().cloned(),
        Some(vec![77])
    );
}

#[test]
fn a_created_community_gets_the_photo_its_box_chose() {
    let mut rig = Rig::new();
    rig.ingest(
        r#"{"@type":"updateNewChat","chat":{"id":-1007,"title":"Field Notes","type":{"@type":"chatTypeSupergroup","supergroup_id":7,"is_channel":true},"unread_count":0}}"#,
    );
    let extra = rig
        .driver
        .create_community_with_photo(
            crate::ids::ChatId(-1007),
            "Notes",
            true,
            Some("/photos/notes.jpg".into()),
        )
        .unwrap()
        .expect("sent");
    let v = rig.last();
    assert_eq!(v["@type"], "createCommunity");
    assert_eq!(v["is_chat_hidden"], true);
    rig.ingest(
        r#"{"@type":"updateCommunity","community":{"@type":"community","id":44,"have_access":true,"name":"Notes","date":1,"status":{"@type":"communityMemberStatusCreator"},"permissions":{"@type":"communityPermissions","can_edit_chat_list":false},"are_chats_collapsed":false}}"#,
    );
    rig.ingest(&format!(
        r#"{{"@type":"communityId","id":44,"@extra":"{}"}}"#,
        extra.0
    ));
    let sent: Vec<Value> = rig
        .recorder
        .snapshot()
        .iter()
        .map(|json| serde_json::from_str(json).unwrap())
        .collect();
    let photo = sent
        .iter()
        .find(|v| v["@type"] == "setCommunityPhoto")
        .expect("photo sent");
    assert_eq!(photo["community_id"], 44);
    assert_eq!(photo["photo"]["photo"]["path"], "/photos/notes.jpg");
    assert!(sent.iter().any(|v| v["@type"] == "getCommunityFullInfo"));
    assert!(rig.driver.session.communities.new_community_photo.is_none());
    assert_eq!(
        rig.outcome(),
        Some(CommunityOutcome::Created {
            community_id: 44,
            chat_id: Some(-1007),
        })
    );
}

#[test]
fn a_refused_create_reports_and_sets_no_photo() {
    let mut rig = Rig::new();
    rig.ingest(
        r#"{"@type":"updateNewChat","chat":{"id":-1007,"title":"Field Notes","type":{"@type":"chatTypeSupergroup","supergroup_id":7,"is_channel":true},"unread_count":0}}"#,
    );
    let extra = rig
        .driver
        .create_community_with_photo(
            crate::ids::ChatId(-1007),
            "Notes",
            false,
            Some("/photos/notes.jpg".into()),
        )
        .unwrap()
        .expect("sent");
    rig.error(extra, "CHANNELS_TOO_MUCH");
    assert!(matches!(
        rig.outcome(),
        Some(CommunityOutcome::CreateFailed { .. })
    ));
    assert!(rig.driver.session.communities.new_community_photo.is_none());
    assert!(
        !rig.recorder
            .snapshot()
            .iter()
            .any(|json| json.contains("setCommunityPhoto"))
    );
}
