//! Connect-driver tests: saving the group permissions box sends only what
//! changed, and a basic group is upgraded before slow mode is set.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::ChatId;
use crate::permissions_box::{Permission, PermissionsSave, toggle};
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::ChatPermissions;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const CREATOR: &str = r#"{"@type":"chatMemberStatusCreator","is_member":true}"#;

struct Rig {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    seq: AtomicU64,
    sink: Arc<dyn DiagnosticSink>,
    dir: std::path::PathBuf,
}

impl Rig {
    fn new() -> Self {
        let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let recorder = Arc::new(RecordingSender::new());
        let seq = AtomicU64::new(0);
        let driver = ready_driver(&recorder, prepared, &sink, &seq);
        Self {
            driver,
            recorder,
            seq,
            sink,
            dir,
        }
    }

    fn ingest(&mut self, json: &str) {
        self.driver
            .ingest(copy_and_parse(json, &self.seq, &self.sink).unwrap())
            .unwrap();
    }

    fn sent(&self, ty: &str) -> Vec<Value> {
        self.recorder
            .snapshot()
            .iter()
            .filter_map(|json| serde_json::from_str::<Value>(json).ok())
            .filter(|value| value["@type"] == ty)
            .collect()
    }

    /// Supergroup 13 (id 113) owned by the viewer: photos off, slow mode
    /// off, boosters restricted.
    fn owned_supergroup(&mut self) {
        self.ingest(
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"G","type":{"@type":"chatTypeSupergroup","supergroup_id":113,"is_channel":false},"permissions":{"@type":"chatPermissions","can_send_basic_messages":true,"can_send_photos":false},"unread_count":0}}"#,
        );
        self.ingest(&format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":113,"member_count":500,"status":{CREATOR}}}}}"#
        ));
        self.ingest(
            r#"{"@type":"updateSupergroupFullInfo","supergroup_id":113,"supergroup_full_info":{"description":"","member_count":500,"slow_mode_delay":0,"unrestrict_boost_count":0,"restricted_count":2,"banned_count":1}}"#,
        );
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn current(rig: &Rig) -> ChatPermissions {
    rig.driver
        .session
        .chats
        .get(&13)
        .and_then(|chat| chat.permissions)
        .unwrap_or_default()
}

#[test]
fn an_unchanged_box_sends_nothing() {
    let mut rig = Rig::new();
    rig.owned_supergroup();
    let save = PermissionsSave::new(current(&rig), 0, 0);
    assert!(matches!(
        rig.driver.save_permissions_box(ChatId(13), save),
        Ok(false)
    ));
    assert!(rig.sent("setChatPermissions").is_empty());
    assert!(rig.sent("setChatSlowModeDelay").is_empty());
    assert!(rig.sent("setSupergroupUnrestrictBoostCount").is_empty());
}

#[test]
fn a_save_sends_each_changed_setting_once() {
    let mut rig = Rig::new();
    rig.owned_supergroup();
    let counts = rig.driver.session.chat_member_counts(ChatId(13));
    assert_eq!((counts.restricted, counts.banned), (2, 1));
    let permissions = toggle(current(&rig), Permission::SendText, false);
    let save = PermissionsSave::new(permissions, 30, 2);
    assert!(matches!(
        rig.driver.save_permissions_box(ChatId(13), save),
        Ok(true)
    ));
    let sent = rig.sent("setChatPermissions");
    let [request] = sent.as_slice() else {
        panic!("one setChatPermissions: {sent:?}");
    };
    assert_eq!(request["chat_id"], 13);
    assert_eq!(request["permissions"]["can_send_basic_messages"], false);
    assert_eq!(request["permissions"]["can_add_link_previews"], false);
    assert_eq!(request["permissions"]["can_send_photos"], false);
    let slow = rig.sent("setChatSlowModeDelay");
    assert_eq!(slow.len(), 1);
    assert_eq!(
        slow.first().map(|v| v["slow_mode_delay"].clone()),
        Some(30.into())
    );
    let boosts = rig.sent("setSupergroupUnrestrictBoostCount");
    assert_eq!(boosts.len(), 1);
    assert_eq!(
        boosts.first().map(|v| v["unrestrict_boost_count"].clone()),
        Some(2.into())
    );
}

#[test]
fn a_basic_group_is_upgraded_before_slow_mode() {
    let mut rig = Rig::new();
    rig.ingest(
        r#"{"@type":"updateNewChat","chat":{"id":30,"title":"Basic","type":{"@type":"chatTypeBasicGroup","basic_group_id":7},"unread_count":0}}"#,
    );
    rig.ingest(&format!(
        r#"{{"@type":"updateBasicGroup","basic_group":{{"@type":"basicGroup","id":7,"member_count":20,"status":{CREATOR},"is_active":true}}}}"#
    ));
    let save = PermissionsSave::new(ChatPermissions::all(), 60, 0);
    assert!(matches!(
        rig.driver.save_permissions_box(ChatId(30), save),
        Ok(true)
    ));
    let upgrades = rig.sent("upgradeBasicGroupChatToSupergroupChat");
    assert_eq!(upgrades.len(), 1);
    assert!(rig.sent("setChatSlowModeDelay").is_empty());
    let extra = upgrades
        .first()
        .map(|v| match &v["@extra"] {
            Value::String(extra) => extra.clone(),
            other => other.to_string(),
        })
        .unwrap_or_default();
    rig.ingest(&format!(
        r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":777,"member_count":20,"status":{CREATOR}}}}}"#
    ));
    rig.ingest(&format!(
        r#"{{"@type":"chat","@extra":"{extra}","id":-100777,"title":"Basic","type":{{"@type":"chatTypeSupergroup","supergroup_id":777,"is_channel":false}},"unread_count":0}}"#
    ));
    let slow = rig.sent("setChatSlowModeDelay");
    assert_eq!(slow.len(), 1, "slow mode set on the new supergroup");
    assert_eq!(
        slow.first().map(|v| v["chat_id"].clone()),
        Some((-100777).into())
    );
    assert_eq!(
        slow.first().map(|v| v["slow_mode_delay"].clone()),
        Some(60.into())
    );
}

#[test]
fn delete_on_the_removed_list_makes_the_user_left_not_a_member() {
    let mut rig = Rig::new();
    rig.owned_supergroup();
    let sent = rig.driver.remove_from_banned(ChatId(13), 5);
    assert!(matches!(sent, Ok(Some(_))));
    let requests = rig.sent("setChatMemberStatus");
    let [request] = requests.as_slice() else {
        panic!("one setChatMemberStatus: {requests:?}");
    };
    assert_eq!(request["member_id"]["user_id"], 5);
    assert_eq!(request["status"]["@type"], "chatMemberStatusLeft");
}

/// Round 2, 167: a save while an earlier setChatPermissions is out is sent
/// too, and if both are refused the box shows the permissions from before
/// the first, not the ones the first showed.
#[test]
fn a_second_save_while_one_is_out_is_sent_and_rolls_back_to_the_original() {
    let mut rig = Rig::new();
    rig.owned_supergroup();
    let original = current(&rig);
    let first = toggle(original, Permission::SendText, false);
    let second = toggle(first, Permission::Videos, false);
    let a = rig.driver.set_chat_permissions(ChatId(13), &first).unwrap();
    let b = rig
        .driver
        .set_chat_permissions(ChatId(13), &second)
        .unwrap();
    assert!(a.is_some() && b.is_some(), "the second is not dropped");
    assert_eq!(rig.sent("setChatPermissions").len(), 2);
    assert_eq!(current(&rig), second);
    for extra in [a, b].into_iter().flatten() {
        rig.ingest(&format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"X"}}"#,
            extra.0
        ));
    }
    assert_eq!(current(&rig), original);
}

/// Round 2, 167: the first save is accepted (TDLib sends
/// `updateChatPermissions` for it) and the second refused: the box shows
/// the first's permissions, which the server has, not the original.
#[test]
fn a_refused_second_save_restores_what_the_accepted_first_saved() {
    let mut rig = Rig::new();
    rig.owned_supergroup();
    let original = current(&rig);
    let first = toggle(original, Permission::SendText, false);
    let second = toggle(first, Permission::Videos, false);
    let a = rig.driver.set_chat_permissions(ChatId(13), &first).unwrap();
    let b = rig
        .driver
        .set_chat_permissions(ChatId(13), &second)
        .unwrap();
    let (Some(a), Some(b)) = (a, b) else {
        panic!("both saves are sent");
    };
    rig.ingest(&format!(r#"{{"@type":"ok","@extra":"{}"}}"#, a.0));
    rig.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"X"}}"#,
        b.0
    ));
    assert_eq!(current(&rig), first);
}

/// The first refused while the second is out: the second decides.
#[test]
fn an_accepted_second_save_stands_after_a_refused_first() {
    let mut rig = Rig::new();
    rig.owned_supergroup();
    let original = current(&rig);
    let first = toggle(original, Permission::SendText, false);
    let second = toggle(first, Permission::Videos, false);
    let a = rig.driver.set_chat_permissions(ChatId(13), &first).unwrap();
    let b = rig
        .driver
        .set_chat_permissions(ChatId(13), &second)
        .unwrap();
    let (Some(a), Some(b)) = (a, b) else {
        panic!("both saves are sent");
    };
    rig.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"X"}}"#,
        a.0
    ));
    assert_eq!(current(&rig), second, "the later save is still out");
    rig.ingest(&format!(r#"{{"@type":"ok","@extra":"{}"}}"#, b.0));
    assert_eq!(current(&rig), second);
}
