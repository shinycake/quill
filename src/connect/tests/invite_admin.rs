//! Connect-driver tests: B8 join-request search/paging/bulk, invite-link
//! members and counts, revoked-link deletion, subscription links.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{ChatId, RequestId};
use crate::platform::MemorySecretStore;
use crate::state::{InviteLinkCountsFetch, InviteLinkFetch, InviteLinkList, JoinRequestFetch};
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::ChannelMemberStatus;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const CREATOR: &str = r#"{"@type":"chatMemberStatusCreator","is_member":true}"#;
const MEMBER: &str = r#"{"@type":"chatMemberStatusMember"}"#;
const ADMIN: &str = r#"{"@type":"chatMemberStatusAdministrator","rights":{"@type":"chatAdministratorRights","can_invite_users":true}}"#;

struct Rig {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    seq: AtomicU64,
    sink: Arc<dyn DiagnosticSink>,
    _dir: std::path::PathBuf,
}

impl Rig {
    fn new(is_channel: bool, status: &str) -> Self {
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
        rig.ingest(&format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":13,"title":"C","type":{{"@type":"chatTypeSupergroup","supergroup_id":113,"is_channel":{is_channel}}},"unread_count":0}}}}"#
        ));
        rig.ingest(&format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":113,"status":{status}}}}}"#
        ));
        // Channel membership is probed through `getChatMember` in the app;
        // record the probe result directly.
        if is_channel && let Some(chat) = rig.driver.session.chats.get_mut(&13) {
            if status == CREATOR {
                chat.my_member_status = Some(ChannelMemberStatus::Creator);
            } else if status == ADMIN {
                chat.my_member_status = Some(ChannelMemberStatus::Administrator);
                chat.set_admin_can_invite_users(Some(true));
            } else {
                chat.my_member_status = Some(ChannelMemberStatus::Member);
            }
        }
        rig
    }

    fn ingest(&mut self, json: &str) {
        self.driver
            .ingest(copy_and_parse(json, &self.seq, &self.sink).unwrap())
            .unwrap();
    }

    fn last_sent(&self, ty: &str) -> Value {
        let json = self
            .recorder
            .snapshot()
            .into_iter()
            .rev()
            .find(|j| j.contains(&format!(r#""@type":"{ty}""#)))
            .unwrap_or_else(|| panic!("{ty} was not sent"));
        serde_json::from_str(&json).unwrap()
    }

    fn sent_count(&self, ty: &str) -> usize {
        self.recorder
            .snapshot()
            .iter()
            .filter(|j| j.contains(&format!(r#""@type":"{ty}""#)))
            .count()
    }
}

fn requests_json(extra: RequestId, total: i32, ids: &[i64]) -> String {
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
        r#"{{"@type":"chatJoinRequests","@extra":"{}","total_count":{total},"requests":[{rows}]}}"#,
        extra.0
    )
}

#[test]
fn join_request_search_drops_stale_replies_and_pages_with_offset() {
    let mut rig = Rig::new(false, ADMIN);
    let first = rig
        .driver
        .search_chat_join_requests(ChatId(13), "an")
        .unwrap()
        .unwrap();
    let sent = rig.last_sent("getChatJoinRequests");
    assert_eq!(sent["query"], "an");
    // A newer search supersedes the first.
    let second = rig
        .driver
        .search_chat_join_requests(ChatId(13), "ann")
        .unwrap()
        .unwrap();
    rig.ingest(&requests_json(first, 9, &[1, 2]));
    assert!(matches!(
        rig.driver.session.groups.join_requests.get(&13),
        Some(JoinRequestFetch::Loading)
    ));
    rig.ingest(&requests_json(second, 3, &[5, 6]));
    let Some(JoinRequestFetch::Loaded(list)) = rig.driver.session.groups.join_requests.get(&13)
    else {
        panic!("loaded");
    };
    assert_eq!(list.requests.len(), 2);
    assert_eq!(list.total_count, 3);

    // The next page continues after the last row and appends.
    let more = rig
        .driver
        .load_more_chat_join_requests(ChatId(13))
        .unwrap()
        .unwrap();
    let sent = rig.last_sent("getChatJoinRequests");
    assert_eq!(sent["query"], "ann");
    assert_eq!(sent["offset_request"]["user_id"], 6);
    assert_eq!(sent["offset_request"]["date"], 1006);
    // Not while a page is in flight.
    assert!(
        rig.driver
            .load_more_chat_join_requests(ChatId(13))
            .unwrap()
            .is_none()
    );
    rig.ingest(&requests_json(more, 3, &[7]));
    let Some(JoinRequestFetch::Loaded(list)) = rig.driver.session.groups.join_requests.get(&13)
    else {
        panic!("loaded");
    };
    assert_eq!(
        list.requests.iter().map(|r| r.user_id).collect::<Vec<_>>(),
        vec![5, 6, 7]
    );
    // Everything is loaded: no further page.
    assert!(
        rig.driver
            .load_more_chat_join_requests(ChatId(13))
            .unwrap()
            .is_none()
    );
}

#[test]
fn process_all_join_requests_clears_the_list_on_ok() {
    let mut rig = Rig::new(false, ADMIN);
    let fetch = rig
        .driver
        .fetch_chat_join_requests(ChatId(13))
        .unwrap()
        .unwrap();
    rig.ingest(&requests_json(fetch, 2, &[1, 2]));
    let extra = rig
        .driver
        .process_all_chat_join_requests(ChatId(13), true)
        .unwrap()
        .unwrap();
    let sent = rig.last_sent("processChatJoinRequests");
    assert_eq!(sent["approve"], true);
    assert_eq!(sent["invite_link"], "");
    // A second tap while it is in flight is dropped.
    assert!(
        rig.driver
            .process_all_chat_join_requests(ChatId(13), false)
            .unwrap()
            .is_none()
    );
    rig.ingest(&format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0));
    let Some(JoinRequestFetch::Loaded(list)) = rig.driver.session.groups.join_requests.get(&13)
    else {
        panic!("loaded");
    };
    assert!(list.requests.is_empty());
    assert_eq!(list.total_count, 0);
    assert_eq!(
        rig.driver
            .session
            .groups
            .pending_join_request_counts
            .get(&13),
        Some(&0)
    );
}

#[test]
fn process_all_failure_keeps_the_list_and_reports() {
    let mut rig = Rig::new(false, ADMIN);
    let fetch = rig
        .driver
        .fetch_chat_join_requests(ChatId(13))
        .unwrap()
        .unwrap();
    rig.ingest(&requests_json(fetch, 1, &[1]));
    let extra = rig
        .driver
        .process_all_chat_join_requests(ChatId(13), false)
        .unwrap()
        .unwrap();
    rig.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_ADMIN_REQUIRED"}}"#,
        extra.0
    ));
    assert!(matches!(
        rig.driver.session.groups.join_requests.get(&13),
        Some(JoinRequestFetch::Loaded(_))
    ));
    assert!(rig.driver.session.groups.invite_link_error.is_some());
}

#[test]
fn plain_members_cannot_use_any_invite_admin_call() {
    let mut rig = Rig::new(true, MEMBER);
    assert!(
        rig.driver
            .search_chat_join_requests(ChatId(13), "x")
            .unwrap()
            .is_none()
    );
    assert!(
        rig.driver
            .process_all_chat_join_requests(ChatId(13), true)
            .unwrap()
            .is_none()
    );
    assert!(
        rig.driver
            .open_chat_invite_link_members(ChatId(13), "https://t.me/+a")
            .unwrap()
            .is_none()
    );
    assert!(
        rig.driver
            .fetch_revoked_chat_invite_links(ChatId(13))
            .unwrap()
            .is_none()
    );
    assert!(
        rig.driver
            .delete_all_revoked_chat_invite_links(ChatId(13))
            .unwrap()
            .is_none()
    );
    assert!(
        rig.driver
            .create_chat_subscription_invite_link(ChatId(13), "VIP", 100)
            .unwrap()
            .is_none()
    );
    assert_eq!(rig.sent_count("getChatJoinRequests"), 0);
    assert_eq!(rig.sent_count("processChatJoinRequests"), 0);
}

#[test]
fn link_counts_are_owner_only() {
    let mut admin = Rig::new(true, ADMIN);
    assert!(
        admin
            .driver
            .fetch_chat_invite_link_counts(ChatId(13))
            .unwrap()
            .is_none()
    );
    let mut owner = Rig::new(true, CREATOR);
    let extra = owner
        .driver
        .fetch_chat_invite_link_counts(ChatId(13))
        .unwrap()
        .unwrap();
    owner.ingest(&format!(
        r#"{{"@type":"chatInviteLinkCounts","@extra":"{}","invite_link_counts":[{{"@type":"chatInviteLinkCount","user_id":5,"invite_link_count":2,"revoked_invite_link_count":1}}]}}"#,
        extra.0
    ));
    let Some(InviteLinkCountsFetch::Loaded(counts)) =
        owner.driver.session.groups.invite_link_counts.get(&13)
    else {
        panic!("loaded");
    };
    assert_eq!(counts[0].user_id, 5);
    assert_eq!(counts[0].invite_link_count, 2);
    assert_eq!(counts[0].revoked_invite_link_count, 1);
}

fn members_json(extra: RequestId, total: i32, ids: &[i64]) -> String {
    let rows = ids
        .iter()
        .map(|id| {
            format!(r#"{{"@type":"chatInviteLinkMember","user_id":{id},"joined_chat_date":{},"via_chat_folder_invite_link":false,"approver_user_id":0}}"#, 2000 + id)
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{"@type":"chatInviteLinkMembers","@extra":"{}","total_count":{total},"members":[{rows}]}}"#,
        extra.0
    )
}

#[test]
fn link_members_page_and_ignore_replies_for_a_previous_link() {
    let mut rig = Rig::new(true, ADMIN);
    let first = rig
        .driver
        .open_chat_invite_link_members(ChatId(13), "https://t.me/+a")
        .unwrap()
        .unwrap();
    assert!(rig.last_sent("getChatInviteLinkMembers")["offset_member"].is_null());
    // Details switch to another link before the first answers.
    let second = rig
        .driver
        .open_chat_invite_link_members(ChatId(13), "https://t.me/+b")
        .unwrap()
        .unwrap();
    rig.ingest(&members_json(first, 5, &[1, 2]));
    let state = &rig.driver.session.groups.invite_link_members[&13];
    assert!(state.members.is_empty());
    assert!(state.loading);
    rig.ingest(&members_json(second, 3, &[7, 8]));
    let state = &rig.driver.session.groups.invite_link_members[&13];
    assert_eq!(state.invite_link, "https://t.me/+b");
    assert_eq!(state.members.len(), 2);
    assert_eq!(state.total_count, 3);

    let more = rig
        .driver
        .load_more_chat_invite_link_members(ChatId(13))
        .unwrap()
        .unwrap();
    let sent = rig.last_sent("getChatInviteLinkMembers");
    assert_eq!(sent["offset_member"]["user_id"], 8);
    assert_eq!(sent["offset_member"]["joined_chat_date"], 2008);
    assert_eq!(sent["invite_link"], "https://t.me/+b");
    rig.ingest(&members_json(more, 3, &[9]));
    let state = &rig.driver.session.groups.invite_link_members[&13];
    assert_eq!(
        state.members.iter().map(|m| m.user_id).collect::<Vec<_>>(),
        vec![7, 8, 9]
    );
    assert!(
        rig.driver
            .load_more_chat_invite_link_members(ChatId(13))
            .unwrap()
            .is_none()
    );
    rig.driver.close_chat_invite_link_members(ChatId(13));
    assert!(
        !rig.driver
            .session
            .groups
            .invite_link_members
            .contains_key(&13)
    );
}

fn link_json(link: &str, revoked: bool, primary: bool) -> String {
    format!(
        r#"{{"@type":"chatInviteLink","invite_link":"{link}","name":"","creator_user_id":777,"date":1,"edit_date":0,"expiration_date":0,"member_limit":0,"member_count":0,"expired_member_count":0,"pending_join_request_count":0,"creates_join_request":false,"is_primary":{primary},"is_revoked":{revoked}}}"#
    )
}

#[test]
fn revoke_moves_the_link_and_deletes_remove_it_from_the_revoked_list() {
    let mut rig = Rig::new(false, ADMIN);
    let list = rig
        .driver
        .fetch_chat_invite_links(ChatId(13))
        .unwrap()
        .unwrap();
    rig.ingest(&format!(
        r#"{{"@type":"chatInviteLinks","@extra":"{}","total_count":2,"invite_links":[{},{}]}}"#,
        list.0,
        link_json("https://t.me/+one", false, false),
        link_json("https://t.me/+two", false, false)
    ));
    let revoked = rig
        .driver
        .fetch_revoked_chat_invite_links(ChatId(13))
        .unwrap()
        .unwrap();
    assert_eq!(rig.last_sent("getChatInviteLinks")["is_revoked"], true);
    rig.ingest(&format!(
        r#"{{"@type":"chatInviteLinks","@extra":"{}","total_count":0,"invite_links":[]}}"#,
        revoked.0
    ));
    // Revoking answers with only the revoked link, not the whole list.
    let revoke = rig
        .driver
        .revoke_chat_invite_link(ChatId(13), "https://t.me/+one")
        .unwrap()
        .unwrap();
    rig.ingest(&format!(
        r#"{{"@type":"chatInviteLinks","@extra":"{}","total_count":1,"invite_links":[{}]}}"#,
        revoke.0,
        link_json("https://t.me/+one", true, false)
    ));
    let Some(InviteLinkFetch::Loaded(active)) = rig.driver.session.groups.invite_links.get(&13)
    else {
        panic!("active loaded");
    };
    assert_eq!(active.links.len(), 1);
    assert_eq!(active.links[0].invite_link, "https://t.me/+two");
    assert_eq!(active.total_count, 1);
    let Some(InviteLinkFetch::Loaded(gone)) =
        rig.driver.session.groups.revoked_invite_links.get(&13)
    else {
        panic!("revoked loaded");
    };
    assert_eq!(gone.links.len(), 1);

    let delete = rig
        .driver
        .delete_revoked_chat_invite_link(ChatId(13), "https://t.me/+one")
        .unwrap()
        .unwrap();
    assert_eq!(
        rig.last_sent("deleteRevokedChatInviteLink")["invite_link"],
        "https://t.me/+one"
    );
    rig.ingest(&format!(r#"{{"@type":"ok","@extra":"{}"}}"#, delete.0));
    let Some(InviteLinkFetch::Loaded(gone)) =
        rig.driver.session.groups.revoked_invite_links.get(&13)
    else {
        panic!("revoked loaded");
    };
    assert!(gone.links.is_empty());
    assert!(rig.driver.session.groups.revoked_link_deletions.is_empty());

    // Delete-all empties a populated list on ok.
    rig.driver.session.groups.revoked_invite_links.insert(
        13,
        InviteLinkFetch::Loaded(InviteLinkList {
            total_count: 1,
            links: vec![
                crate::telegram::envelope::parse_chat_invite_link(Some(
                    &serde_json::from_str(&link_json("https://t.me/+x", true, false)).unwrap(),
                ))
                .unwrap(),
            ],
        }),
    );
    let all = rig
        .driver
        .delete_all_revoked_chat_invite_links(ChatId(13))
        .unwrap()
        .unwrap();
    let sent = rig.last_sent("deleteAllRevokedChatInviteLinks");
    assert_eq!(
        sent["creator_user_id"],
        rig.driver.session.my_user_id.unwrap_or(0)
    );
    rig.ingest(&format!(r#"{{"@type":"ok","@extra":"{}"}}"#, all.0));
    let Some(InviteLinkFetch::Loaded(gone)) =
        rig.driver.session.groups.revoked_invite_links.get(&13)
    else {
        panic!("revoked loaded");
    };
    assert!(gone.links.is_empty());
}

#[test]
fn subscription_links_are_for_channel_admins_and_use_the_monthly_period() {
    let mut group = Rig::new(false, ADMIN);
    assert!(
        group
            .driver
            .create_chat_subscription_invite_link(ChatId(13), "VIP", 100)
            .unwrap()
            .is_none()
    );
    let mut channel = Rig::new(true, ADMIN);
    assert!(
        channel
            .driver
            .create_chat_subscription_invite_link(ChatId(13), "VIP", 0)
            .unwrap()
            .is_none()
    );
    let extra = channel
        .driver
        .create_chat_subscription_invite_link(ChatId(13), "VIP", 250)
        .unwrap()
        .unwrap();
    let sent = channel.last_sent("createChatSubscriptionInviteLink");
    assert_eq!(sent["subscription_pricing"]["star_count"], 250);
    assert_eq!(sent["subscription_pricing"]["period"], 2_592_000);
    channel.ingest(&format!(
        r#"{{"@type":"chatInviteLink","@extra":"{}","invite_link":"https://t.me/+vip","name":"VIP","creator_user_id":777,"date":1,"edit_date":0,"expiration_date":0,"subscription_pricing":{{"@type":"starSubscriptionPricing","period":2592000,"star_count":250}},"member_limit":0,"member_count":0,"expired_member_count":0,"pending_join_request_count":0,"creates_join_request":false,"is_primary":false,"is_revoked":false}}"#,
        extra.0
    ));
    let Some(InviteLinkFetch::Loaded(list)) = channel.driver.session.groups.invite_links.get(&13)
    else {
        panic!("loaded");
    };
    assert_eq!(
        list.links[0]
            .subscription_pricing
            .as_ref()
            .map(|p| p.star_count),
        Some(250)
    );
    channel
        .driver
        .edit_chat_subscription_invite_link(ChatId(13), "https://t.me/+vip", "Gold")
        .unwrap()
        .unwrap();
    assert_eq!(
        channel.last_sent("editChatSubscriptionInviteLink")["name"],
        "Gold"
    );
}
