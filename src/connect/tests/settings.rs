//! Connect-driver tests: sessions, websites, downloads.
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::ids::{AccountKey, ChatId, FileId};
use crate::platform::MemorySecretStore;
use crate::state::Session;
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::ParsedWebsite;
use serde_json::Value;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

impl JsonSender for Arc<FailDownloadSender> {
    fn send_json(&self, request: &str) -> Result<(), ConnectSendError> {
        if request.contains("downloadFile") || request.contains("addFileToDownloads") {
            return Err(ConnectSendError::Native);
        }
        self.sent
            .lock()
            .expect("download sender")
            .push(request.to_string());
        Ok(())
    }
}

#[test]
fn user_download_send_failure_marks_failed_download() {
    // An `addFileToDownloads` transport failure (the request never reached
    // TDLib) records the failure like an error response, so the row
    // offers Retry instead of silently returning to "not downloaded".
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let sender = Arc::new(FailDownloadSender {
        sent: Mutex::new(Vec::new()),
    });
    let mut driver = ConnectDriver::new(
        Session::new(AccountKey::primary(), sink.clone()),
        sender,
        test_credentials(),
        prepared,
    );
    let seq = AtomicU64::new(0);
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &sink,
                )
                .unwrap(),
            )
            .unwrap();
    let err = driver
        .download_user_file(FileId(21), Some((ChatId(7), 99)))
        .unwrap_err();
    assert!(matches!(err, ConnectSendError::Native));
    assert!(driver.session.failed_downloads.contains(&21));
    assert!(!driver.session.downloading.contains(&21));
    assert!(!driver.session.user_downloads.contains(&21));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn user_downloads_use_list_api_pause_and_cancel() {
    // Slice media-downloads-pause: user-initiated downloads go through
    // `addFileToDownloads` (not one-shot `downloadFile`); pause/resume
    // send `toggleDownloadIsPaused`; cancel of a listed download sends
    // `removeFileFromDownloads(delete_from_cache:false)`. Automatic
    // downloads stay on one-shot `downloadFile` / `cancelDownloadFile`.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let mut driver = ConnectDriver::new(
        Session::new(AccountKey::primary(), sink.clone()),
        recorder.clone(),
        test_credentials(),
        prepared,
    );
    let seq = AtomicU64::new(0);
    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    // The auth-ready ingest above may already have fired automatic
    // downloads (thumbs etc.), so index relative to the count before
    // this test's own requests.
    let base = recorder.snapshot().len();
    driver
        .download_user_file(FileId(51), Some((ChatId(7), 99)))
        .expect("user download")
        .expect("sent");
    let sent = recorder.snapshot();
    assert_eq!(sent.len(), base + 1);
    let add: Value = serde_json::from_str(&sent[base]).unwrap();
    assert_eq!(add["@type"], "addFileToDownloads");
    assert_eq!(add["file_id"], 51);
    assert_eq!(add["chat_id"], 7);
    assert_eq!(add["message_id"], 99);
    assert_eq!(add["priority"], 32);
    assert!(driver.session.user_downloads.contains(&51));
    assert!(driver.session.downloading.contains(&51));

    assert!(driver.pause_download(FileId(51)).expect("pause"));
    assert!(driver.resume_download(FileId(51)).expect("resume"));
    let sent = recorder.snapshot();
    assert_eq!(sent.len(), base + 3);
    let pause: Value = serde_json::from_str(&sent[base + 1]).unwrap();
    assert_eq!(pause["@type"], "toggleDownloadIsPaused");
    assert_eq!(pause["file_id"], 51);
    assert_eq!(pause["is_paused"], true);
    let resume: Value = serde_json::from_str(&sent[base + 2]).unwrap();
    assert_eq!(resume["@type"], "toggleDownloadIsPaused");
    assert_eq!(resume["file_id"], 51);
    assert_eq!(resume["is_paused"], false);

    // Pause of a non-user download is a no-op (never listed).
    assert!(!driver.pause_download(FileId(52)).expect("pause noop"));
    assert_eq!(recorder.snapshot().len(), base + 3);

    // Cancel of the listed download removes it from the list.
    assert!(driver.cancel_download(FileId(51)).expect("cancel"));
    let sent = recorder.snapshot();
    assert_eq!(sent.len(), base + 4);
    let cancel: Value = serde_json::from_str(&sent[base + 3]).unwrap();
    assert_eq!(cancel["@type"], "removeFileFromDownloads");
    assert_eq!(cancel["file_id"], 51);
    assert_eq!(cancel["delete_from_cache"], false);
    assert!(!driver.session.downloading.contains(&51));
    assert!(!driver.session.user_downloads.contains(&51));

    // Automatic download: one-shot `downloadFile` + `cancelDownloadFile`.
    driver
        .download_file(FileId(53), 1)
        .expect("auto download")
        .expect("sent");
    assert!(!driver.session.user_downloads.contains(&53));
    assert!(driver.cancel_download(FileId(53)).expect("auto cancel"));
    let sent = recorder.snapshot();
    assert_eq!(sent.len(), base + 6);
    let auto: Value = serde_json::from_str(&sent[base + 4]).unwrap();
    assert_eq!(auto["@type"], "downloadFile");
    let auto_cancel: Value = serde_json::from_str(&sent[base + 5]).unwrap();
    assert_eq!(auto_cancel["@type"], "cancelDownloadFile");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Slice B2: `send_bot_start_message` unblocks a fully-blocked bot
/// chat first (Telegram X `MessagesController.ACTION_BOT_START`
/// behavior), so "Restart bot" / the START press doesn't clear the
/// chat and then fail the start.
#[test]
fn b2_bot_start_unblocks_first_on_fully_blocked_chat() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

    // Chat 7: private bot chat, fully blocked (`blockListMain`).
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Bot","type":{"@type":"chatTypePrivate","user_id":42},"unread_count":0,"block_list":{"@type":"blockListMain"}}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert!(
        driver
            .session
            .chats
            .get(&7)
            .is_some_and(|chat| chat.blocked)
    );

    let extra = driver
        .send_bot_start_message(ChatId(7), 42, "startparam")
        .expect("start send")
        .expect("request id");

    // Unblock goes out BEFORE the start message (other Ready/chat
    // bookkeeping sends may also be in flight; filter like the
    // existing driver tests do).
    let of_type = |t: &str| {
        recorder
            .snapshot()
            .into_iter()
            .filter(|sent| sent.contains(&format!("\"@type\":\"{t}\"")))
            .collect::<Vec<_>>()
    };
    let unblock_sends = of_type("setMessageSenderBlockList");
    let start_sends = of_type("sendBotStartMessage");
    assert_eq!(unblock_sends.len(), 1);
    assert_eq!(start_sends.len(), 1);
    let unblock: Value = serde_json::from_str(&unblock_sends[0]).unwrap();
    assert!(unblock["block_list"].is_null());
    assert_eq!(unblock["sender_id"]["user_id"], 42);
    let start: Value = serde_json::from_str(&start_sends[0]).unwrap();
    assert_eq!(start["parameter"], "startparam");
    assert_eq!(start["@extra"], extra.0.to_string());
    // Order: the unblock was recorded before the start message.
    let snapshot = recorder.snapshot();
    let unblock_at = snapshot
        .iter()
        .position(|sent| sent.contains("\"@type\":\"setMessageSenderBlockList\""))
        .unwrap();
    let start_at = snapshot
        .iter()
        .position(|sent| sent.contains("\"@type\":\"sendBotStartMessage\""))
        .unwrap();
    assert!(unblock_at < start_at);

    // Chat 8: unblocked bot chat — only the start message, no unblock.
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":8,"title":"Bot2","type":{"@type":"chatTypePrivate","user_id":43},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    driver
        .send_bot_start_message(ChatId(8), 43, "")
        .expect("start send")
        .expect("request id");
    assert_eq!(of_type("setMessageSenderBlockList").len(), 1);
    assert_eq!(of_type("sendBotStartMessage").len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Slice A3: the fetch is guarded by Ready, deduped while in flight
/// and while the cache is fresh.
#[test]
fn sessions_fetch_guards() {
    // Not ready: refused.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink);
    let mut driver = ConnectDriver::new(session, recorder, test_credentials(), prepared);
    assert_invalid(driver.maybe_fetch_active_sessions());
    let _ = std::fs::remove_dir_all(&dir);

    // Ready: first fetch sends, the in-flight second is deduped.
    let (dir, mut driver, recorder, _sink, _dyn_sink, _seq) = sessions_driver();
    driver
        .maybe_fetch_active_sessions()
        .expect("send")
        .expect("request id");
    let snapshot = recorder.snapshot();
    assert!(
        snapshot
            .iter()
            .any(|s| s.contains("\"@type\":\"getActiveSessions\""))
    );
    let sent_before = snapshot.len();
    assert!(
        driver
            .maybe_fetch_active_sessions()
            .expect("dedupe")
            .is_none()
    );
    assert_eq!(recorder.snapshot().len(), sent_before);
    // Cached: no refetch.
    driver.session.sessions = Some(vec![]);
    assert!(
        driver
            .maybe_fetch_active_sessions()
            .expect("cached")
            .is_none()
    );
    assert_eq!(recorder.snapshot().len(), sent_before);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Slice A3: terminate guards — unknown id and concurrent mutations
/// are refused before anything is sent.
#[test]
fn sessions_terminate_guards() {
    let (dir, mut driver, recorder, _sink, _dyn_sink, _seq) = sessions_driver();
    driver.session.sessions = Some(vec![
        session_fixture(11, true, false),
        session_fixture(22, false, false),
    ]);
    // Unknown id: refused, nothing sent.
    let sent_before = recorder.snapshot().len();
    assert_invalid(driver.terminate_session(99));
    assert_eq!(recorder.snapshot().len(), sent_before);
    // Known id: sent; a second mutation while in flight is refused.
    driver.terminate_session(22).expect("terminate send");
    assert!(
        recorder.snapshot().iter().any(
            |s| s.contains("\"@type\":\"terminateSession\"") && s.contains("\"session_id\":22")
        )
    );
    assert_invalid(driver.terminate_session(11));
    assert_invalid(driver.terminate_all_other_sessions());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Slice A3: `terminateSession` → `ok` → the reducer marks the list
/// stale → the same ingest refetches `getActiveSessions` → the
/// authoritative answer replaces the cache (no optimistic deletion).
#[test]
fn sessions_terminate_ok_triggers_authoritative_refetch() {
    let (dir, mut driver, recorder, _sink, dyn_sink, seq) = sessions_driver();
    driver.session.sessions = Some(vec![
        session_fixture(11, true, false),
        session_fixture(22, false, false),
    ]);
    let extra = driver.terminate_session(22).expect("terminate send");
    let sent = recorder.snapshot().len();
    driver
        .ingest(
            copy_and_parse(
                &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    // The same ingest refetched the list (no optimistic deletion:
    // the old cache stays visible, marked stale, until the
    // authoritative answer replaces it).
    let snapshot = recorder.snapshot();
    assert_eq!(snapshot.len(), sent + 1);
    assert!(snapshot[sent].contains("\"@type\":\"getActiveSessions\""));
    assert_eq!(driver.session.sessions.as_ref().unwrap().len(), 2);
    assert!(driver.session.sessions_stale);
    assert!(!driver.session.sessions_mutating);
    // The authoritative answer lands — the @extra comes from the
    // recorded outbound JSON (what TDLib would echo back).
    let fetch_extra: i64 = {
        let snapshot = recorder.snapshot();
        let sent = snapshot
            .iter()
            .rev()
            .find(|s| s.contains("\"@type\":\"getActiveSessions\""))
            .expect("refetch sent");
        let v: serde_json::Value = serde_json::from_str(sent).unwrap();
        v["@extra"].as_str().unwrap().parse().unwrap()
    };
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"sessions","@extra":"{fetch_extra}","sessions":[{{"@type":"session","id":11,"is_current":true,"device_model":"Device 11","application_name":"Quill"}}]}}"#
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    let sessions = driver.session.sessions.as_ref().expect("refetched");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, 11);
    assert!(!driver.session.sessions_stale);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Slice A4: toggle guards — unknown id and concurrent mutations are
/// refused before anything is sent; the sent value negates the
/// cached flag.
#[test]
fn session_toggle_guards() {
    let (dir, mut driver, recorder, _sink, _dyn_sink, _seq) = sessions_driver();
    driver.session.sessions = Some(vec![
        session_fixture(11, true, false),
        session_fixture(22, false, false),
    ]);
    // Unknown id: refused, nothing sent.
    let sent_before = recorder.snapshot().len();
    assert_invalid(driver.toggle_session_can_accept_secret_chats(99));
    assert_invalid(driver.toggle_session_can_accept_calls(99));
    assert_eq!(recorder.snapshot().len(), sent_before);
    // The fixture rejects secret chats: the toggle sends `true`.
    // (The builder sends `session_id` as a JSON number, like
    // `terminateSession`.)
    driver
        .toggle_session_can_accept_secret_chats(22)
        .expect("toggle send");
    assert!(recorder.snapshot().iter().any(|s| {
        s.contains("\"@type\":\"toggleSessionCanAcceptSecretChats\"")
            && s.contains("\"session_id\":22")
            && s.contains("\"can_accept_secret_chats\":true")
    }));
    // A second mutation while in flight is refused.
    assert_invalid(driver.toggle_session_can_accept_calls(22));
    assert_invalid(driver.terminate_session(22));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Slice A4: `toggleSessionCanAcceptCalls` → `ok` → the reducer marks
/// the sessions list stale → the same ingest refetches
/// `getActiveSessions` (the toggled value arrives in the
/// authoritative answer, never optimistically).
#[test]
fn session_toggle_ok_triggers_authoritative_refetch() {
    let (dir, mut driver, recorder, _sink, dyn_sink, seq) = sessions_driver();
    driver.session.sessions = Some(vec![
        session_fixture(11, true, false),
        session_fixture(22, false, false),
    ]);
    let extra = driver
        .toggle_session_can_accept_calls(22)
        .expect("toggle send");
    let sent = recorder.snapshot().len();
    driver
        .ingest(
            copy_and_parse(
                &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    // The fixture accepted calls: the toggle sent `false`.
    assert!(recorder.snapshot()[sent - 1].contains("\"can_accept_calls\":false"));
    // The same ingest refetched the list; the cached flag is
    // untouched until the authoritative answer replaces it.
    let snapshot = recorder.snapshot();
    assert_eq!(snapshot.len(), sent + 1);
    assert!(snapshot[sent].contains("\"@type\":\"getActiveSessions\""));
    assert!(driver.session.sessions.as_ref().unwrap()[1].can_accept_calls);
    assert!(driver.session.sessions_stale);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Slice A4: websites fetch guards — in-flight fetch deduped, fresh
/// cache reused.
#[test]
fn websites_fetch_guards() {
    let (dir, mut driver, recorder, _sink, _dyn_sink, _seq) = sessions_driver();
    let sent_before = recorder.snapshot().len();
    driver.maybe_fetch_connected_websites().expect("fetch");
    assert!(
        recorder
            .snapshot()
            .iter()
            .any(|s| s.contains("\"@type\":\"getConnectedWebsites\""))
    );
    // In flight: deduped.
    let sent_after_first = recorder.snapshot().len();
    assert!(
        driver
            .maybe_fetch_connected_websites()
            .expect("dedup")
            .is_none()
    );
    assert_eq!(recorder.snapshot().len(), sent_after_first);
    // Cached: no refetch.
    driver.session.connected_websites = Some(vec![]);
    assert!(
        driver
            .maybe_fetch_connected_websites()
            .expect("cached")
            .is_none()
    );
    assert_eq!(recorder.snapshot().len(), sent_after_first);
    assert!(sent_after_first > sent_before);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Slice A4: disconnect guards — unknown website id and concurrent
/// mutations are refused before anything is sent.
#[test]
fn websites_disconnect_guards() {
    let (dir, mut driver, recorder, _sink, _dyn_sink, _seq) = sessions_driver();
    // Disconnect-all with no cache (or an empty one): refused,
    // nothing sent.
    let sent_before = recorder.snapshot().len();
    assert_invalid(driver.disconnect_all_websites());
    driver.session.connected_websites = Some(vec![]);
    assert_invalid(driver.disconnect_all_websites());
    assert_eq!(recorder.snapshot().len(), sent_before);
    driver.session.connected_websites = Some(vec![ParsedWebsite {
        id: 55,
        domain_name: "example.com".into(),
        bot_user_id: 77,
        browser: "Chrome".into(),
        platform: "Web".into(),
        log_in_date: 1758900000,
        last_active_date: 1759000000,
        ip_address: "9.9.9.9".into(),
        location: "Boston, United States".into(),
    }]);
    // Unknown id: refused, nothing sent.
    let sent_before = recorder.snapshot().len();
    assert_invalid(driver.disconnect_website(66));
    assert_eq!(recorder.snapshot().len(), sent_before);
    // Known id: sent; a second mutation while in flight is refused.
    driver.disconnect_website(55).expect("disconnect send");
    assert!(
        recorder
            .snapshot()
            .iter()
            .any(|s| s.contains("\"@type\":\"disconnectWebsite\"")
                && s.contains("\"website_id\":55"))
    );
    assert_invalid(driver.disconnect_website(55));
    assert_invalid(driver.disconnect_all_websites());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Slice A4: `disconnectAllWebsites` → `ok` → the reducer marks the
/// websites list stale → the same ingest refetches
/// `getConnectedWebsites` → the authoritative answer replaces the
/// cache (no optimistic deletion).
#[test]
fn disconnect_all_websites_ok_triggers_authoritative_refetch() {
    let (dir, mut driver, recorder, _sink, dyn_sink, seq) = sessions_driver();
    driver.session.connected_websites = Some(vec![ParsedWebsite {
        id: 55,
        domain_name: "example.com".into(),
        bot_user_id: 77,
        browser: "Chrome".into(),
        platform: "Web".into(),
        log_in_date: 1758900000,
        last_active_date: 1759000000,
        ip_address: "9.9.9.9".into(),
        location: "Boston, United States".into(),
    }]);
    let extra = driver.disconnect_all_websites().expect("disconnect send");
    let sent = recorder.snapshot().len();
    driver
        .ingest(
            copy_and_parse(
                &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    // The same ingest refetched the list (no optimistic deletion:
    // the old cache stays visible, marked stale, until the
    // authoritative answer replaces it).
    let snapshot = recorder.snapshot();
    assert_eq!(snapshot.len(), sent + 1);
    assert!(snapshot[sent].contains("\"@type\":\"getConnectedWebsites\""));
    assert_eq!(driver.session.connected_websites.as_ref().unwrap().len(), 1);
    assert!(driver.session.websites_stale);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn device_login_qr_requires_confirmation_and_correlated_native_acceptance() {
    let (dir, mut driver, recorder, sink, dyn_sink, seq) = sessions_driver();
    let link = "tg://login?token=AQID";
    assert!(
        !recorder
            .snapshot()
            .iter()
            .any(|s| s.contains("confirmQrCodeAuthentication"))
    );
    assert!(
        driver
            .confirm_device_login("https://example.invalid")
            .is_err()
    );
    let request = driver.confirm_device_login(link).unwrap();
    assert!(driver.confirm_device_login(link).is_err());
    let sent = recorder.snapshot().len();
    let response = |id: crate::ids::RequestId, body: serde_json::Value| {
        let mut body = body;
        body["@extra"] = serde_json::json!(id.as_extra());
        copy_and_parse(&body.to_string(), &seq, &dyn_sink).unwrap()
    };
    driver
        .ingest(response(
            crate::ids::RequestId(request.0 + 10000),
            serde_json::json!({"@type":"session","id":11}),
        ))
        .unwrap();
    assert!(driver.session.sessions_mutating);
    assert_eq!(driver.session.device_login_result, None);
    driver
        .ingest(response(
            request,
            serde_json::json!({"@type":"session","id":11}),
        ))
        .unwrap();
    assert_eq!(
        driver.session.device_login_result,
        Some(crate::auth::DeviceLoginResult::Linked)
    );
    assert!(!driver.session.sessions_mutating);
    assert!(
        recorder.snapshot()[sent..]
            .iter()
            .any(|s| s.contains("getActiveSessions"))
    );
    for body in [
        serde_json::json!({"@type":"session","id":0}),
        serde_json::json!({"@type":"error","code":400,"message":"LOGIN_TOKEN_EXPIRED"}),
    ] {
        let request = driver.confirm_device_login(link).unwrap();
        driver.ingest(response(request, body)).unwrap();
        assert_eq!(
            driver.session.device_login_result,
            Some(crate::auth::DeviceLoginResult::Failed)
        );
        assert!(!driver.session.sessions_mutating);
    }
    let request = driver.confirm_device_login(link).unwrap();
    driver
        .ingest(response(
            request,
            serde_json::json!({"@type":"session","id":22,"is_password_pending":true}),
        ))
        .unwrap();
    assert_eq!(
        driver.session.device_login_result,
        Some(crate::auth::DeviceLoginResult::PasswordRequired)
    );
    driver.session.auth = crate::telegram::envelope::AuthorizationState::WaitPhoneNumber;
    assert!(driver.confirm_device_login(link).is_err());
    assert!(!format!("{:?}", sink.snapshot()).contains(link));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn media_download_resolves_message_origin_and_picker_files_use_direct_api() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    driver.ingest(copy_and_parse(r#"{"@type":"updateNewMessage","message":{"id":99,"chat_id":7,"content":{"@type":"messageVideo","video":{"@type":"video","video":{"id":51,"local":{"can_be_downloaded":true}}}}}}"#, &seq, &sink).unwrap()).unwrap();
    driver
        .download_user_file(FileId(51), None)
        .unwrap()
        .unwrap();
    let sent = recorder.snapshot();
    let request: Value = serde_json::from_str(sent.last().unwrap()).unwrap();
    assert_eq!(request["@type"], "addFileToDownloads");
    assert_eq!(request["chat_id"], 7);
    assert_eq!(request["message_id"], 99);
    assert!(driver.session.user_downloads.contains(&51));
    driver
        .download_user_file(FileId(52), None)
        .unwrap()
        .unwrap();
    let sent = recorder.snapshot();
    let request: Value = serde_json::from_str(sent.last().unwrap()).unwrap();
    assert_eq!(request["@type"], "downloadFile");
    assert_eq!(request["file_id"], 52);
    assert!(!driver.session.user_downloads.contains(&52));
    assert!(driver.cancel_download(FileId(52)).unwrap());
    let sent = recorder.snapshot();
    let request: Value = serde_json::from_str(sent.last().unwrap()).unwrap();
    assert_eq!(request["@type"], "cancelDownloadFile");
    let _ = std::fs::remove_dir_all(dir);
}

/// The default auto-delete timer: fetched once, only valid values leave,
/// and the confirmed value lands from the `ok`.
#[test]
fn default_auto_delete_fetch_and_set() {
    let (dir, mut driver, recorder, _sink, dyn_sink, seq) = sessions_driver();
    let extra = driver
        .get_default_auto_delete()
        .expect("send")
        .expect("request id");
    assert!(
        recorder
            .snapshot()
            .iter()
            .any(|s| s.contains("\"@type\":\"getDefaultMessageAutoDeleteTime\""))
    );
    // Deduped while in flight.
    assert!(driver.get_default_auto_delete().expect("dedupe").is_none());
    let answer = format!(
        r#"{{"@type":"messageAutoDeleteTime","time":86400,"@extra":"{}"}}"#,
        extra.0
    );
    driver
        .ingest(copy_and_parse(&answer, &seq, &dyn_sink).unwrap())
        .unwrap();
    assert_eq!(driver.session.default_auto_delete_secs, Some(86_400));
    // Cached: no refetch.
    assert!(driver.get_default_auto_delete().expect("cached").is_none());

    // 1 hour is not a whole day: refused before it leaves.
    let sent = recorder.snapshot().len();
    assert!(driver.set_default_auto_delete(3600).is_err());
    assert_eq!(recorder.snapshot().len(), sent);

    let extra = driver.set_default_auto_delete(604_800).expect("send");
    assert!(
        recorder
            .snapshot()
            .last()
            .unwrap()
            .contains("\"@type\":\"setDefaultMessageAutoDeleteTime\"")
    );
    // One write at a time.
    assert!(driver.set_default_auto_delete(0).is_err());
    let ok = format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0);
    driver
        .ingest(copy_and_parse(&ok, &seq, &dyn_sink).unwrap())
        .unwrap();
    assert_eq!(driver.session.default_auto_delete_secs, Some(604_800));
    let _ = std::fs::remove_dir_all(&dir);
}
