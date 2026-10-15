//! Buying Premium in the app (tdesktop `StartPremiumPayment`): the bot and
//! the invoice slug come from the app config; the bot gets `/start <ref>`
//! at once, or the invoice link's payment form is asked for. Recorded JSON
//! only, nothing is paid.
use super::super::*;
use super::support::{prepared_tmp, ready_driver, sent_request};
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::ChatId;
use crate::payment_invoice::InputInvoice;
use crate::platform::MemorySecretStore;
use crate::state::DeepLinkAction;
use crate::telegram::client::copy_and_parse;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

#[test]
fn the_app_config_names_the_bot_and_the_start_carries_the_ref() {
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);

    driver.ensure_earn_app_config().unwrap();
    let request = sent_request(&recorder, "getApplicationConfig");
    let config = json!({
        "@type": "jsonValueObject", "@extra": request["@extra"],
        "members": [
            {"key": "premium_bot_username",
             "value": {"@type": "jsonValueString", "value": "PremiumBot"}},
            {"key": "premium_invoice_slug",
             "value": {"@type": "jsonValueString", "value": "slug_1"}},
        ]
    })
    .to_string();
    driver
        .ingest(copy_and_parse(&config, &seq, &sink).unwrap())
        .unwrap();
    let purchase = driver.session.payments.hub.purchase.clone();
    assert_eq!(purchase.bot_username, "PremiumBot");
    assert_eq!(purchase.invoice_slug, "slug_1");

    // The bot's username is resolved, then its chat gets `/start stories`.
    driver
        .resolve_deep_link(DeepLinkAction::OpenUsername {
            domain: purchase.bot_username.clone(),
            start_param: Some("stories".into()),
            post: None,
            story_id: None,
        })
        .unwrap();
    assert_eq!(
        sent_request(&recorder, "searchPublicChat")["username"],
        "PremiumBot"
    );
    driver
        .send_bot_start_message(ChatId(77), 77, "stories")
        .unwrap();
    let start = sent_request(&recorder, "sendBotStartMessage");
    assert_eq!(start["bot_user_id"], 77);
    assert_eq!(start["chat_id"], 77);
    assert_eq!(start["parameter"], "stories");
}

#[test]
fn without_a_bot_the_invoice_link_asks_for_the_payment_form() {
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    driver
        .send_payment_form_request(InputInvoice::Name("slug_1".into()))
        .unwrap();
    assert_eq!(
        sent_request(&recorder, "getPaymentForm")["input_invoice"],
        json!({"@type": "inputInvoiceName", "name": "slug_1"})
    );
}

#[test]
fn a_collectible_lookup_sends_the_item_type_and_keeps_the_answer_or_the_failure() {
    use crate::collectible_info::CollectibleLookup;
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);

    driver
        .request_collectible_info(CollectibleLookup::new(
            "durov",
            "Pavel",
            Some(ChatId(5)),
            "https://t.me/durov".into(),
        ))
        .unwrap();
    let request = sent_request(&recorder, "getCollectibleItemInfo");
    assert_eq!(
        request["type"],
        json!({"@type": "collectibleItemTypeUsername", "username": "durov"})
    );
    let answer = json!({
        "@type": "collectibleItemInfo", "@extra": request["@extra"],
        "purchase_date": 1_700_000_000, "currency": "USD", "amount": 1250,
        "cryptocurrency": "TON", "cryptocurrency_amount": 5_500_000_000_i64,
        "url": "https://fragment.com/username/durov",
    })
    .to_string();
    driver
        .ingest(copy_and_parse(&answer, &seq, &sink).unwrap())
        .unwrap();
    let lookup = driver.session.payments.hub.collectible.clone().unwrap();
    assert!(!lookup.loading);
    assert_eq!(
        lookup.info.unwrap().url,
        "https://fragment.com/username/durov"
    );

    // A lookup TDLib refuses keeps the reason for the UI.
    driver
        .request_collectible_info(CollectibleLookup::new("someone", "S", None, String::new()))
        .unwrap();
    let request = sent_request(&recorder, "getCollectibleItemInfo");
    let error = json!({
        "@type": "error", "@extra": request["@extra"],
        "code": 400, "message": "COLLECTIBLE_NOT_FOUND",
    })
    .to_string();
    driver
        .ingest(copy_and_parse(&error, &seq, &sink).unwrap())
        .unwrap();
    let lookup = driver.session.payments.hub.collectible.clone().unwrap();
    assert_eq!(lookup.failed.as_deref(), Some("COLLECTIBLE_NOT_FOUND"));
}
