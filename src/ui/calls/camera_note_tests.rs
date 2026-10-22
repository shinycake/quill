use super::camera_toggle_note;
use quill::calls::engine::EngineError;
use quill::lang::{keys, tr};

#[test]
fn camera_note_follows_the_resulting_call_state_not_the_request() {
    assert_eq!(
        camera_toggle_note(Ok(()), false).text(),
        tr(keys::Q_OPS_CAMERA_OFF_2)
    );
    assert_eq!(
        camera_toggle_note(Ok(()), true).text(),
        tr(keys::Q_OPS_CAMERA_ON_2)
    );
    let failed = camera_toggle_note(Err(EngineError::Unavailable), true);
    assert!(failed.text().contains("unavailable"));
    assert_eq!(failed.kind(), crate::ui::status_note::NoteKind::Error);
}

#[test]
fn a_missing_camera_is_named() {
    let note = camera_toggle_note(Err(EngineError::NoCamera), false);
    assert_eq!(note.text(), tr(keys::Q_OPS_NO_CAMERA_FOUND));
    assert_eq!(note.kind(), crate::ui::status_note::NoteKind::Error);
}
