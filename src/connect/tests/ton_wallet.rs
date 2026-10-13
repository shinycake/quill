//! Connect-driver tests: the read-only TON wallet. The explorer prefix comes
//! from one `getApplicationConfig`, wallet messages preview in the chat
//! list, and nothing else is ever sent.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
use crate::state::RequestPurpose;
use crate::telegram::client::copy_and_parse;
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
    fn new() -> Self {
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
        rig.driver.session.my_user_id = Some(9);
        rig.ingest(
            r#"{"@type":"updateUser","user":{"id":1,"first_name":"Dana","last_name":"Cole","type":{"@type":"userTypeRegular"}}}"#,
        );
        rig.ingest(
            r#"{"@type":"updateNewChat","chat":{"id":1,"title":"Dana Cole","type":{"@type":"chatTypePrivate","user_id":1},"unread_count":0}}"#,
        );
        rig
    }

    fn ingest(&mut self, json: &str) {
        self.driver
            .ingest(copy_and_parse(json, &self.seq, &self.sink).unwrap())
            .unwrap();
    }

    fn sent(&self, ty: &str) -> usize {
        self.recorder
            .snapshot()
            .into_iter()
            .filter(|json| json.contains(&format!(r#""@type":"{ty}""#)))
            .count()
    }
}

#[test]
fn explorer_prefix_is_asked_once_and_applied() {
    let mut rig = Rig::new();
    assert!(rig.driver.fetch_ton_explorer_url().unwrap());
    assert!(!rig.driver.fetch_ton_explorer_url().unwrap(), "asked once");
    assert_eq!(rig.sent("getApplicationConfig"), 1);
    let extra = rig
        .driver
        .session
        .requests
        .pending_extra_for(RequestPurpose::GetTonExplorerUrl, None)
        .expect("config pending");
    rig.ingest(&format!(
        r#"{{"@type":"jsonValueObject","@extra":"{}","members":[{{"key":"ton_blockchain_explorer_url","value":{{"@type":"jsonValueString","value":"https://tonscan.org"}}}}]}}"#,
        extra.0
    ));
    let wallet = &rig.driver.session.ton_wallet;
    assert_eq!(wallet.explorer_url.as_deref(), Some("https://tonscan.org"));
    assert_eq!(
        wallet.explorer_url_for("EQabc"),
        "https://tonscan.org/EQabc"
    );
    // The boosts value is left alone: it was not what was asked.
    assert_eq!(rig.driver.session.groups.boosts_per_gift, None);
}

#[test]
fn a_failed_config_keeps_the_default_explorer() {
    let mut rig = Rig::new();
    rig.driver.fetch_ton_explorer_url().unwrap();
    let extra = rig
        .driver
        .session
        .requests
        .pending_extra_for(RequestPurpose::GetTonExplorerUrl, None)
        .expect("config pending");
    rig.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"BAD"}}"#,
        extra.0
    ));
    assert_eq!(
        rig.driver.session.ton_wallet.explorer_url_for("EQabc"),
        "https://tonviewer.com/EQabc"
    );
}

#[test]
fn wallet_messages_preview_in_the_chat_list_and_send_nothing() {
    let mut rig = Rig::new();
    let before = rig.recorder.snapshot().len();
    rig.ingest(
        r#"{"@type":"updateChatLastMessage","chat_id":1,"last_message":{"id":10,"chat_id":1,"date":1,"is_outgoing":false,"sender_id":{"@type":"messageSenderUser","user_id":1},"content":{"@type":"messageTonWalletTransfer","sender_user_id":1,"transaction_id":"t","peer_address":"EQabc","amount":2500000000,"comment":"hi","is_comment_encrypted":false}},"positions":[]}"#,
    );
    let chat = rig.driver.session.chats.get(&1).expect("chat");
    assert_eq!(chat.last_preview, "sent you 2.5 Grams");
    rig.ingest(
        r#"{"@type":"updateChatLastMessage","chat_id":1,"last_message":{"id":11,"chat_id":1,"date":2,"is_outgoing":true,"sender_id":{"@type":"messageSenderUser","user_id":9},"content":{"@type":"messageTonWalletTransfer","sender_user_id":9,"transaction_id":"t2","peer_address":"EQabc","amount":-1000000000,"comment":"","is_comment_encrypted":false}},"positions":[]}"#,
    );
    let chat = rig.driver.session.chats.get(&1).expect("chat");
    assert_eq!(chat.last_preview, "You sent Dana 1 Gram");
    rig.ingest(
        r#"{"@type":"updateChatLastMessage","chat_id":1,"last_message":{"id":12,"chat_id":1,"date":3,"is_outgoing":false,"sender_id":{"@type":"messageSenderUser","user_id":1},"content":{"@type":"messageTonConnectRequest","session_id":"5","state":{"@type":"tonConnectRequestStatePending","expiration_date":4000000000},"dapp_name":"Fragment","topic":"","trace_id":""}},"positions":[]}"#,
    );
    let chat = rig.driver.session.chats.get(&1).expect("chat");
    assert_eq!(chat.last_preview, "Wallet request from Fragment");
    assert_eq!(
        rig.recorder.snapshot().len(),
        before,
        "displaying wallet messages sends nothing"
    );
}
