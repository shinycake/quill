use super::super::app::QuillApp;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
/// Phase C3a: voice-chat title rename dialog (`setVideoChatTitle`,
/// schema 1.8.67, line 14312). Created when the dialog opens with the
/// current title pre-filled.
pub struct GroupCallTitleDialog {
    pub(crate) title_input: Entity<TextareaState>,
}

impl GroupCallTitleDialog {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<QuillApp>, current: &str) -> Self {
        let title_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Voice chat title")
                .auto_grow(1, 2)
                .submit_on_enter(false)
        });
        title_input.update(cx, |input, cx| {
            input.set_value(current, window, cx);
        });
        Self { title_input }
    }
}
