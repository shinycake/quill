//! Set, Suggest and Reset a person's photo from the Edit contact and Add
//! contact boxes (tdesktop `EditContactBox` in `edit_contact_box.cpp`),
//! not from the profile's action list.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::pressable::action_row;
use crate::ui::shell::DialogKind;
use gpui_kit::assets::IconName;
use gpui_kit::*;
use quill::profile_forms::PersonalPhotoMode;

/// The box the rows sit in. A pick from the Add contact box closes it
/// first; the Edit contact box is replaced by the photo step itself.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum PhotoRowsHost {
    EditContact,
    AddContact,
}

impl QuillApp {
    pub(in crate::ui) fn contact_photo_rows(
        &self,
        user_id: i64,
        host: PhotoRowsHost,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        let user = session.user(user_id)?;
        if session.my_user_id == Some(user_id) || user.is_bot {
            return None;
        }
        let has_personal = session
            .user_full_info(user_id)
            .is_some_and(|info| info.extras.personal_photo.is_some());
        let rows = [
            (
                user.is_contact,
                PersonalPhotoMode::Set,
                "contact-photo-set",
                IconName::Image,
            ),
            (
                true,
                PersonalPhotoMode::Suggest,
                "contact-photo-suggest",
                IconName::Camera,
            ),
            (
                has_personal,
                PersonalPhotoMode::Reset,
                "contact-photo-reset",
                IconName::RotateCcw,
            ),
        ];
        let mut column = div().flex().flex_col().w_full().gap_0p5();
        for (shown, mode, id, icon) in rows {
            if !shown {
                continue;
            }
            column = column.child(
                action_row(id, Some(icon), mode.title(), false, cx)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if host == PhotoRowsHost::AddContact {
                            this.close_add_contact_dialog(cx);
                            this.close_kit_dialog_if_done(DialogKind::AddContact, window, cx);
                        }
                        this.start_personal_photo(user_id, mode, cx);
                    }))
                    .test_support(),
            );
        }
        Some(column.into_any_element())
    }
}
