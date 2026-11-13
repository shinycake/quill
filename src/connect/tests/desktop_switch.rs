use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

/// A client opened before the app's desktop notifications switch was
/// flipped (the run's first, `early_connect`) takes the new value when it
/// reads the switch again as it joins the app (sweep 19-003).
#[test]
fn a_client_opened_before_the_switch_takes_it_when_it_reads_again() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    driver.read_desktop_notifications(Some(&dir));
    assert!(driver.session.settings.desktop_notifications, "default: on");
    crate::settings::set_desktop_notifications(&dir, false).expect("saved");
    assert!(
        driver.session.settings.desktop_notifications,
        "opened before the switch"
    );
    driver.read_desktop_notifications(Some(&dir));
    assert!(!driver.session.settings.desktop_notifications);
    let _ = std::fs::remove_dir_all(&dir);
}
