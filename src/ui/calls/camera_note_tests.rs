use super::camera_toggle_note;
use quill::lang::{keys, tr};

#[test]
fn camera_note_follows_the_resulting_call_state_not_the_request() {
    // A toggle-on the driver left off (video not ready) says off.
    assert_eq!(
        camera_toggle_note(Ok(()), false).text(),
        tr(keys::Q_OPS_CAMERA_OFF_2)
    );
    assert_eq!(
        camera_toggle_note(Ok(()), true).text(),
        tr(keys::Q_OPS_CAMERA_ON_2)
    );
    let failed = camera_toggle_note(Err("boom".into()), true);
    assert!(failed.text().contains("boom"));
    assert_eq!(failed.kind(), crate::ui::status_note::NoteKind::Error);
}
