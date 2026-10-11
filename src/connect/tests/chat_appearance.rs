//! Connect-driver tests: a channel's or group's appearance requests.
use super::super::{ConnectDriver, RecordingSender};
use super::*;
use crate::chat_appearance::ApplyPlan;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::ChatId;
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn count(recorder: &RecordingSender, ty: &str) -> usize {
    recorder
        .snapshot()
        .iter()
        .filter(|j| j.contains(&format!(r#""@type":"{ty}""#)))
        .count()
}

#[test]
fn only_those_who_may_change_info_can_open_or_apply() {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let ingest = |driver: &mut ConnectDriver<Arc<RecordingSender>>, json: &str| {
        driver
            .ingest(copy_and_parse(json, &seq, &sink).unwrap())
            .unwrap();
    };
    ingest(
        &mut driver,
        r#"{"@type":"updateNewChat","chat":{"id":-20,"title":"C","type":{"@type":"chatTypeSupergroup","supergroup_id":120,"is_channel":true},"unread_count":0}}"#,
    );
    let plan = ApplyPlan {
        name: Some((9, 55)),
        profile: Some((4, 66)),
        status: Some(Some(77)),
    };
    // A plain member of the channel sends nothing.
    ingest(
        &mut driver,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":120,"is_channel":true,"status":{"@type":"chatMemberStatusMember"}}}"#,
    );
    assert!(driver.open_chat_appearance(ChatId(-20)).is_err());
    assert!(driver.apply_chat_appearance(ChatId(-20), plan).is_err());
    assert_eq!(count(&recorder, "setChatAccentColor"), 0);

    // An administrator with the change-info right: one request per part,
    // each addressed to the chat.
    ingest(
        &mut driver,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":120,"is_channel":true,"status":{"@type":"chatMemberStatusAdministrator","rights":{"@type":"chatAdministratorRights","can_change_info":true}}}}"#,
    );
    assert_eq!(driver.apply_chat_appearance(ChatId(-20), plan).unwrap(), 3);
    let accent = sent_request(&recorder, "setChatAccentColor");
    assert_eq!(accent["chat_id"], -20);
    assert_eq!(accent["accent_color_id"], 9);
    assert_eq!(accent["background_custom_emoji_id"], 55);
    let profile = sent_request(&recorder, "setChatProfileAccentColor");
    assert_eq!(profile["chat_id"], -20);
    assert_eq!(profile["profile_accent_color_id"], 4);
    assert_eq!(profile["profile_background_custom_emoji_id"], 66);
    let status = sent_request(&recorder, "setChatEmojiStatus");
    assert_eq!(status["chat_id"], -20);
    assert_eq!(status["emoji_status"]["type"]["custom_emoji_id"], "77");

    // Invalid ids are refused before anything goes out.
    let bad = ApplyPlan {
        name: Some((-1, 0)),
        profile: None,
        status: None,
    };
    assert!(driver.apply_chat_appearance(ChatId(-20), bad).is_err());
}

#[test]
fn opening_refreshes_the_level_and_loads_choices_once() {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    for json in [
        r#"{"@type":"updateNewChat","chat":{"id":-20,"title":"C","type":{"@type":"chatTypeSupergroup","supergroup_id":120,"is_channel":true},"unread_count":0}}"#,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":120,"is_channel":true,"status":{"@type":"chatMemberStatusCreator","is_member":true}}}"#,
    ] {
        driver
            .ingest(copy_and_parse(json, &seq, &sink).unwrap())
            .unwrap();
    }
    // A stale level is dropped so the box never gates on old boosts.
    driver.session.groups.chat_boost_status.insert(-20, (1, 3));
    driver.open_chat_appearance(ChatId(-20)).unwrap();
    assert!(!driver.session.groups.chat_boost_status.contains_key(&-20));
    assert_eq!(count(&recorder, "getChatBoostStatus"), 1);
    assert_eq!(count(&recorder, "getChatBoostFeatures"), 1);
    assert_eq!(count(&recorder, "getDefaultChatEmojiStatuses"), 1);
    assert_eq!(
        sent_request(&recorder, "getChatBoostFeatures")["is_channel"],
        true
    );
    assert!(driver.session.stickers.emoji.chat_status_open);
    driver.close_chat_appearance();
    assert!(!driver.session.stickers.emoji.chat_status_open);
}
