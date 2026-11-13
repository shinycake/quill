//! What the New Group / New Channel dialog lets go of when a stage ends,
//! and what it says when a create cannot be sent. Driven through a real
//! `ConnectDriver` over a recording sender: nothing leaves the process.
use super::{Release, created};
use quill::connect::{ConnectDriver, RecordingSender, prepare_connect};
use quill::credentials::TelegramCredentials;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{AccountKey, ChatId, RequestId};
use quill::lang::{keys, tr};
use quill::new_chat::{NewChatKind, NewChatRequest, NewChatStage, TypeSaveStage};
use quill::platform::MemorySecretStore;
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn ready_driver() -> ConnectDriver<Arc<RecordingSender>> {
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let credentials = TelegramCredentials {
        api_id: 99,
        api_hash: "unit-test-hash-not-for-network".into(),
    };
    let dir = quill::scratch::unique_dir("quill-new-chat-release");
    let prepared = prepare_connect(
        &dir,
        AccountKey::primary(),
        &MemorySecretStore::new(),
        &credentials,
    )
    .expect("prepare");
    let session = Session::new(AccountKey::primary(), sink.clone());
    let mut driver = ConnectDriver::new(
        session,
        Arc::new(RecordingSender::new()),
        credentials,
        prepared,
    );
    let seq = AtomicU64::new(0);
    let ready = r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#;
    if let Some(parsed) = copy_and_parse(ready, &seq, &sink) {
        let _ = driver.ingest(parsed);
    }
    driver
}

fn saving(sent: &[RequestId]) -> NewChatStage {
    NewChatStage::Type {
        chat: ChatId(9),
        save: TypeSaveStage::Saving(sent.to_vec()),
    }
}

/// A type Save that settled (to the subscribers step, or refused) lets go
/// of its entries in the session; one still out keeps them.
#[test]
fn a_settled_type_save_is_forgotten() {
    let sent = [RequestId(3), RequestId(4)];
    let mut driver = ready_driver();
    driver.watch_new_chat_type_save(&sent);
    assert_eq!(driver.session.groups.new_chats.type_saves.len(), 2);
    let still = saving(&sent);
    assert_eq!(saving(&sent).released_by(&still), None, "still out");
    for settled in [
        NewChatStage::Subscribers { chat: ChatId(9) },
        NewChatStage::Type {
            chat: ChatId(9),
            save: TypeSaveStage::Refused("no".into()),
        },
    ] {
        let mut driver = ready_driver();
        driver.watch_new_chat_type_save(&sent);
        let release = saving(&sent).released_by(&settled);
        assert_eq!(release, Some(Release::Save(sent.to_vec())));
        if let Some(Release::Save(sent)) = release {
            driver.forget_new_chat_type_save(&sent);
        }
        assert!(
            driver.session.groups.new_chats.type_saves.is_empty(),
            "the settled Save's entries are gone: {settled:?}"
        );
    }
}

/// Create while a create of the same kind is still out: no request, and the
/// dialog says so instead of doing nothing.
#[test]
fn a_create_while_one_is_out_says_so() {
    let mut driver = ready_driver();
    let request = NewChatRequest {
        kind: NewChatKind::Channel,
        title: "News".into(),
        description: String::new(),
        user_ids: Vec::new(),
        auto_delete: 0,
    };
    let first = created(driver.create_new_chat(&request, None));
    assert!(first.is_ok(), "the first create goes out");
    let second = created(driver.create_new_chat(&request, None));
    let note = second.expect_err("a note, no request");
    assert_eq!(
        note,
        crate::ui::status_note::StatusNote::info(tr(keys::Q_PEOPLE_CHAT_STILL_BEING_CREATED))
    );
}
