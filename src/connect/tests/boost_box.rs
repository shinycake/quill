//! Connect-driver tests: the boost box (`parity:deeplink-boost`). Nothing is
//! boosted before the Boost click, and moving a boost from another chat takes
//! a second click.
use super::super::*;
use super::*;
use crate::boost_box::{BoostNotice, BoostPhase};
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{ChatId, RequestId};
use crate::platform::MemorySecretStore;
use crate::state::RequestPurpose;
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

struct Rig {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    seq: AtomicU64,
    sink: Arc<dyn DiagnosticSink>,
    _dir: std::path::PathBuf,
}

impl Rig {
    fn new(premium: bool) -> Self {
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
        rig.driver.session.payments.premium_option = Some(premium);
        rig.ingest(
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"News","type":{"@type":"chatTypeSupergroup","supergroup_id":113,"is_channel":true},"unread_count":0}}"#,
        );
        for (id, title, channel) in [(20, "Old channel", true), (21, "Old group", false)] {
            rig.ingest(&format!(
                r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{{"@type":"chatTypeSupergroup","supergroup_id":{id},"is_channel":{channel}}},"unread_count":0}}}}"#
            ));
        }
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

    fn phase(&self) -> BoostPhase {
        self.driver
            .session
            .groups
            .boost_box
            .as_ref()
            .expect("box open")
            .phase
            .clone()
    }

    /// Answer the pending `getAvailableChatBoostSlots`.
    fn answer_slots(&mut self, slots: &str) {
        let extra = self
            .driver
            .session
            .requests
            .pending_extra_for(RequestPurpose::GetBoostSlotsForBoost, Some(ChatId(13)))
            .expect("slots request pending");
        self.ingest(&format!(
            r#"{{"@type":"chatBoostSlots","@extra":"{}","slots":{slots}}}"#,
            extra.0
        ));
    }

    fn boost_extra(&self) -> RequestId {
        self.driver
            .session
            .requests
            .pending_extra_for(RequestPurpose::BoostChat, Some(ChatId(13)))
            .expect("boostChat pending")
    }
}

fn slot(id: i32, chat: i64, cooldown: i64) -> String {
    format!(
        r#"{{"slot_id":{id},"currently_boosted_chat_id":{chat},"start_date":1,"expiration_date":4000000000,"cooldown_until_date":{cooldown}}}"#
    )
}

fn now() -> i64 {
    crate::local_time::now_unix()
}

#[test]
fn opening_the_box_reads_the_level_and_boosts_nothing() {
    let mut rig = Rig::new(true);
    assert!(rig.driver.open_boost_box(ChatId(13)).unwrap());
    assert_eq!(rig.sent_all("getChatBoostStatus").len(), 1);
    assert_eq!(rig.sent_all("getChatBoostFeatures").len(), 1);
    assert!(rig.sent_all("getAvailableChatBoostSlots").is_empty());
    assert!(rig.sent_all("boostChat").is_empty());
    assert_eq!(rig.phase(), BoostPhase::Main);
    let state = rig.driver.session.groups.boost_box.as_ref().unwrap();
    assert!(!state.group, "a channel");
    // A private chat has nothing to boost.
    assert!(!rig.driver.open_boost_box(ChatId(999)).unwrap());
}

#[test]
fn the_status_answer_gives_the_viewers_own_boosts() {
    let mut rig = Rig::new(true);
    rig.driver.open_boost_box(ChatId(13)).unwrap();
    let extra = rig
        .driver
        .session
        .requests
        .pending_extra_for(RequestPurpose::GetChatBoostStatus, Some(ChatId(13)))
        .unwrap();
    rig.ingest(&format!(
        r#"{{"@type":"chatBoostStatus","@extra":"{}","level":1,"boost_count":3,"current_level_boost_count":2,"next_level_boost_count":5,"applied_slot_ids":[7]}}"#,
        extra.0
    ));
    let details = &rig.driver.session.groups.chat_boost_details[&13];
    assert_eq!(details.my_boost_count, 1);
}

#[test]
fn a_free_slot_boosts_with_that_slot() {
    let mut rig = Rig::new(true);
    rig.driver.open_boost_box(ChatId(13)).unwrap();
    rig.driver.request_chat_boost(ChatId(13)).unwrap();
    assert_eq!(rig.phase(), BoostPhase::Working);
    assert!(rig.sent_all("boostChat").is_empty(), "slots first");
    rig.answer_slots(&format!("[{},{}]", slot(4, 20, 0), slot(5, 0, 0)));
    let sent = rig.sent_all("boostChat");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["chat_id"], 13);
    assert_eq!(sent[0]["slot_ids"], serde_json::json!([5]));
    // The answer returns the box to its counters.
    let extra = rig.boost_extra();
    rig.ingest(&format!(
        r#"{{"@type":"chatBoostSlots","@extra":"{}","slots":[]}}"#,
        extra.0
    ));
    assert_eq!(rig.phase(), BoostPhase::Main);
}

#[test]
fn other_slots_open_the_reassign_list_and_wait_for_a_second_click() {
    let mut rig = Rig::new(true);
    rig.driver.open_boost_box(ChatId(13)).unwrap();
    rig.driver.request_chat_boost(ChatId(13)).unwrap();
    rig.answer_slots(&format!(
        "[{},{},{}]",
        slot(4, 20, 0),
        slot(5, 21, 0),
        slot(6, 20, now() + 3_600)
    ));
    assert!(rig.sent_all("boostChat").is_empty(), "no boost yet");
    let BoostPhase::Reassign { slots, selected } = rig.phase() else {
        panic!("reassign list");
    };
    assert_eq!(slots.len(), 3);
    assert!(selected.is_empty());
    // Nothing picked: Reassign does nothing.
    assert!(!rig.driver.reassign_boosts().unwrap());
    assert!(rig.sent_all("boostChat").is_empty());
    // A row that is cooling down cannot be picked.
    let state = rig.driver.session.groups.boost_box.as_mut().unwrap();
    state.toggle_slot(6, now());
    assert!(state.reassign_selection(now()).is_empty());
    state.toggle_slot(4, now());
    state.toggle_slot(5, now());
    assert!(rig.driver.reassign_boosts().unwrap());
    let sent = rig.sent_all("boostChat");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["slot_ids"], serde_json::json!([4, 5]));
    assert_eq!(rig.phase(), BoostPhase::Working);
    // The answer names where the boosts came from.
    let extra = rig.boost_extra();
    rig.ingest(&format!(
        r#"{{"@type":"chatBoostSlots","@extra":"{}","slots":[]}}"#,
        extra.0
    ));
    let state = rig.driver.session.groups.boost_box.as_ref().unwrap();
    assert_eq!(state.phase, BoostPhase::Main);
    assert_eq!(
        state.note.as_deref(),
        Some("2 boosts are reassigned from 2 groups and channels.")
    );
}

#[test]
fn a_lone_slot_asks_before_replacing() {
    let mut rig = Rig::new(true);
    rig.driver.open_boost_box(ChatId(13)).unwrap();
    rig.driver.request_chat_boost(ChatId(13)).unwrap();
    rig.answer_slots(&format!("[{}]", slot(4, 20, 0)));
    assert!(rig.sent_all("boostChat").is_empty());
    assert!(rig.driver.reassign_boosts().unwrap());
    assert_eq!(
        rig.sent_all("boostChat")[0]["slot_ids"],
        serde_json::json!([4])
    );
}

#[test]
fn a_lone_slot_in_cooldown_says_to_wait() {
    let mut rig = Rig::new(true);
    rig.driver.open_boost_box(ChatId(13)).unwrap();
    rig.driver.request_chat_boost(ChatId(13)).unwrap();
    rig.answer_slots(&format!("[{}]", slot(4, 21, now() + 7_300)));
    assert!(matches!(
        rig.phase(),
        BoostPhase::Notice(BoostNotice::Flood { group: true, .. })
    ));
    assert!(rig.sent_all("boostChat").is_empty());
    assert!(!rig.driver.reassign_boosts().unwrap());
}

#[test]
fn no_free_slot_picks_the_notice_by_who_the_viewer_is() {
    let mut rig = Rig::new(true);
    rig.driver.open_boost_box(ChatId(13)).unwrap();
    rig.driver.request_chat_boost(ChatId(13)).unwrap();
    rig.answer_slots(&format!("[{}]", slot(4, 13, 0)));
    assert_eq!(rig.phase(), BoostPhase::Notice(BoostNotice::Already));

    let mut rig = Rig::new(false);
    rig.driver.open_boost_box(ChatId(13)).unwrap();
    rig.driver.request_chat_boost(ChatId(13)).unwrap();
    rig.answer_slots("[]");
    assert_eq!(rig.phase(), BoostPhase::Notice(BoostNotice::PremiumNeeded));

    let mut rig = Rig::new(true);
    rig.driver.open_boost_box(ChatId(13)).unwrap();
    rig.driver.request_chat_boost(ChatId(13)).unwrap();
    rig.answer_slots("[]");
    assert_eq!(rig.phase(), BoostPhase::Notice(BoostNotice::GiftedNoBoosts));
    assert!(rig.sent_all("boostChat").is_empty());
}

#[test]
fn a_refused_boost_returns_to_the_counters_with_the_reason() {
    let mut rig = Rig::new(true);
    rig.driver.open_boost_box(ChatId(13)).unwrap();
    rig.driver.request_chat_boost(ChatId(13)).unwrap();
    rig.answer_slots(&format!("[{}]", slot(5, 0, 0)));
    let extra = rig.boost_extra();
    rig.ingest(&format!(
        r#"{{"@type":"error","code":400,"message":"BOOST_NOT_MODIFIED","@extra":"{}"}}"#,
        extra.0
    ));
    let state = rig.driver.session.groups.boost_box.as_ref().unwrap();
    assert_eq!(state.phase, BoostPhase::Main);
    assert_eq!(state.note.as_deref(), Some("Could not boost (error 400)"));
}

#[test]
fn closing_the_box_drops_a_pending_boost_intent() {
    let mut rig = Rig::new(true);
    rig.driver.open_boost_box(ChatId(13)).unwrap();
    rig.driver.request_chat_boost(ChatId(13)).unwrap();
    rig.driver.close_boost_box();
    assert!(rig.driver.session.groups.boost_box.is_none());
    // The late slots answer finds no intent and no box: nothing is sent.
    rig.answer_slots(&format!("[{}]", slot(5, 0, 0)));
    assert!(rig.sent_all("boostChat").is_empty());
}
