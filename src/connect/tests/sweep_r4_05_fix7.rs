//! Fix round 7 of sweep bundle r4-05: a block or unblock that never left
//! shows its line and waits for nothing.
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::platform::MemorySecretStore;
use crate::state::BlockOrigin;
use crate::state::fetch::ChangeNote;
use crate::telegram::envelope::ParsedWebsite;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

type Recorder = Arc<RecordingSender>;

fn driver_parts() -> (ConnectDriver<Recorder>, Recorder) {
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink;
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    (driver, recorder)
}

fn shows_line_waiting_for_nothing(note: &ChangeNote) -> bool {
    matches!(note, ChangeNote::Refused { waiting, .. } if waiting.is_empty())
}

#[test]
fn a_failed_unblock_send_shows_the_line_with_nothing_waiting() {
    let (mut driver, recorder) = driver_parts();
    recorder.refuse_sends(true);
    assert!(driver.unblock_sender(62).is_err());
    assert!(shows_line_waiting_for_nothing(
        &driver.session.settings.block_note
    ));
}

#[test]
fn a_failed_block_send_shows_the_line_with_nothing_waiting() {
    let (mut driver, recorder) = driver_parts();
    recorder.refuse_sends(true);
    assert!(driver.block_sender(61, BlockOrigin::PrivacyPage).is_err());
    assert!(shows_line_waiting_for_nothing(
        &driver.session.settings.block_note
    ));
}

#[test]
fn a_failed_terminate_send_shows_the_line_with_nothing_waiting() {
    let (mut driver, recorder) = driver_parts();
    driver.session.settings.sessions = crate::state::fetch::Fetch::Ready(vec![
        support::session_fixture(11, true, false),
        support::session_fixture(22, false, false),
    ]);
    recorder.refuse_sends(true);
    assert!(driver.terminate_session(22).is_err());
    assert!(shows_line_waiting_for_nothing(
        &driver.session.settings.sessions_note
    ));
}

#[test]
fn a_failed_disconnect_website_send_shows_the_line_with_nothing_waiting() {
    let (mut driver, recorder) = driver_parts();
    driver.session.settings.connected_websites =
        crate::state::fetch::Fetch::Ready(vec![ParsedWebsite {
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
    recorder.refuse_sends(true);
    assert!(driver.disconnect_website(55).is_err());
    assert!(shows_line_waiting_for_nothing(
        &driver.session.settings.websites_note
    ));
}

#[test]
fn a_failed_device_link_send_shows_the_line_with_nothing_waiting() {
    let (mut driver, recorder) = driver_parts();
    recorder.refuse_sends(true);
    assert!(
        driver
            .confirm_device_login("tg://login?token=AQID")
            .is_err()
    );
    assert!(shows_line_waiting_for_nothing(
        &driver.session.settings.sessions_note
    ));
}

#[test]
fn a_failed_mask_change_send_shows_the_line_with_nothing_waiting() {
    let (mut driver, recorder) = driver_parts();
    recorder.refuse_sends(true);
    assert!(driver.change_mask_set(12, true, false).is_err());
    assert!(shows_line_waiting_for_nothing(
        &driver.session.stickers.masks.change_note
    ));
    assert!(
        !driver.session.stickers.masks.change_pending(),
        "a change that never left is not pending"
    );
    assert!(!driver.session.stickers.masks.set_change_pending(12));
}
