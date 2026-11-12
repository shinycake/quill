use crate::ui::app_slice::window_slices::WindowSlices;

#[test]
fn a_frame_without_the_conversation_counts_as_drawn() {
    let slices = WindowSlices::default();
    assert!(!slices.conversation_shown());
    assert!(slices.history_drawn());
}

#[test]
fn a_replayed_conversation_is_shown_but_not_drawn() {
    let slices = WindowSlices::default();
    slices.mark_conversation_shown();
    assert!(slices.conversation_shown());
    assert!(!slices.history_drawn());
    assert!(!slices.conversation_shown());
}

#[test]
fn a_rendered_conversation_is_drawn_whichever_mark_comes_first() {
    let slices = WindowSlices::default();
    slices.mark_conversation_shown();
    slices.mark_conversation_rendered();
    assert!(slices.history_drawn());
    slices.mark_conversation_rendered();
    slices.mark_conversation_shown();
    assert!(slices.history_drawn());
    assert!(!slices.conversation_shown());
}
