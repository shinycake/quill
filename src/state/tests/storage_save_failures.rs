//! Data & Storage saves: a failure of one request survives the `ok` of another.
use super::common::*;
use super::*;
use crate::data_settings::PresetsSeed;

fn error(extra: RequestId) -> String {
    format!(
        r#"{{"@type":"error","@extra":"{}","code":500,"message":"boom"}}"#,
        extra.0
    )
}

fn ok(extra: RequestId) -> String {
    format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0)
}

#[test]
fn a_later_ok_does_not_wipe_a_failed_limit_option() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let failed = session.request(RequestPurpose::SetStorageOption, None);
    let accepted = session.request(RequestPurpose::SetStorageOption, None);
    apply_json(&mut session, &seq, &sink, &error(failed));
    apply_json(&mut session, &seq, &sink, &ok(accepted));
    assert!(
        session
            .settings
            .data_storage_save
            .error_of(StorageSaveKind::Limits)
            .is_some(),
        "the partly applied limits keep their failure line"
    );
    // The next save of that kind starts clean; another kind is untouched.
    session
        .settings
        .data_storage_save
        .fail(StorageSaveKind::ResetStatistics, "reset");
    session
        .settings
        .data_storage_save
        .begin(StorageSaveKind::Limits);
    assert!(
        session
            .settings
            .data_storage_save
            .error_of(StorageSaveKind::Limits)
            .is_none()
    );
    assert_eq!(session.settings.data_storage_error(), Some("reset"));
}

#[test]
fn the_presets_fetch_is_one_stage_that_fails_with_its_own_request_only() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let asked = session.request(RequestPurpose::GetAutoDownloadSettingsPresets, None);
    let other = session.request(RequestPurpose::GetAutoDownloadSettingsPresets, None);
    session.settings.data_storage.seeded = PresetsSeed::Loading(asked);
    apply_json(&mut session, &seq, &sink, &error(other));
    assert!(
        session.settings.data_storage.seeded.is_loading(),
        "an answer of another request changes nothing"
    );
    apply_json(&mut session, &seq, &sink, &error(asked));
    assert!(session.settings.data_storage_error().is_some());
    assert!(
        session.settings.data_storage.seeded.askable(),
        "Retry may ask again"
    );
}
