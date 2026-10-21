//! QR box driver tests: backdrops and `getUserLink`, with TDLib 1.8.69
//! JSON shapes.
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::peer_qr::color::Rgb;
use crate::peer_qr::free::{FreeRequest, FreeTicket};
use crate::peer_qr::link::{UserLinkData, UserLinkFetch};
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use crate::telegram::requests_privacy::PrivacySettingKey;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

type Fixture = (
    std::path::PathBuf,
    ConnectDriver<Arc<RecordingSender>>,
    Arc<RecordingSender>,
    Arc<dyn DiagnosticSink>,
    AtomicU64,
);

fn fixture() -> Fixture {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let driver = ready_driver(&recorder, prepared, &sink, &seq);
    (dir, driver, recorder, sink, seq)
}

fn cleanup(f: Fixture) {
    let (dir, driver, ..) = f;
    drop(driver);
    std::fs::remove_dir_all(dir).unwrap();
}

fn ingest(f: &mut Fixture, json: &str) {
    let owned = copy_and_parse(json, &f.4, &f.3).unwrap();
    f.1.ingest(owned).unwrap();
}

fn last_request(f: &Fixture) -> Value {
    serde_json::from_str(f.2.snapshot().last().unwrap()).unwrap()
}

fn extra_of(request: &Value) -> String {
    request["@extra"].as_str().unwrap().to_string()
}

const NOW: i64 = 1_000_000;

fn answer_backdrops(f: &mut Fixture, request: &Value, id: i32) {
    ingest(
        f,
        &format!(
            r#"{{"@type":"profileShareThemeBackdrops","@extra":"{}","backdrops":[
                {{"@type":"profileShareThemeBackdrop","id":{id},"name":"Ocean",
                  "colors":{{"@type":"upgradedGiftBackdropColors","center_color":5934320,
                  "edge_color":2838453,"symbol_color":0,"text_color":16777215}}}}]}}"#,
            extra_of(request)
        ),
    );
}

#[test]
fn backdrops_are_asked_for_again_an_hour_later() {
    let mut f = fixture();
    f.1.fetch_share_theme_backdrops(NOW).unwrap();
    let request = last_request(&f);
    assert_eq!(request["@type"], "getProfileShareThemeBackdrops");
    let sent = f.2.snapshot().len();
    f.1.fetch_share_theme_backdrops(NOW).unwrap();
    assert_eq!(f.2.snapshot().len(), sent, "one request at a time");
    answer_backdrops(&mut f, &request, 7);
    let list = &f.1.session.users_state.peer_qr.backdrops.list;
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, 7);
    assert_eq!(list[0].center, Rgb::hex(0x5A_8CF0));
    assert_eq!(list[0].edge, Rgb::hex(0x2B_4FB5));
    f.1.fetch_share_theme_backdrops(NOW + 3599).unwrap();
    assert_eq!(f.2.snapshot().len(), sent, "fresh for an hour");
    f.1.fetch_share_theme_backdrops(NOW + 3600).unwrap();
    assert_eq!(f.2.snapshot().len(), sent + 1);
    assert_eq!(
        f.1.session.users_state.peer_qr.backdrops.list.len(),
        1,
        "the old list stays while the refresh is on its way"
    );
    let refresh = last_request(&f);
    answer_backdrops(&mut f, &refresh, 9);
    assert_eq!(f.1.session.users_state.peer_qr.backdrops.list[0].id, 9);
    cleanup(f);
}

#[test]
fn failed_backdrops_can_be_asked_again() {
    let mut f = fixture();
    f.1.fetch_share_theme_backdrops(NOW).unwrap();
    let request = last_request(&f);
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"METHOD_INVALID"}}"#,
            extra_of(&request)
        ),
    );
    let backdrops = &f.1.session.users_state.peer_qr.backdrops;
    assert!(!backdrops.loading);
    assert_eq!(backdrops.requested_at, None);
    let sent = f.2.snapshot().len();
    f.1.fetch_share_theme_backdrops(NOW + 1).unwrap();
    assert_eq!(f.2.snapshot().len(), sent + 1);
    cleanup(f);
}

#[test]
fn contact_link_expires_relative_to_now() {
    let mut f = fixture();
    f.1.fetch_own_user_link(None).unwrap();
    let request = last_request(&f);
    assert_eq!(request["@type"], "getUserLink");
    assert_eq!(request["remove_messages_fee"], false);
    assert_eq!(request["return_permanent_link"], false);
    assert_eq!(request["revoke_previous_links"], false);
    let before = i64::try_from(crate::state::unix_ms_now() / 1000).unwrap();
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"userLink","@extra":"{}","url":"https://t.me/contact/abc",
                "expires_in":300,"can_get_permanent_link":false}}"#,
            extra_of(&request)
        ),
    );
    let UserLinkFetch::Ready(UserLinkData { url, expires_at }) =
        &f.1.session.users_state.peer_qr.contact_link
    else {
        panic!("contact link not stored");
    };
    assert_eq!(url, "https://t.me/contact/abc");
    assert!((before + 300..=before + 302).contains(expires_at));
    cleanup(f);
}

#[test]
fn free_codes_are_queued_for_the_box() {
    let mut f = fixture();
    let reset = FreeTicket {
        request: FreeRequest::ResetPermanent,
        box_id: 1,
        serial: 1,
    };
    let temporary = FreeTicket {
        request: FreeRequest::Temporary,
        box_id: 1,
        serial: 2,
    };
    f.1.fetch_own_user_link(Some(reset)).unwrap();
    let request = last_request(&f);
    assert_eq!(request["remove_messages_fee"], true);
    assert_eq!(request["return_permanent_link"], true);
    assert_eq!(request["revoke_previous_links"], true);
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"userLink","@extra":"{}","url":"https://t.me/free",
                "expires_in":0,"can_get_permanent_link":true}}"#,
            extra_of(&request)
        ),
    );
    f.1.fetch_own_user_link(Some(temporary)).unwrap();
    let failed = last_request(&f);
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"PREMIUM_ACCOUNT_REQUIRED"}}"#,
            extra_of(&failed)
        ),
    );
    let answers = f.1.session.users_state.peer_qr.take_free_answers();
    assert_eq!(answers.len(), 2);
    let (ticket, code) = &answers[0];
    assert_eq!(*ticket, reset);
    let code = code.as_ref().unwrap();
    assert_eq!(code.link.url, "https://t.me/free");
    assert_eq!(code.link.expires_at, 0);
    assert!(code.can_permanent);
    let (ticket, failed) = &answers[1];
    assert_eq!(*ticket, temporary);
    assert!(failed.as_ref().is_err_and(|reason| !reason.is_empty()));
    assert!(
        f.1.session
            .users_state
            .peer_qr
            .take_free_answers()
            .is_empty()
    );
    assert_eq!(
        f.1.session.users_state.peer_qr.contact_link,
        UserLinkFetch::Idle
    );
    cleanup(f);
}

#[test]
fn a_failed_contact_link_keeps_its_reason_and_is_asked_again() {
    let mut f = fixture();
    f.1.fetch_own_user_link(None).unwrap();
    let request = last_request(&f);
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":429,"message":"Too Many Requests: retry after 30"}}"#,
            extra_of(&request)
        ),
    );
    let qr = &mut f.1.session.users_state.peer_qr;
    assert_eq!(qr.contact_link, UserLinkFetch::Failed);
    assert!(
        qr.contact_error
            .as_ref()
            .is_some_and(|reason| !reason.is_empty())
    );
    qr.retry_contact_link();
    assert_eq!(qr.contact_link, UserLinkFetch::Idle);
    assert_eq!(qr.contact_error, None);
    let sent = f.2.snapshot().len();
    f.1.fetch_own_user_link(None).unwrap();
    assert_eq!(f.2.snapshot().len(), sent + 1);
    cleanup(f);
}

/// The box opens, asks for the phone privacy rules, and only learns from
/// their answers that it needs a contact link.
#[test]
fn rules_landing_after_the_box_opened_ask_for_the_contact_link() {
    const ME: i64 = 77;
    let mut f = fixture();
    f.1.session.my_user_id = Some(ME);
    ingest(
        &mut f,
        r#"{"@type":"updateUser","user":{"id":77,"first_name":"Avery","phone_number":"15550001","type":{"@type":"userTypeRegular"}}}"#,
    );
    f.1.fetch_privacy_rules(PrivacySettingKey::ShowPhoneNumber)
        .unwrap();
    let show = last_request(&f);
    f.1.fetch_privacy_rules(PrivacySettingKey::AllowFindingByPhoneNumber)
        .unwrap();
    let find = last_request(&f);
    let now = crate::local_time::now_unix();
    let sent = f.2.snapshot().len();
    f.1.ensure_own_contact_link(now).unwrap();
    assert_eq!(f.2.snapshot().len(), sent, "the rules are still loading");
    for (request, rule) in [
        (&show, "userPrivacySettingRuleRestrictAll"),
        (&find, "userPrivacySettingRuleAllowContacts"),
    ] {
        ingest(
            &mut f,
            &format!(
                r#"{{"@type":"userPrivacySettingRules","rules":[{{"@type":"{rule}"}}],"@extra":"{}"}}"#,
                extra_of(request)
            ),
        );
    }
    f.1.ensure_own_contact_link(now).unwrap();
    assert_eq!(f.2.snapshot().len(), sent + 1);
    let request = last_request(&f);
    assert_eq!(request["@type"], "getUserLink");
    assert_eq!(request["remove_messages_fee"], false);
    f.1.ensure_own_contact_link(now).unwrap();
    assert_eq!(f.2.snapshot().len(), sent + 1, "asked once");
    cleanup(f);
}
