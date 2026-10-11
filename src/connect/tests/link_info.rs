//! Gift code, language pack and premium offer links: they go to TDLib, the
//! boxes only inform, and a code is applied only on the explicit call.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn feed(
    driver: &mut ConnectDriver<Arc<RecordingSender>>,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
    value: serde_json::Value,
) {
    driver
        .ingest(copy_and_parse(&value.to_string(), seq, sink).unwrap())
        .unwrap();
}

fn count_sent(recorder: &RecordingSender, ty: &str) -> usize {
    recorder
        .snapshot()
        .iter()
        .filter_map(|j| serde_json::from_str::<serde_json::Value>(j).ok())
        .filter(|v| v["@type"] == ty)
        .count()
}

#[test]
fn premium_offer_links_ask_tdlib_instead_of_searching_a_username() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    for link in [
        "tg://premium_offer",
        "https://t.me/premium_offer",
        "https://t.me/giftcode/AbC",
        "https://t.me/setlanguage/pt",
    ] {
        driver.session.chats_state.deep_link = None;
        driver.request_deep_link_info(link).unwrap().unwrap();
        assert_eq!(sent_request(&recorder, "getInternalLinkType")["link"], link);
    }
    assert_eq!(count_sent(&recorder, "searchPublicChat"), 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn gift_code_is_applied_only_by_the_explicit_call() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);

    let extra = driver.request_gift_code_info("AbC").unwrap();
    assert_eq!(
        sent_request(&recorder, "checkPremiumGiftCode")["code"],
        "AbC"
    );
    assert!(driver.session.payments.gift_code.as_ref().unwrap().loading);
    // Not known yet: apply refuses.
    assert!(driver.apply_gift_code().is_err());
    feed(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"premiumGiftCodeInfo","@extra":extra.as_extra(),
            "creator_id":{"@type":"messageSenderUser","user_id":7},
            "creation_date":1700000000,"is_from_giveaway":false,"month_count":3,
            "day_count":90,"user_id":0,"use_date":0}),
    );
    // Checking a code never applies it.
    assert_eq!(count_sent(&recorder, "applyPremiumGiftCode"), 0);
    let lookup = driver.session.payments.gift_code.as_ref().unwrap();
    assert!(!lookup.loading && lookup.info.is_some());

    let extra = driver.apply_gift_code().unwrap();
    assert_eq!(count_sent(&recorder, "applyPremiumGiftCode"), 1);
    assert!(driver.session.payments.gift_code.as_ref().unwrap().applying);
    // A second click while applying sends nothing.
    assert!(driver.apply_gift_code().is_err());
    feed(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"ok","@extra":extra.as_extra()}),
    );
    let lookup = driver.session.payments.gift_code.as_ref().unwrap();
    assert!(lookup.applied && !lookup.applying);
    assert!(driver.apply_gift_code().is_err());
    assert_eq!(count_sent(&recorder, "applyPremiumGiftCode"), 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn used_or_invalid_gift_codes_cannot_be_applied() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);

    let extra = driver.request_gift_code_info("used").unwrap();
    feed(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"premiumGiftCodeInfo","@extra":extra.as_extra(),
            "month_count":1,"day_count":30,"use_date":1700000500}),
    );
    assert!(driver.apply_gift_code().is_err());

    let extra = driver.request_gift_code_info("bad").unwrap();
    feed(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"error","@extra":extra.as_extra(),"code":400,"message":"PREMIUM_GIFT_CODE_INVALID"}),
    );
    let lookup = driver.session.payments.gift_code.as_ref().unwrap();
    assert!(!lookup.loading && lookup.info.is_none() && lookup.error.is_some());
    assert!(driver.apply_gift_code().is_err());
    assert_eq!(count_sent(&recorder, "applyPremiumGiftCode"), 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_failed_apply_keeps_the_box_open_with_the_reason() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let extra = driver.request_gift_code_info("AbC").unwrap();
    feed(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"premiumGiftCodeInfo","@extra":extra.as_extra(),"month_count":3,"day_count":90}),
    );
    let extra = driver.apply_gift_code().unwrap();
    feed(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"error","@extra":extra.as_extra(),"code":400,"message":"USER_ALREADY_PREMIUM"}),
    );
    let lookup = driver.session.payments.gift_code.as_ref().unwrap();
    assert!(!lookup.applying && !lookup.applied && lookup.error.is_some());
    assert!(lookup.info.is_some());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn language_pack_link_only_reads_the_pack_info() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let extra = driver.request_language_pack_info("pt").unwrap();
    assert_eq!(
        sent_request(&recorder, "getLanguagePackInfo")["language_pack_id"],
        "pt"
    );
    feed(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"languagePackInfo","@extra":extra.as_extra(),"id":"pt",
            "name":"Portuguese","native_name":"Português","is_official":false,
            "total_string_count":10,"translated_string_count":9}),
    );
    let lookup = driver.session.settings.language_link.as_ref().unwrap();
    assert!(!lookup.loading);
    assert_eq!(lookup.info.as_ref().unwrap().native_name, "Português");
    for forbidden in [
        "setOption",
        "setCustomLanguagePack",
        "synchronizeLanguagePack",
    ] {
        assert_eq!(count_sent(&recorder, forbidden), 0, "{forbidden}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}
