use super::super::*;

/// Phase C2h: voice-chat start dialog (`createVideoChat`, schema
/// 1.8.67, line 14256) — title plus schedule presets. `start_date: 0`
/// starts immediately; otherwise a Unix timestamp. Presets are
/// relative offsets so no timezone handling is needed.
pub struct GroupCallStartDialog {
    pub(crate) chat_id: ChatId,
    pub(crate) title_input: Entity<TextareaState>,
    /// Seconds in the future (0 = now).
    pub(crate) schedule_offset: i64,
}

impl GroupCallStartDialog {
    pub(crate) fn new(
        window: &mut Window,
        cx: &mut Context<QuillApp>,
        chat_id: ChatId,
        chat_title: &str,
    ) -> Self {
        let title_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Voice chat title (empty = chat title)")
                .auto_grow(1, 2)
                .submit_on_enter(false)
        });
        title_input.update(cx, |input, cx| {
            input.set_value(chat_title, window, cx);
        });
        Self {
            chat_id,
            title_input,
            schedule_offset: 0,
        }
    }
}

/// Phase C2h: schedule presets for `createVideoChat.start_date`
/// (schema 1.8.67, line 14256: 0 = immediate, otherwise ≥10s and ≤8d
/// in the future).
pub(crate) const GROUP_CALL_SCHEDULE_PRESETS: [(i64, &str); 5] = [
    (0, "Now"),
    (3600, "In 1 hour"),
    (3 * 3600, "In 3 hours"),
    (12 * 3600, "In 12 hours"),
    (24 * 3600, "In 24 hours"),
];

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
