//! Sweep fixes (s4-chat-list) that need a send that fails.
use super::super::*;
use super::flood_retry_lost::SwitchSender;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::AccountKey;
use crate::platform::MemorySecretStore;
use crate::state::Session;
use crate::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

struct Rig {
    driver: ConnectDriver<SwitchSender>,
    down: Arc<AtomicBool>,
    _dir: std::path::PathBuf,
}

fn rig() -> Rig {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let down = Arc::new(AtomicBool::new(false));
    let sender = SwitchSender {
        inner: Arc::new(RecordingSender::new()),
        down: down.clone(),
    };
    let session = Session::new(AccountKey::primary(), sink.clone());
    let mut driver = ConnectDriver::new(session, sender, test_credentials(), prepared);
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
    Rig {
        driver,
        down,
        _dir: dir,
    }
}

#[test]
fn a_contacts_retry_that_cannot_be_sent_ends_in_the_error_not_in_loading() {
    let mut rig = rig();
    rig.down.store(true, Ordering::SeqCst);
    assert!(rig.driver.fetch_contacts().is_err());
    let users = &rig.driver.session.users_state;
    assert!(users.contacts.is_none());
    assert!(users.contacts_error, "the tab draws its Retry");
}

#[test]
fn the_sync_switch_stays_put_when_the_prefs_cannot_be_saved() {
    let mut rig = rig();
    assert!(rig.driver.session.settings.contact_prefs.sync_enabled);
    // The file's place is taken by a directory: the save fails.
    let file = rig.driver.paths.root.join("contacts_prefs.json");
    std::fs::create_dir_all(&file).unwrap();
    assert!(rig.driver.set_contact_sync(false).is_err());
    assert!(rig.driver.session.settings.contact_prefs.sync_enabled);
    std::fs::remove_dir_all(&file).unwrap();
    rig.driver.set_contact_sync(false).unwrap();
    assert!(!rig.driver.session.settings.contact_prefs.sync_enabled);
}
