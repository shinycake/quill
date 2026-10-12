//! Driver tests: paid messages. TDLib JSON is recorded from the 1.8.68
//! schema shapes; no Stars are ever sent.
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::ids::ChatId;
use crate::paid_messages::{CanSend, SendGate};
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

pub(super) type Fixture = (
    std::path::PathBuf,
    ConnectDriver<Arc<RecordingSender>>,
    Arc<RecordingSender>,
    Arc<dyn DiagnosticSink>,
    AtomicU64,
);

pub(super) fn fixture() -> Fixture {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let driver = ready_driver(&recorder, prepared, &sink, &seq);
    (dir, driver, recorder, sink, seq)
}

pub(super) fn cleanup(f: Fixture) {
    let (dir, driver, ..) = f;
    drop(driver);
    std::fs::remove_dir_all(dir).unwrap();
}

pub(super) fn ingest(f: &mut Fixture, json: &str) {
    let owned = copy_and_parse(json, &f.4, &f.3).unwrap();
    f.1.ingest(owned).unwrap();
}

pub(super) fn requests(f: &Fixture, type_name: &str) -> Vec<Value> {
    f.2.snapshot()
        .iter()
        .filter_map(|json| serde_json::from_str::<Value>(json).ok())
        .filter(|value| value["@type"] == type_name)
        .collect()
}

pub(super) fn extra_of(request: &Value) -> String {
    request["@extra"].as_str().unwrap().to_string()
}

/// A private chat `chat_id` with user `user_id`, open in the driver.
pub(super) fn open_private(f: &mut Fixture, chat_id: i64, user_id: i64, contact: bool, bot: bool) {
    ingest(
        f,
        &format!(
            r#"{{"@type":"updateUser","user":{{"@type":"user","id":{user_id},"first_name":"Ann","last_name":"","is_contact":{contact},"type":{{"@type":"{}"}}}}}}"#,
            if bot {
                "userTypeBot"
            } else {
                "userTypeRegular"
            }
        ),
    );
    ingest(
        f,
        &format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"Ann","type":{{"@type":"chatTypePrivate","user_id":{user_id}}},"unread_count":0}}}}"#
        ),
    );
    f.1.session.open_chat(ChatId(chat_id));
}

#[test]
fn charging_user_is_asked_once_and_gates_the_composer() {
    let mut f = fixture();
    open_private(&mut f, 500, 501, false, false);
    f.1.maybe_fetch_paid_messages_info().unwrap();
    let asks = requests(&f, "canSendMessageToUser");
    assert_eq!(asks.len(), 1);
    assert_eq!(asks[0]["user_id"], 501);
    assert_eq!(asks[0]["only_local"], false);
    assert_eq!(f.1.session.send_gate(ChatId(500)), SendGate::Free);
    // The same chat does not ask again while the first ask is out.
    f.1.maybe_fetch_paid_messages_info().unwrap();
    assert_eq!(requests(&f, "canSendMessageToUser").len(), 1);
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"canSendMessageToUserResultUserHasPaidMessages","outgoing_paid_message_star_count":25,"@extra":"{}"}}"#,
            extra_of(&asks[0])
        ),
    );
    assert_eq!(
        f.1.session.payments.paid_messages.gate_answer(501),
        Some(CanSend::HasPaidMessages { stars: 25 })
    );
    assert_eq!(
        f.1.session.send_gate(ChatId(500)),
        SendGate::Charges { stars: 25 }
    );
    // A fresh answer is trusted for a while.
    f.1.maybe_fetch_paid_messages_info().unwrap();
    assert_eq!(requests(&f, "canSendMessageToUser").len(), 1);
    cleanup(f);
}

#[test]
fn restricted_user_asks_for_premium_until_the_viewer_has_it() {
    let mut f = fixture();
    open_private(&mut f, 510, 511, false, false);
    f.1.maybe_fetch_paid_messages_info().unwrap();
    let ask = requests(&f, "canSendMessageToUser").remove(0);
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"canSendMessageToUserResultUserRestrictsNewChats","@extra":"{}"}}"#,
            extra_of(&ask)
        ),
    );
    assert_eq!(
        f.1.session.send_gate(ChatId(510)),
        SendGate::PremiumRequired
    );
    ingest(
        &mut f,
        r#"{"@type":"updateOption","name":"is_premium","value":{"@type":"optionValueBoolean","value":true}}"#,
    );
    assert_eq!(f.1.session.send_gate(ChatId(510)), SendGate::Free);
    cleanup(f);
}

#[test]
fn contacts_and_bots_are_never_asked() {
    let mut f = fixture();
    open_private(&mut f, 520, 521, true, false);
    f.1.maybe_fetch_paid_messages_info().unwrap();
    open_private(&mut f, 530, 531, false, true);
    f.1.session.bots.bot_user_ids.insert(531);
    f.1.maybe_fetch_paid_messages_info().unwrap();
    assert!(requests(&f, "canSendMessageToUser").is_empty());
    cleanup(f);
}

#[test]
fn options_and_user_prices_feed_the_state() {
    let mut f = fixture();
    open_private(&mut f, 540, 541, false, false);
    ingest(
        &mut f,
        r#"{"@type":"updateOption","name":"paid_message_star_count_max","value":{"@type":"optionValueInteger","value":"5000"}}"#,
    );
    ingest(
        &mut f,
        r#"{"@type":"updateOption","name":"can_enable_paid_messages","value":{"@type":"optionValueBoolean","value":true}}"#,
    );
    assert_eq!(f.1.session.payments.paid_messages.max(), 5000);
    assert!(f.1.session.payments.paid_messages.charging_available());
    // What the user pays the account arrives with the pushed full info,
    // and opening the chat sends no extra `getUserFullInfo`.
    f.1.maybe_fetch_paid_messages_info().unwrap();
    assert!(requests(&f, "getUserFullInfo").is_empty());
    ingest(
        &mut f,
        r#"{"@type":"updateUserFullInfo","user_id":541,"user_full_info":{"@type":"userFullInfo","incoming_paid_message_star_count":12,"outgoing_paid_message_star_count":0,"bio":null}}"#,
    );
    assert_eq!(f.1.session.user_pays_me(ChatId(540)), 12);
    cleanup(f);
}

#[test]
fn remove_fee_reads_revenue_then_allows_free_messages() {
    let mut f = fixture();
    open_private(&mut f, 550, 551, false, false);
    f.1.fetch_user_full_info(551).unwrap();
    let info = requests(&f, "getUserFullInfo").remove(0);
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"userFullInfo","incoming_paid_message_star_count":3,"bio":null,"@extra":"{}"}}"#,
            extra_of(&info)
        ),
    );
    f.1.fetch_paid_message_revenue(551).unwrap();
    f.1.fetch_paid_message_revenue(551).unwrap();
    let revenue = requests(&f, "getPaidMessageRevenue");
    assert_eq!(revenue.len(), 1, "one request at a time");
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"starCount","star_count":90,"@extra":"{}"}}"#,
            extra_of(&revenue[0])
        ),
    );
    assert_eq!(
        f.1.session.payments.paid_messages.revenue.get(&551),
        Some(&90)
    );
    f.1.allow_unpaid_messages(551, true).unwrap();
    let allow = requests(&f, "allowUnpaidMessagesFromUser").remove(0);
    assert_eq!(allow["user_id"], 551);
    assert_eq!(allow["refund_payments"], true);
    assert!(f.1.session.payments.paid_messages.saving);
    ingest(
        &mut f,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra_of(&allow)),
    );
    assert!(!f.1.session.payments.paid_messages.saving);
    assert_eq!(f.1.session.user_pays_me(ChatId(550)), 0);
    assert!(f.1.session.payments.paid_messages.revenue.is_empty());
    cleanup(f);
}

#[test]
fn a_refused_exception_leaves_a_line_to_show() {
    let mut f = fixture();
    f.1.allow_unpaid_messages(600, false).unwrap();
    let allow = requests(&f, "allowUnpaidMessagesFromUser").remove(0);
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"error","code":400,"message":"USER_NOT_FOUND","@extra":"{}"}}"#,
            extra_of(&allow)
        ),
    );
    assert!(!f.1.session.payments.paid_messages.saving);
    assert!(f.1.session.payments.paid_messages.error.is_some());
    cleanup(f);
}

#[test]
fn direct_messages_group_is_clamped_to_the_server_limit() {
    let mut f = fixture();
    ingest(
        &mut f,
        r#"{"@type":"updateOption","name":"paid_message_star_count_max","value":{"@type":"optionValueInteger","value":"100"}}"#,
    );
    f.1.set_direct_messages_group(ChatId(-1001), true, 5000)
        .unwrap();
    f.1.set_direct_messages_group(ChatId(-1001), false, 40)
        .unwrap();
    let sent = requests(&f, "setChatDirectMessagesGroup");
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0]["is_enabled"], true);
    assert_eq!(sent[0]["paid_message_star_count"], 100);
    assert_eq!(sent[1]["is_enabled"], false);
    assert_eq!(sent[1]["paid_message_star_count"], 0);
    cleanup(f);
}

#[test]
fn direct_messages_price_is_read_from_the_supergroup() {
    let mut f = fixture();
    ingest(
        &mut f,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":77,"is_direct_messages_group":true,"paid_message_star_count":30}}"#,
    );
    assert_eq!(
        f.1.session.payments.paid_messages.dm_price.get(&77),
        Some(&30)
    );
    cleanup(f);
}

#[test]
fn new_chat_settings_save_the_price_and_roll_it_back_on_failure() {
    let mut f = fixture();
    f.1.fetch_new_chat_privacy().unwrap();
    let get = requests(&f, "getNewChatPrivacySettings").remove(0);
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"newChatPrivacySettings","allow_new_chats_from_unknown_users":true,"incoming_paid_message_star_count":0,"@extra":"{}"}}"#,
            extra_of(&get)
        ),
    );
    f.1.set_new_chat_settings(crate::paid_messages::MessagesPolicy::Charge(40).to_settings())
        .unwrap();
    let set = requests(&f, "setNewChatPrivacySettings").remove(0);
    assert_eq!(set["settings"]["allow_new_chats_from_unknown_users"], true);
    assert_eq!(set["settings"]["incoming_paid_message_star_count"], 40);
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"error","code":400,"message":"STARS_NOT_ALLOWED","@extra":"{}"}}"#,
            extra_of(&set)
        ),
    );
    assert!(matches!(
        f.1.session.settings.privacy_data.new_chat,
        Some(crate::privacy::NewChatPrivacyState::Ready(s))
            if s.incoming_paid_message_star_count == 0 && s.allow_from_unknown
    ));
    cleanup(f);
}
