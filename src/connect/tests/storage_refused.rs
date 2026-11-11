//! A Data & Storage ask or save refused before any request ends in the
//! failure the page draws, never in a page that waits or a Save that does
//! nothing.
use super::super::{ConnectDriver, RecordingSender};
use super::*;
use crate::data_settings::{AutoDownloadNetSettings, NetworkKind, PresetsSeed};
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
use crate::state::StorageSaveKind;
use crate::telegram::envelope::AuthorizationState;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn refused_driver() -> (
    std::path::PathBuf,
    ConnectDriver<Arc<RecordingSender>>,
    Arc<RecordingSender>,
) {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    driver.session.auth = AuthorizationState::WaitPhoneNumber;
    (dir, driver, recorder)
}

#[test]
fn a_refused_presets_ask_fails_with_a_retry_not_an_empty_page() {
    let (dir, mut driver, _) = refused_driver();
    assert!(driver.fetch_auto_download_presets().is_err());
    assert!(driver.session.settings.data_storage.seeded.failure().is_some());
    assert_eq!(
        driver.session.settings.data_storage.seeded,
        PresetsSeed::Failed("Couldn't load auto-download settings.".into())
    );
    assert!(
        driver.session.settings.data_storage.seeded.askable(),
        "Retry may ask again"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_refused_save_or_limit_change_says_so() {
    let (dir, mut driver, recorder) = refused_driver();
    let before = recorder.snapshot().len();
    assert!(
        driver
            .set_auto_download_settings(NetworkKind::WiFi, AutoDownloadNetSettings::default())
            .is_err()
    );
    assert!(
        driver
            .session
            .settings
            .data_storage_save
            .error_of(StorageSaveKind::AutoDownload)
            .is_some()
    );
    assert!(driver.set_storage_limits(Some(1), None).is_err());
    assert!(
        driver
            .session
            .settings
            .data_storage_save
            .error_of(StorageSaveKind::Limits)
            .is_some()
    );
    assert!(driver.clear_storage(&[], &[]).is_err());
    assert!(driver.session.settings.storage_clear.failed().is_some());
    assert!(driver.maybe_fetch_storage_statistics().is_err());
    assert!(driver.session.settings.storage_stats.error().is_some());
    assert_eq!(recorder.snapshot().len(), before, "nothing went out");
    let _ = std::fs::remove_dir_all(dir);
}
