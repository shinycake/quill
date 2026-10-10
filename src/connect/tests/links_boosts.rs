//! Connect-driver tests: other admins' links, per-link join requests, the
//! boosts list and link, and supergroup username changes.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{ChatId, RequestId};
use crate::platform::MemorySecretStore;
use crate::state::InviteLinkFetch;
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const CREATOR: &str = r#"{"@type":"chatMemberStatusCreator","is_member":true}"#;
const ADMIN: &str = r#"{"@type":"chatMemberStatusAdministrator","rights":{"@type":"chatAdministratorRights","can_invite_users":true}}"#;
const USERNAMES: &str = r#","usernames":{"@type":"usernames","active_usernames":["rustaceans","rust_club"],"disabled_usernames":["old_rust"],"editable_username":"rustaceans","collectible_usernames":["rust_club","old_rust"]}"#;

struct Rig {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    seq: AtomicU64,
    sink: Arc<dyn DiagnosticSink>,
    _dir: std::path::PathBuf,
}

impl Rig {
    fn new(status: &str) -> Self {
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
        rig.driver.session.my_user_id = Some(1);
        rig.ingest(
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"C","type":{"@type":"chatTypeSupergroup","supergroup_id":113,"is_channel":false},"unread_count":0}}"#,
        );
        rig.ingest(&format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":113,"status":{status}{USERNAMES}}}}}"#
        ));
        rig
    }

    fn ingest(&mut self, json: &str) {
        self.driver
            .ingest(copy_and_parse(json, &self.seq, &self.sink).unwrap())
            .unwrap();
    }

    fn sent_all(&self, ty: &str) -> Vec<Value> {
        self.recorder
            .snapshot()
            .into_iter()
            .filter(|j| j.contains(&format!(r#""@type":"{ty}""#)))
            .map(|j| serde_json::from_str(&j).unwrap())
            .collect()
    }

    fn last_sent(&self, ty: &str) -> Value {
        self.sent_all(ty)
            .pop()
            .unwrap_or_else(|| panic!("{ty} was not sent"))
    }
}

fn link_json(link: &str, uses: i32) -> String {
    format!(
        r#"{{"@type":"chatInviteLink","invite_link":"{link}","name":"","creator_user_id":9,"date":1,"edit_date":0,"expiration_date":0,"member_limit":0,"member_count":{uses},"expired_member_count":0,"pending_join_request_count":0,"creates_join_request":false,"is_primary":false,"is_revoked":false}}"#
    )
}

fn links_json(extra: u64, links: &[String]) -> String {
    format!(
        r#"{{"@type":"chatInviteLinks","@extra":"{extra}","total_count":{},"invite_links":[{}]}}"#,
        links.len(),
        links.join(",")
    )
}

fn ok_json(extra: RequestId) -> String {
    format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0)
}

#[test]
fn other_admin_links_load_active_and_revoked_for_the_creator() {
    let mut rig = Rig::new(CREATOR);
    let first = rig
        .driver
        .open_admin_invite_links(ChatId(13), 9)
        .unwrap()
        .unwrap();
    let sent = rig.sent_all("getChatInviteLinks");
    assert_eq!(sent.len(), 2);
    assert!(sent.iter().all(|s| s["creator_user_id"] == 9));
    assert_eq!(sent[0]["is_revoked"], false);
    assert_eq!(sent[1]["is_revoked"], true);
    let revoked = first.0 + 1;
    // Answers can come in any order.
    rig.ingest(&links_json(revoked, &[link_json("https://t.me/+old", 3)]));
    rig.ingest(&links_json(first.0, &[link_json("https://t.me/+a", 1)]));
    let state = rig.driver.session.admin_invite_links.get(&13).unwrap();
    let InviteLinkFetch::Loaded(active) = &state.active else {
        panic!("active loaded");
    };
    let InviteLinkFetch::Loaded(old) = &state.revoked else {
        panic!("revoked loaded");
    };
    assert_eq!(active.links[0].invite_link, "https://t.me/+a");
    assert_eq!(old.links[0].invite_link, "https://t.me/+old");
}

#[test]
fn other_admin_links_are_owner_only_and_never_the_viewers_own() {
    let mut admin = Rig::new(ADMIN);
    assert!(
        admin
            .driver
            .open_admin_invite_links(ChatId(13), 9)
            .unwrap()
            .is_none()
    );
    let mut owner = Rig::new(CREATOR);
    assert!(
        owner
            .driver
            .open_admin_invite_links(ChatId(13), 1)
            .unwrap()
            .is_none()
    );
    assert!(owner.sent_all("getChatInviteLinks").is_empty());
}

#[test]
fn a_stale_admin_reply_is_dropped_after_switching_admin() {
    let mut rig = Rig::new(CREATOR);
    let first = rig
        .driver
        .open_admin_invite_links(ChatId(13), 9)
        .unwrap()
        .unwrap();
    let second = rig
        .driver
        .open_admin_invite_links(ChatId(13), 10)
        .unwrap()
        .unwrap();
    rig.ingest(&links_json(first.0, &[link_json("https://t.me/+nine", 1)]));
    let state = rig.driver.session.admin_invite_links.get(&13).unwrap();
    assert_eq!(state.creator_user_id, 10);
    assert!(matches!(state.active, InviteLinkFetch::Loading));
    rig.ingest(&links_json(second.0, &[link_json("https://t.me/+ten", 1)]));
    let state = rig.driver.session.admin_invite_links.get(&13).unwrap();
    assert!(matches!(state.active, InviteLinkFetch::Loaded(_)));
}

#[test]
fn deleting_an_admins_revoked_links_sends_their_id_and_clears_only_their_list() {
    let mut rig = Rig::new(CREATOR);
    rig.driver.open_admin_invite_links(ChatId(13), 9).unwrap();
    let extra = rig
        .driver
        .delete_all_revoked_admin_links(ChatId(13))
        .unwrap()
        .unwrap();
    assert_eq!(
        rig.last_sent("deleteAllRevokedChatInviteLinks")["creator_user_id"],
        9
    );
    rig.ingest(&ok_json(extra));
    let state = rig.driver.session.admin_invite_links.get(&13).unwrap();
    assert!(matches!(
        &state.revoked,
        InviteLinkFetch::Loaded(list) if list.links.is_empty()
    ));
    assert!(!rig.driver.session.revoked_invite_links.contains_key(&13));
}

fn requests_json(extra: u64, total: i32, ids: &[i64]) -> String {
    let rows = ids
        .iter()
        .map(|id| {
            format!(
                r#"{{"@type":"chatJoinRequest","user_id":{id},"date":{},"bio":""}}"#,
                1000 + id
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{"@type":"chatJoinRequests","@extra":"{extra}","total_count":{total},"requests":[{rows}]}}"#
    )
}

#[test]
fn link_join_requests_filter_by_link_page_and_process_in_place() {
    let mut rig = Rig::new(ADMIN);
    let first = rig
        .driver
        .open_link_join_requests(ChatId(13), "https://t.me/+x")
        .unwrap()
        .unwrap();
    let sent = rig.last_sent("getChatJoinRequests");
    assert_eq!(sent["invite_link"], "https://t.me/+x");
    assert_eq!(sent["offset_request"]["user_id"], 0);
    rig.ingest(&requests_json(first.0, 3, &[5, 6]));
    let more = rig
        .driver
        .load_more_link_join_requests(ChatId(13))
        .unwrap()
        .unwrap();
    let sent = rig.last_sent("getChatJoinRequests");
    assert_eq!(sent["offset_request"]["user_id"], 6);
    assert_eq!(sent["offset_request"]["date"], 1006);
    assert_eq!(sent["invite_link"], "https://t.me/+x");
    rig.ingest(&requests_json(more.0, 3, &[7]));
    assert_eq!(
        rig.driver
            .session
            .link_join_requests
            .get(&13)
            .unwrap()
            .requests
            .len(),
        3
    );
    // One approved request leaves the list on `ok`.
    let one = rig
        .driver
        .process_chat_join_request(ChatId(13), 6, true)
        .unwrap()
        .unwrap();
    rig.ingest(&ok_json(one));
    let state = rig.driver.session.link_join_requests.get(&13).unwrap();
    assert_eq!(state.requests.len(), 2);
    assert_eq!(state.total_count, 2);
    // Bulk for the link only.
    let all = rig
        .driver
        .process_link_join_requests(ChatId(13), false)
        .unwrap()
        .unwrap();
    let sent = rig.last_sent("processChatJoinRequests");
    assert_eq!(sent["invite_link"], "https://t.me/+x");
    assert_eq!(sent["approve"], false);
    rig.ingest(&ok_json(all));
    let state = rig.driver.session.link_join_requests.get(&13).unwrap();
    assert!(state.requests.is_empty());
    assert_eq!(state.total_count, 0);
}

#[test]
fn a_late_link_requests_page_for_another_link_is_ignored() {
    let mut rig = Rig::new(ADMIN);
    let old = rig
        .driver
        .open_link_join_requests(ChatId(13), "https://t.me/+a")
        .unwrap()
        .unwrap();
    rig.driver
        .open_link_join_requests(ChatId(13), "https://t.me/+b")
        .unwrap();
    rig.ingest(&requests_json(old.0, 1, &[5]));
    let state = rig.driver.session.link_join_requests.get(&13).unwrap();
    assert_eq!(state.invite_link, "https://t.me/+b");
    assert!(state.requests.is_empty());
    assert!(state.loading);
}

fn boosts_json(extra: u64, total: i32, next: &str, ids: &[i64]) -> String {
    let rows = ids
        .iter()
        .map(|id| {
            format!(
                r#"{{"@type":"chatBoost","id":"b{id}","count":1,"source":{{"@type":"chatBoostSourcePremium","user_id":{id}}},"start_date":100,"expiration_date":900}}"#
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{"@type":"foundChatBoosts","@extra":"{extra}","total_count":{total},"boosts":[{rows}],"next_offset":"{next}"}}"#
    )
}

#[test]
fn boosts_page_with_the_server_offset_and_switch_tabs() {
    let mut rig = Rig::new(ADMIN);
    let first = rig
        .driver
        .open_chat_boosts(ChatId(13), false)
        .unwrap()
        .unwrap();
    let sent = rig.last_sent("getChatBoosts");
    assert_eq!(sent["only_gift_codes"], false);
    assert_eq!(sent["offset"], "");
    rig.ingest(&boosts_json(first.0, 3, "n1", &[5, 6]));
    let more = rig
        .driver
        .load_more_chat_boosts(ChatId(13))
        .unwrap()
        .unwrap();
    assert_eq!(rig.last_sent("getChatBoosts")["offset"], "n1");
    rig.ingest(&boosts_json(more.0, 3, "", &[7]));
    let state = rig.driver.session.chat_boost_lists.get(&13).unwrap();
    assert_eq!(state.boosts.len(), 3);
    assert!(state.next_offset.is_empty());
    // The last page ends paging.
    assert!(
        rig.driver
            .load_more_chat_boosts(ChatId(13))
            .unwrap()
            .is_none()
    );
    // Switching to the Gifts tab resets and asks for gift codes only.
    let gifts = rig
        .driver
        .open_chat_boosts(ChatId(13), true)
        .unwrap()
        .unwrap();
    assert_eq!(rig.last_sent("getChatBoosts")["only_gift_codes"], true);
    // The earlier tab's late page is stale.
    rig.ingest(&boosts_json(first.0, 3, "", &[9]));
    assert!(
        rig.driver
            .session
            .chat_boost_lists
            .get(&13)
            .unwrap()
            .boosts
            .is_empty()
    );
    rig.ingest(&boosts_json(gifts.0, 0, "", &[]));
    assert!(
        !rig.driver
            .session
            .chat_boost_lists
            .get(&13)
            .unwrap()
            .loading
    );
}

#[test]
fn boost_link_is_fetched_once_and_cached() {
    let mut rig = Rig::new(CREATOR);
    let extra = rig
        .driver
        .fetch_chat_boost_link(ChatId(13))
        .unwrap()
        .unwrap();
    assert_eq!(rig.last_sent("getChatBoostLink")["chat_id"], 13);
    rig.ingest(&format!(
        r#"{{"@type":"chatBoostLink","@extra":"{}","link":"https://t.me/boost/rustaceans","is_public":true}}"#,
        extra.0
    ));
    assert_eq!(
        rig.driver.session.chat_boost_links.get(&13),
        Some(&("https://t.me/boost/rustaceans".to_string(), true))
    );
    assert!(
        rig.driver
            .fetch_chat_boost_link(ChatId(13))
            .unwrap()
            .is_none()
    );
}

#[test]
fn boosts_need_an_admin() {
    let mut rig = Rig::new(r#"{"@type":"chatMemberStatusMember"}"#);
    assert!(
        rig.driver
            .open_chat_boosts(ChatId(13), false)
            .unwrap()
            .is_none()
    );
    assert!(
        rig.driver
            .fetch_chat_boost_link(ChatId(13))
            .unwrap()
            .is_none()
    );
    assert!(rig.sent_all("getChatBoosts").is_empty());
}

#[test]
fn usernames_toggle_and_reorder_send_the_supergroup_requests() {
    let mut rig = Rig::new(CREATOR);
    rig.driver
        .toggle_group_username(ChatId(13), "old_rust", true)
        .unwrap()
        .unwrap();
    let sent = rig.last_sent("toggleSupergroupUsernameIsActive");
    assert_eq!(sent["supergroup_id"], 113);
    assert_eq!(sent["username"], "old_rust");
    assert_eq!(sent["is_active"], true);
    // One change at a time.
    assert!(
        rig.driver
            .move_group_username(ChatId(13), "rust_club", true)
            .unwrap()
            .is_none()
    );
}

#[test]
fn reordering_moves_one_slot_and_sends_the_full_order() {
    let mut rig = Rig::new(CREATOR);
    rig.driver
        .move_group_username(ChatId(13), "rust_club", true)
        .unwrap()
        .unwrap();
    assert_eq!(
        rig.last_sent("reorderSupergroupActiveUsernames")["usernames"],
        serde_json::json!(["rust_club", "rustaceans"])
    );
    // The top one can't go further up.
    let mut top = Rig::new(CREATOR);
    assert!(
        top.driver
            .move_group_username(ChatId(13), "rustaceans", true)
            .unwrap()
            .is_none()
    );
}

#[test]
fn the_editable_username_cannot_be_turned_off_and_only_owners_edit() {
    let mut owner = Rig::new(CREATOR);
    assert!(
        owner
            .driver
            .toggle_group_username(ChatId(13), "rustaceans", false)
            .unwrap()
            .is_none()
    );
    let mut admin = Rig::new(ADMIN);
    assert!(
        admin
            .driver
            .toggle_group_username(ChatId(13), "old_rust", true)
            .unwrap()
            .is_none()
    );
    assert!(
        admin
            .sent_all("toggleSupergroupUsernameIsActive")
            .is_empty()
    );
}

#[test]
fn update_supergroup_refreshes_the_cached_username_lists() {
    let mut rig = Rig::new(CREATOR);
    assert_eq!(
        rig.driver
            .session
            .chat_usernames(ChatId(13))
            .unwrap()
            .disabled,
        vec!["old_rust"]
    );
    rig.ingest(
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":113,"status":{"@type":"chatMemberStatusCreator","is_member":true},"usernames":{"@type":"usernames","active_usernames":["rustaceans","rust_club","old_rust"],"disabled_usernames":[],"editable_username":"rustaceans","collectible_usernames":["rust_club","old_rust"]}}}"#,
    );
    let lists = rig.driver.session.chat_usernames(ChatId(13)).unwrap();
    assert_eq!(lists.active.len(), 3);
    assert!(lists.disabled.is_empty());
}

#[test]
fn a_refused_username_change_reports_an_error() {
    let mut rig = Rig::new(CREATOR);
    let extra = rig
        .driver
        .toggle_group_username(ChatId(13), "old_rust", true)
        .unwrap()
        .unwrap();
    rig.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"USERNAMES_ACTIVE_TOO_MUCH"}}"#,
        extra.0
    ));
    assert!(rig.driver.session.chat_action_error.is_some());
}
