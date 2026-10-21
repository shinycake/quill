//! QR box driver tests: backdrops and `getUserLink`, with TDLib 1.8.69
//! JSON shapes.
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::peer_qr::color::Rgb;
use crate::peer_qr::free::{FreeRequest, FreeTicket};
use crate::peer_qr::link::{UserLinkData, UserLinkFetch};
use crate::peer_qr::session::BackdropsFetch;
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
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

#[test]
fn backdrops_load_once() {
    let mut f = fixture();
    f.1.fetch_share_theme_backdrops().unwrap();
    let request = last_request(&f);
    assert_eq!(request["@type"], "getProfileShareThemeBackdrops");
    let sent = f.2.snapshot().len();
    f.1.fetch_share_theme_backdrops().unwrap();
    assert_eq!(f.2.snapshot().len(), sent);
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"profileShareThemeBackdrops","@extra":"{}","backdrops":[
                {{"@type":"profileShareThemeBackdrop","id":7,"name":"Ocean",
                  "colors":{{"@type":"upgradedGiftBackdropColors","center_color":5934320,
                  "edge_color":2838453,"symbol_color":0,"text_color":16777215}}}}]}}"#,
            extra_of(&request)
        ),
    );
    let BackdropsFetch::Ready(list) = &f.1.session.users_state.peer_qr.backdrops else {
        panic!("backdrops not stored");
    };
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, 7);
    assert_eq!(list[0].center, Rgb::hex(0x5A_8CF0));
    assert_eq!(list[0].edge, Rgb::hex(0x2B_4FB5));
    cleanup(f);
}

#[test]
fn failed_backdrops_can_be_asked_again() {
    let mut f = fixture();
    f.1.fetch_share_theme_backdrops().unwrap();
    let request = last_request(&f);
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"METHOD_INVALID"}}"#,
            extra_of(&request)
        ),
    );
    assert_eq!(
        f.1.session.users_state.peer_qr.backdrops,
        BackdropsFetch::Failed
    );
    let sent = f.2.snapshot().len();
    f.1.fetch_share_theme_backdrops().unwrap();
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
