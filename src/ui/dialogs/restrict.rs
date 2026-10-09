use super::super::app::QuillApp;
use super::super::scheduled::new_date_time_picker;
use gpui_kit::component::date_picker::DatePickerState;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::local_time::now_unix;
use quill::moderation::RestrictUntil;
use quill::telegram::envelope::ChatPermissions;

/// Slice G1: restrict/ban dialog (`setChatMemberStatus`, schema
/// 1.8.67, line 13592). `until` is how long it lasts: a preset or a date
/// and time picked with the same date picker as scheduled messages
/// (tdesktop `EditRestrictedBox::showRestrictUntil`).
pub struct RestrictDialog {
    pub(crate) chat_id: ChatId,
    pub(crate) user_id: i64,
    pub(crate) ban: bool,
    pub(crate) until: RestrictUntil,
    /// The custom date and time; shown while `until` is `Custom`.
    pub(crate) custom: Entity<DatePickerState>,
    pub(crate) error: Option<&'static str>,
    pub(crate) permissions: ChatPermissions,
}

impl RestrictDialog {
    pub(crate) fn new(
        window: &mut Window,
        cx: &mut Context<QuillApp>,
        chat_id: ChatId,
        user_id: i64,
        ban: bool,
        current: ChatPermissions,
    ) -> Self {
        // tdesktop opens the custom picker one day ahead.
        let tomorrow = now_unix() + 86_400;
        Self {
            chat_id,
            user_id,
            ban,
            until: RestrictUntil::Forever,
            custom: new_date_time_picker(window, cx, tomorrow, 366 * 86_400),
            error: None,
            permissions: current,
        }
    }
}
