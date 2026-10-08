//! Launch / OS-delivered links are routed by shape (tdesktop `openLocalUrl`),
//! with TDLib answers recorded as JSON.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
use crate::state::{DeepLinkAction, DeepLinkState};
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

#[test]
fn launch_links_resolve_locally_instead_of_get_deep_link_info() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);

    // tg://resolve and t.me/<user> go straight to searchPublicChat; TDLib's
    // getDeepLinkInfo answers 404 for them.
    for link in ["tg://resolve?domain=durov", "https://t.me/durov"] {
        driver.session.deep_link = None;
        let extra = driver.request_deep_link_info(link).unwrap().unwrap();
        assert_eq!(
            sent_request(&recorder, "searchPublicChat")["username"],
            "durov"
        );
        assert!(
            !recorder
                .snapshot()
                .iter()
                .any(|j| j.contains("getDeepLinkInfo")),
            "{link}"
        );
        // Recorded shape of TDLib's searchPublicChat answer: a `chat`.
        feed(
            &mut driver,
            &seq,
            &sink,
            json!({"@type": "chat", "@extra": extra.as_extra(), "id": 42, "title": "Durov",
                "type": {"@type": "chatTypePrivate", "user_id": 42}, "unread_count": 0}),
        );
        assert!(matches!(
            driver.session.deep_link,
            Some(DeepLinkState::ChatReady { chat_id, action: DeepLinkAction::OpenUsername { ref domain, .. } })
                if chat_id.0 == 42 && domain == "durov"
        ));
    }

    // Invites (web `+hash`) go through the checked preview, never auto-join.
    driver.session.deep_link = None;
    driver
        .request_deep_link_info("https://t.me/+AbCd")
        .unwrap()
        .unwrap();
    assert_eq!(
        sent_request(&recorder, "checkChatInviteLink")["invite_link"],
        "https://t.me/+AbCd"
    );

    // A link with no local form still asks TDLib.
    driver.session.deep_link = None;
    driver
        .request_deep_link_info("tg://proxy?server=x&port=1")
        .unwrap()
        .unwrap();
    assert_eq!(
        sent_request(&recorder, "getDeepLinkInfo")["link"],
        "tg://proxy?server=x&port=1"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn failed_links_use_tdesktop_wording() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let mut fail = |link: &str, code: i32, msg: &str| {
        driver.session.deep_link = None;
        let extra = driver.request_deep_link_info(link).unwrap().unwrap();
        feed(
            &mut driver,
            &seq,
            &sink,
            json!({"@type": "error", "@extra": extra.as_extra(), "code": code, "message": msg}),
        );
        match driver.session.deep_link.clone() {
            Some(DeepLinkState::ShowText(text)) => text,
            other => panic!("unexpected {other:?}"),
        }
    };
    assert_eq!(
        fail("https://t.me/nobody_here", 400, "USERNAME_NOT_OCCUPIED"),
        "The username \"nobody_here\" is not occupied by anyone."
    );
    assert_eq!(
        fail("tg://join?invite=Dead", 400, "INVITE_HASH_EXPIRED"),
        "This invite link is broken or has expired."
    );
    assert_eq!(
        fail("tg://proxy?server=x&port=1", 404, "Not Found"),
        "This link isn't supported by Quill."
    );
    assert_eq!(
        fail("tg://resolve?domain=durov", 500, "Internal"),
        "Couldn't open the link (error 500)."
    );
    std::fs::remove_dir_all(dir).unwrap();
}
