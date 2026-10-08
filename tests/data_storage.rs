//! Slice S4 fix-up: apply-on-ok reducer coverage for Data & Storage
//! (the `set_account_ttl_ok_stores_confirmed_days` pattern).

use quill::data_settings::{AutoDownloadNetSettings, NetworkKind};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::AccountKey;
use quill::state::{RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn session() -> (Session, Arc<MemorySink>) {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    (Session::new(AccountKey::primary(), dyn_sink), sink)
}

fn apply_json(session: &mut Session, seq: &AtomicU64, sink: &Arc<MemorySink>, json: &str) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let owned = copy_and_parse(json, seq, &dyn_sink).unwrap();
    session.apply(owned);
}

/// Slice S4: a `setAutoDownloadSettings` ok applies the confirmed sent
/// settings (they ride the purpose — the `ok` carries none), marks the
/// prefs seeded + dirty, and clears the error.
#[test]
fn set_auto_download_settings_ok_applies_confirmed_settings() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    assert!(!session.data_storage.seeded);
    let settings = AutoDownloadNetSettings {
        is_auto_download_enabled: true,
        max_photo_file_size: 5 * 1024 * 1024,
        use_less_data_for_calls: true,
        ..Default::default()
    };
    let extra = session.request(
        RequestPurpose::SetAutoDownloadSettings {
            network: NetworkKind::Mobile,
            settings,
        },
        None,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    let applied = session.data_storage.for_network(NetworkKind::Mobile);
    assert!(applied.is_auto_download_enabled);
    assert_eq!(applied.max_photo_file_size, 5 * 1024 * 1024);
    assert!(applied.use_less_data_for_calls);
    // The other networks are untouched by this network's ok.
    assert!(
        !session
            .data_storage
            .for_network(NetworkKind::WiFi)
            .is_auto_download_enabled
    );
    assert!(session.data_storage.seeded);
    assert!(session.data_storage_dirty);
    assert!(session.data_storage_error.is_none());
}

/// Slice S4: a `setAutoDownloadSettings` TDLib error surfaces on
/// `data_storage_error` (shown on the screen, never a toast) and the
/// unconfirmed settings are never applied.
#[test]
fn set_auto_download_settings_error_surfaces_without_applying() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let settings = AutoDownloadNetSettings {
        is_auto_download_enabled: true,
        ..Default::default()
    };
    let extra = session.request(
        RequestPurpose::SetAutoDownloadSettings {
            network: NetworkKind::WiFi,
            settings,
        },
        None,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"NETWORK_TYPE_INVALID"}}"#,
            extra.0,
        ),
    );
    let err = session.data_storage_error.expect("error surfaced");
    assert!(
        err.starts_with("Couldn't save auto-download settings:"),
        "{err}"
    );
    assert!(!session.data_storage.seeded);
    assert!(
        !session
            .data_storage
            .for_network(NetworkKind::WiFi)
            .is_auto_download_enabled
    );
}

/// Batch 6: an `optimizeStorage` answer drops the cached stats, clears
/// the working flags, records the freed bytes, and clears the error.
#[test]
fn optimize_storage_answer_drops_stats_and_reports_freed_bytes() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let stats_extra = session.request(RequestPurpose::GetStorageStatistics, None);
    session.storage_stats_loading = true;
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"storageStatistics","size":7000,"count":3,"by_chat":[],"@extra":"{}"}}"#,
            stats_extra.0,
        ),
    );
    assert!(session.storage_stats.is_some());
    session.data_storage_error = Some("stale".into());
    let extra = session.request(RequestPurpose::OptimizeStorage, None);
    session.storage_clearing = true;
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"storageStatistics","size":"5242880","count":2,"by_chat":[],"@extra":"{}"}}"#,
            extra.0
        ),
    );
    assert!(session.storage_stats.is_none());
    assert!(!session.storage_stats_loading);
    assert!(!session.storage_clearing);
    assert_eq!(session.storage_freed, Some(5_242_880));
    assert!(session.data_storage_error.is_none());
}

/// Batch 6: an `optimizeStorage` error surfaces without dropping the
/// cached stats or reporting freed bytes.
#[test]
fn optimize_storage_error_keeps_stats() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let stats_extra = session.request(RequestPurpose::GetStorageStatistics, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"storageStatistics","size":7000,"count":3,"by_chat":[],"@extra":"{}"}}"#,
            stats_extra.0,
        ),
    );
    assert!(session.storage_stats.is_some());
    let extra = session.request(RequestPurpose::OptimizeStorage, None);
    session.storage_clearing = true;
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"INTERNAL"}}"#,
            extra.0,
        ),
    );
    let err = session.data_storage_error.expect("error surfaced");
    assert!(err.starts_with("Couldn't clear the cache:"), "{err}");
    assert!(session.storage_stats.is_some());
    assert!(!session.storage_clearing);
    assert!(session.storage_freed.is_none());
}
