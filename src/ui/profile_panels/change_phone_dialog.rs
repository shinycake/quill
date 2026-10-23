//! The note behind the Edit Profile phone row. tdesktop cannot change a
//! number: its Settings phone row and `tg://settings/edit/change-number`
//! only show `lng_change_phone_error`, so Quill shows the same note (with
//! a title, which the kit dialog always renders).

use super::*;
use quill::lang::{keys, tr};

/// `lng_change_phone_error`.
pub(super) fn change_phone_error() -> &'static str {
    tr(keys::CHANGE_PHONE_ERROR)
}

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
        let body = div().text_sm().child(change_phone_error());
        let footer = Button::new("change-phone-close")
            .label(tr(keys::BOX_OK))
            .on_click(cx.listener(|this, _, window, cx| {
                this.close_profile_dialog(cx);
                this.close_kit_dialog_if_done(DialogKind::ProfilePanel, window, cx);
            }));
        Some((
            tr(keys::Q_PEOPLE_CHANGE_PHONE_NUMBER).into(),
            body.into_any_element(),
            div().flex().gap_2().child(footer).into_any_element(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::change_phone_error;

    #[test]
    fn note_matches_tdesktop_string() {
        assert_eq!(
            change_phone_error(),
            "You can only change your phone number using mobile Telegram apps. Please use an official Telegram app on your phone to update your number."
        );
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "change_phone_dialog_tests.rs"]
mod ui_tests;
