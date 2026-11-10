//! Connect-driver tests: Telegram Business links, chatbot, start page and
//! ads switch (recorded JSON, nothing live).
use super::super::*;
use super::*;
use crate::business_bots::{BotRights, ConnectedBot};
use crate::business_settings::BusinessRecipients;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
use crate::state::ChatLinkSave;
use crate::state::fetch::Fetch;
use crate::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

type Recorder = Arc<RecordingSender>;

fn driver_parts() -> (
    ConnectDriver<Recorder>,
    Recorder,
    Arc<dyn DiagnosticSink>,
    AtomicU64,
) {
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let driver = ready_driver(&recorder, prepared, &sink, &seq);
    (driver, recorder, sink, seq)
}

fn feed(
    driver: &mut ConnectDriver<Recorder>,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
    json: &str,
) {
    driver
        .ingest(copy_and_parse(json, seq, sink).unwrap())
        .unwrap();
}

fn extra_of(recorder: &Recorder, type_name: &str) -> String {
    sent_request(recorder, type_name)["@extra"]
        .as_str()
        .unwrap()
        .to_string()
}

fn count_sent(recorder: &Recorder, type_name: &str) -> usize {
    recorder
        .snapshot()
        .iter()
        .filter(|json| json.contains(&format!("\"@type\":\"{type_name}\"")))
        .count()
}

const LINK: &str = r#""link":"https://t.me/m/Ab","text":{"@type":"formattedText","text":"Hi","entities":[]},"title":"Promo","view_count":2"#;

#[test]
fn chat_links_load_create_edit_and_delete() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    driver.ensure_chat_links().unwrap();
    driver.ensure_chat_links().unwrap();
    assert_eq!(count_sent(&recorder, "getBusinessChatLinks"), 1);
    let extra = extra_of(&recorder, "getBusinessChatLinks");
    feed(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"businessChatLinks","@extra":"{extra}","links":[{{"@type":"businessChatLink",{LINK}}}]}}"#
        ),
    );
    assert_eq!(
        driver
            .session
            .settings
            .business_extra
            .chat_links
            .data()
            .unwrap()
            .len(),
        1
    );

    driver.create_chat_link("Hello", "").unwrap();
    let extra = extra_of(&recorder, "createBusinessChatLink");
    feed(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"businessChatLink","@extra":"{extra}","link":"https://t.me/m/New","text":{{"@type":"formattedText","text":"Hello","entities":[]}},"title":"","view_count":0}}"#
        ),
    );
    let links = driver
        .session
        .settings
        .business_extra
        .chat_links
        .data()
        .cloned()
        .unwrap();
    assert_eq!(links[0].link, "https://t.me/m/New");
    assert_eq!(
        driver.session.settings.business_extra.chat_link_save,
        ChatLinkSave::Saved
    );

    driver
        .edit_chat_link("https://t.me/m/Ab", "Hey", "Promo 2")
        .unwrap();
    let extra = extra_of(&recorder, "editBusinessChatLink");
    feed(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"businessChatLink","@extra":"{extra}","link":"https://t.me/m/Ab","text":{{"@type":"formattedText","text":"Hey","entities":[]}},"title":"Promo 2","view_count":2}}"#
        ),
    );
    let links = driver
        .session
        .settings
        .business_extra
        .chat_links
        .data()
        .cloned()
        .unwrap();
    assert_eq!(links.len(), 2);
    assert_eq!(links[1].title, "Promo 2");

    driver.delete_chat_link("https://t.me/m/Ab").unwrap();
    assert_eq!(
        driver
            .session
            .settings
            .business_extra
            .chat_links
            .data()
            .unwrap()
            .len(),
        1
    );
    let extra = extra_of(&recorder, "deleteBusinessChatLink");
    feed(
        &mut driver,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{extra}"}}"#),
    );
    // The row stays gone; nothing asks the list again.
    driver.ensure_chat_links().unwrap();
    assert_eq!(count_sent(&recorder, "getBusinessChatLinks"), 1);
    assert_eq!(
        driver
            .session
            .settings
            .business_extra
            .chat_links
            .data()
            .map(Vec::len),
        Some(1)
    );
}

#[test]
fn a_refused_link_delete_puts_the_row_back() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    driver.ensure_chat_links().unwrap();
    let extra = extra_of(&recorder, "getBusinessChatLinks");
    feed(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"businessChatLinks","@extra":"{extra}","links":[{{"@type":"businessChatLink",{LINK}}}]}}"#
        ),
    );
    driver.delete_chat_link("https://t.me/m/Ab").unwrap();
    let links = |driver: &ConnectDriver<_>| {
        driver
            .session
            .settings
            .business_extra
            .chat_links
            .data()
            .map_or(0, Vec::len)
    };
    assert_eq!(links(&driver), 0);
    let extra = extra_of(&recorder, "deleteBusinessChatLink");
    feed(
        &mut driver,
        &seq,
        &sink,
        &format!(r#"{{"@type":"error","@extra":"{extra}","code":400,"message":"X"}}"#),
    );
    assert_eq!(links(&driver), 1);
    assert!(driver.session.settings.business.error.is_some());
}

#[test]
fn a_failed_links_load_waits_for_retry() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    driver.ensure_chat_links().unwrap();
    let extra = extra_of(&recorder, "getBusinessChatLinks");
    feed(
        &mut driver,
        &seq,
        &sink,
        &format!(r#"{{"@type":"error","@extra":"{extra}","code":500,"message":"X"}}"#),
    );
    assert!(
        driver
            .session
            .settings
            .business_extra
            .chat_links
            .error()
            .is_some()
    );
    // Opening the screen again does not ask on its own; Retry does.
    driver.ensure_chat_links().unwrap();
    assert_eq!(count_sent(&recorder, "getBusinessChatLinks"), 1);
    driver.retry_chat_links().unwrap();
    assert_eq!(count_sent(&recorder, "getBusinessChatLinks"), 2);
    assert!(
        driver
            .session
            .settings
            .business_extra
            .chat_links
            .is_loading()
    );
}

#[test]
fn a_failed_bot_load_waits_for_retry() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    driver.ensure_connected_bot().unwrap();
    let extra = extra_of(&recorder, "getBusinessConnectedBot");
    feed(
        &mut driver,
        &seq,
        &sink,
        &format!(r#"{{"@type":"error","@extra":"{extra}","code":500,"message":"X"}}"#),
    );
    assert!(
        driver
            .session
            .settings
            .business_extra
            .connected_bot
            .error()
            .is_some()
    );
    driver.ensure_connected_bot().unwrap();
    assert_eq!(count_sent(&recorder, "getBusinessConnectedBot"), 1);
    driver.retry_connected_bot().unwrap();
    assert_eq!(count_sent(&recorder, "getBusinessConnectedBot"), 2);
}

#[test]
fn no_connected_bot_is_a_404() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    driver.ensure_connected_bot().unwrap();
    let extra = extra_of(&recorder, "getBusinessConnectedBot");
    feed(
        &mut driver,
        &seq,
        &sink,
        &format!(r#"{{"@type":"error","@extra":"{extra}","code":404,"message":"Not Found"}}"#),
    );
    assert_eq!(
        driver.session.settings.business_extra.connected_bot,
        Fetch::Ready(None)
    );
    assert!(driver.session.settings.business.error.is_none());
}

#[test]
fn a_bot_is_saved_and_asked_for_again() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    let bot = ConnectedBot {
        bot_user_id: 42,
        recipients: BusinessRecipients::everyone(),
        rights: BotRights {
            can_reply: true,
            ..BotRights::default()
        },
    };
    driver.save_connected_bot(Some(&bot), None).unwrap();
    let request = sent_request(&recorder, "setBusinessConnectedBot");
    assert_eq!(request["bot"]["rights"]["can_reply"], true);
    let extra = request["@extra"].as_str().unwrap().to_string();
    feed(
        &mut driver,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{extra}"}}"#),
    );
    driver.ensure_connected_bot().unwrap();
    assert_eq!(count_sent(&recorder, "getBusinessConnectedBot"), 1);

    driver.save_connected_bot(None, Some(42)).unwrap();
    assert_eq!(
        sent_request(&recorder, "deleteBusinessConnectedBot")["bot_user_id"],
        42
    );
    assert_eq!(
        driver.session.settings.business_extra.connected_bot,
        Fetch::Ready(None)
    );
}

/// Round 17 C finding 12: a refused chatbot save does not leave the bot as
/// connected for the next visit, which asks TDLib again.
#[test]
fn a_refused_bot_save_is_forgotten_and_asked_for_again() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    let bot = ConnectedBot {
        bot_user_id: 42,
        recipients: BusinessRecipients::everyone(),
        rights: BotRights::default(),
    };
    driver.save_connected_bot(Some(&bot), None).unwrap();
    let extra = extra_of(&recorder, "setBusinessConnectedBot");
    feed(
        &mut driver,
        &seq,
        &sink,
        &format!(r#"{{"@type":"error","@extra":"{extra}","code":400,"message":"BOT_INVALID"}}"#),
    );
    assert_eq!(
        driver.session.settings.business_extra.connected_bot,
        Fetch::Idle
    );
    driver.ensure_connected_bot().unwrap();
    assert_eq!(count_sent(&recorder, "getBusinessConnectedBot"), 1);
}

#[test]
fn the_bot_lookup_collects_three_answers() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    feed(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":42,"title":"Shop Bot","type":{"@type":"chatTypePrivate","user_id":42},"unread_count":0}}"#,
    );
    driver.lookup_business_bot("@shop_bot").unwrap();
    // The same query is not sent twice.
    driver.lookup_business_bot("shop_bot").unwrap();
    assert_eq!(count_sent(&recorder, "searchPublicChat"), 1);
    let exact = extra_of(&recorder, "searchPublicChat");
    feed(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chat","@extra":"{exact}","id":42,"title":"Shop Bot","type":{{"@type":"chatTypePrivate","user_id":42}},"unread_count":0}}"#
        ),
    );
    let mine = extra_of(&recorder, "searchChatsOnServer");
    feed(
        &mut driver,
        &seq,
        &sink,
        &format!(r#"{{"@type":"chats","@extra":"{mine}","total_count":1,"chat_ids":[42]}}"#),
    );
    let global = extra_of(&recorder, "searchPublicChats");
    feed(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{global}","code":400,"message":"QUERY_TOO_SHORT"}}"#
        ),
    );
    let lookup = &driver.session.settings.business_extra.lookup;
    assert!(lookup.done());
    assert_eq!(lookup.exact, Some(Some(42)));
    assert_eq!(lookup.mine.as_deref(), Some(&[42][..]));
    assert_eq!(lookup.global.as_deref(), Some(&[][..]));
    assert!(driver.session.settings.business.error.is_none());
    // An invalid query clears it without sending.
    driver.lookup_business_bot("@").unwrap();
    assert!(
        driver
            .session
            .settings
            .business_extra
            .lookup
            .query
            .is_empty()
    );
    assert_eq!(count_sent(&recorder, "searchPublicChat"), 1);
}

#[test]
fn start_page_and_ads_requests() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    driver.save_business_start_page("Hi", "", Some(44)).unwrap();
    let request = sent_request(&recorder, "setBusinessStartPage");
    assert_eq!(request["start_page"]["sticker"]["id"], 44);
    assert!(
        driver
            .save_business_start_page(&"x".repeat(40), "", None)
            .is_err()
    );
    feed(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateOption","name":"business_start_page_title_length_max","value":{"@type":"optionValueInteger","value":"48"}}"#,
    );
    assert!(
        driver
            .save_business_start_page(&"x".repeat(40), "", None)
            .is_ok()
    );
    driver.set_sponsored_enabled(true).unwrap();
    assert_eq!(
        sent_request(&recorder, "toggleHasSponsoredMessagesEnabled")["has_sponsored_messages_enabled"],
        true
    );
}

#[test]
fn a_refused_link_save_goes_back_to_idle_and_only_its_own_answer_counts() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    let save = |driver: &ConnectDriver<_>| driver.session.settings.business_extra.chat_link_save;
    driver.create_chat_link("Hello", "").unwrap();
    assert!(matches!(save(&driver), ChatLinkSave::Saving(_)));
    let first = extra_of(&recorder, "createBusinessChatLink");
    // The box was closed and a new one opened: the old save no longer drives it.
    driver.session.settings.business_extra.chat_link_save = ChatLinkSave::Idle;
    assert_eq!(save(&driver), ChatLinkSave::Idle);
    feed(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"businessChatLink","@extra":"{first}","link":"https://t.me/m/Old","text":{{"@type":"formattedText","text":"Hello","entities":[]}},"title":"","view_count":0}}"#
        ),
    );
    assert_eq!(save(&driver), ChatLinkSave::Idle);

    driver.create_chat_link("Again", "").unwrap();
    let second = recorder
        .snapshot()
        .iter()
        .filter_map(|j| serde_json::from_str::<serde_json::Value>(j).ok())
        .filter(|v| v["@type"] == "createBusinessChatLink")
        .filter_map(|v| v["@extra"].as_str().map(str::to_string))
        .next_back()
        .unwrap();
    feed(
        &mut driver,
        &seq,
        &sink,
        &format!(r#"{{"@type":"error","@extra":"{second}","code":400,"message":"X"}}"#),
    );
    assert_eq!(save(&driver), ChatLinkSave::Idle);
    assert!(driver.session.settings.business.error.is_some());
}
