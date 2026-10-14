//! The box behind your own phone row. tdesktop cannot change a number:
//! tapping the row (Settings, or `tg://settings/edit/change-number`) only
//! shows `lng_change_phone_error`, so Quill shows the same note.

use super::*;

/// `lng_change_phone_error`.
pub(super) const CHANGE_PHONE_ERROR: &str = "You can only change your phone number using mobile Telegram apps. Please use an official Telegram app on your phone to update your number.";

impl QuillApp {
    pub(in crate::ui) fn open_change_phone_dialog(&mut self, cx: &mut Context<Self>) {
        self.dialogs.profile_dialog = Some(ProfileDialog::ChangePhone);
        cx.notify();
    }

    /// Title, body and footer of the note.
    pub(super) fn change_phone_dialog_parts(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<(String, AnyElement, AnyElement)> {
        let body = div().text_sm().child(CHANGE_PHONE_ERROR);
        let footer = Button::new("change-phone-close")
            .label("OK")
            .on_click(cx.listener(|this, _, window, cx| {
                this.close_profile_dialog(cx);
                this.close_kit_dialog_if_done(DialogKind::ProfilePanel, window, cx);
            }));
        Some((
            "Change phone number".into(),
            body.into_any_element(),
            div().flex().gap_2().child(footer).into_any_element(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::CHANGE_PHONE_ERROR;

    #[test]
    fn note_matches_tdesktop_string() {
        assert!(CHANGE_PHONE_ERROR.starts_with("You can only change your phone number"));
        assert!(CHANGE_PHONE_ERROR.ends_with("to update your number."));
    }
}

#[cfg(all(test, feature = "demo-capture"))]
#[path = "change_phone_dialog_tests.rs"]
mod ui_tests;
