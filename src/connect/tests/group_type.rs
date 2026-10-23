//! Connect-driver tests: the Type box's link check, the public links
//! limit and freeing a link, and the save of the box's choice.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::group_info_edit::GroupInfoEdit;
use crate::group_type::{GroupTypeData, GroupTypeEdit, plan_group_type_save};
use crate::ids::{ChatId, RequestId};
use crate::platform::MemorySecretStore;
use crate::premium_limits::PremiumLimitKind;
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::UsernameCheckResult;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const CREATOR: &str = r#"{"@type":"chatMemberStatusCreator","is_member":true}"#;
const USERNAMES: &str = r#","usernames":{"@type":"usernames","active_usernames":["rustaceans","rust_club"],"disabled_usernames":["old_rust"],"editable_username":"rustaceans","collectible_usernames":[]}"#;

struct Rig {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    seq: AtomicU64,
    sink: Arc<dyn DiagnosticSink>,
    _dir: std::path::PathBuf,
}

impl Rig {
    /// A public supergroup (13) and a basic group (30) the viewer owns.
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
        rig.driver.session.my_user_id = Some(1);
        rig.ingest(
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"C","type":{"@type":"chatTypeSupergroup","supergroup_id":113,"is_channel":false},"unread_count":0}}"#,
        );
        rig.ingest(&format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":113,"status":{CREATOR}{USERNAMES}}}}}"#
        ));
        rig.ingest(
            r#"{"@type":"updateNewChat","chat":{"id":30,"title":"Basic","type":{"@type":"chatTypeBasicGroup","basic_group_id":7},"unread_count":0}}"#,
        );
        rig.ingest(&format!(
            r#"{{"@type":"updateBasicGroup","basic_group":{{"@type":"basicGroup","id":7,"member_count":5,"status":{CREATOR},"is_active":true}}}}"#
        ));
        rig
    }

    fn ingest(&mut self, json: &str) {
        self.driver
            .ingest(copy_and_parse(json, &self.seq, &self.sink).unwrap())
            .unwrap();
    }

    fn sent(&self, ty: &str) -> Vec<Value> {
        self.recorder
            .snapshot()
            .into_iter()
            .filter(|j| j.contains(&format!(r#""@type":"{ty}""#)))
            .map(|j| serde_json::from_str(&j).unwrap())
            .collect()
    }

    fn answer(&mut self, extra: RequestId, ty: &str) {
        self.ingest(&format!(r#"{{"@type":"{ty}","@extra":"{}"}}"#, extra.0));
    }
}

#[test]
fn only_the_newest_check_counts() {
    let mut rig = Rig::new();
    let old = rig
        .driver
        .check_group_username(ChatId(13), "quillchat")
        .unwrap()
        .unwrap();
    let new = rig
        .driver
        .check_group_username(ChatId(13), "quillchats")
        .unwrap()
        .unwrap();
    let sent = rig.sent("checkChatUsername");
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0]["chat_id"], 13);
    rig.answer(old, "checkChatUsernameResultOk");
    assert_eq!(rig.driver.session.groups.group_type.check, None);
    rig.answer(new, "checkChatUsernameResultUsernameOccupied");
    assert_eq!(
        rig.driver
            .session
            .group_username_verdict(ChatId(13), "quillchats"),
        Some(UsernameCheckResult::Occupied)
    );
    assert_eq!(
        rig.driver
            .session
            .group_username_verdict(ChatId(13), "quillchat"),
        None
    );
}

#[test]
fn a_basic_group_is_checked_as_a_new_chat() {
    let mut rig = Rig::new();
    rig.driver
        .check_group_username(ChatId(30), "bookclub")
        .unwrap();
    assert_eq!(rig.sent("checkChatUsername")[0]["chat_id"], 0);
}

#[test]
fn too_many_public_chats_opens_the_limit_box() {
    let mut rig = Rig::new();
    let check = rig
        .driver
        .check_group_username(ChatId(30), "bookclub")
        .unwrap()
        .unwrap();
    rig.answer(check, "checkChatUsernameResultPublicChatsTooMany");
    let state = &rig.driver.session.groups.group_type;
    assert_eq!(state.public_limit_reached, Some(true));
    assert_eq!(
        rig.driver.session.payments.hub.limit_box,
        Some((PremiumLimitKind::PublicLinks, 0))
    );
}

#[test]
fn revoking_a_link_frees_the_limit() {
    let mut rig = Rig::new();
    rig.driver.session.groups.group_type.public_limit_reached = Some(true);
    rig.driver.session.payments.hub.limit_box = Some((PremiumLimitKind::PublicLinks, 0));
    rig.driver.revoke_public_link(ChatId(13)).unwrap().unwrap();
    let clear = rig.sent("setSupergroupUsername");
    let disable = rig.sent("disableAllSupergroupUsernames");
    assert_eq!(clear[0]["username"], "");
    assert_eq!(disable[0]["supergroup_id"], 113);
    let requests = rig
        .driver
        .session
        .groups
        .group_type
        .revoke
        .clone()
        .unwrap()
        .requests;
    for id in requests {
        rig.answer(id, "ok");
    }
    let state = &rig.driver.session.groups.group_type;
    assert_eq!(state.revoke, None);
    assert_eq!(state.public_limit_reached, Some(false));
    assert_eq!(rig.driver.session.payments.hub.limit_box, None);
}

#[test]
fn going_private_turns_every_link_off() {
    let mut rig = Rig::new();
    let current = rig.driver.session.group_type_current(ChatId(13));
    assert!(current.public);
    assert_eq!(current.username, "rustaceans");
    let wanted = GroupTypeData {
        public: false,
        ..current.clone()
    };
    let edits = plan_group_type_save(&current, &wanted, false);
    let sent = rig.driver.save_group_type(ChatId(13), &edits, &[]).unwrap();
    assert_eq!(sent.len(), 2);
    assert_eq!(rig.sent("disableAllSupergroupUsernames").len(), 1);
    assert_eq!(rig.sent("setSupergroupUsername")[0]["username"], "");
}

#[test]
fn join_approval_carries_the_invite_links_answer() {
    let mut rig = Rig::new();
    let edits = [GroupTypeEdit::JoinByRequest {
        enabled: true,
        apply_to_invite_links: true,
    }];
    rig.driver.save_group_type(ChatId(13), &edits, &[]).unwrap();
    let sent = rig.sent("toggleSupergroupJoinByRequest");
    assert_eq!(sent[0]["join_by_request"], true);
    assert_eq!(sent[0]["apply_to_invite_links"], true);
}

#[test]
fn a_basic_group_upgrades_before_its_link_is_set() {
    let mut rig = Rig::new();
    let current = rig.driver.session.group_type_current(ChatId(30));
    let wanted = GroupTypeData {
        public: true,
        username: "bookclub".into(),
        ..current.clone()
    };
    let edits = plan_group_type_save(&current, &wanted, true);
    assert_eq!(edits.first(), Some(&GroupTypeEdit::Upgrade));
    let info = [GroupInfoEdit::Title("Book club".into())];
    let sent = rig
        .driver
        .save_group_type(ChatId(30), &edits, &info)
        .unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(rig.sent("setSupergroupUsername").len(), 0);
    assert_eq!(rig.sent("setChatTitle").len(), 0);
    rig.ingest(&format!(
        r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":777,"member_count":5,"status":{CREATOR}}}}}"#
    ));
    rig.ingest(&format!(
        r#"{{"@type":"chat","@extra":"{}","id":-100777,"title":"Basic","type":{{"@type":"chatTypeSupergroup","supergroup_id":777,"is_channel":false}},"unread_count":0}}"#,
        sent[0].0
    ));
    let username = rig.sent("setSupergroupUsername");
    assert_eq!(username[0]["supergroup_id"], 777);
    assert_eq!(username[0]["username"], "bookclub");
    assert_eq!(rig.sent("setChatTitle")[0]["chat_id"], -100777);
}
