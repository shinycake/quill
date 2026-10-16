use super::camera_toggle_note;
use quill::lang::{keys, tr};

#[test]
fn camera_note_follows_the_resulting_call_state_not_the_request() {
    // A toggle-on the driver left off (video not ready) says off.
    assert_eq!(
        camera_toggle_note(Ok(()), false),
        tr(keys::Q_OPS_CAMERA_OFF_2)
    );
    assert_eq!(
        camera_toggle_note(Ok(()), true),
        tr(keys::Q_OPS_CAMERA_ON_2)
    );
    assert!(camera_toggle_note(Err("boom".into()), true).contains("boom"));
}
