use super::super::app::QuillApp;
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::*;
use quill::lang::{keys, tr};
/// One-line fields.
const SINGLE_LINE: (usize, usize) = (1, 1);
/// The bio shows at least three lines and grows with its content, so a
/// long bio is never cut mid-line (a fixed 40 px box showed 1.5 lines).
pub(crate) const BIO_ROWS: (usize, usize) = (3, 8);

/// A5: edit-profile dialog — the profile edit UI entry point
/// (`parity:auth-edit-name`). One form with one Save (tdesktop's
/// `Settings::Information`): the name (`setName`) and bio (`setBio`)
/// fields are saved together, sending only what changed. The username row
/// keeps its availability check and its own button, the photo is chosen
/// with the native file dialog (`setProfilePhoto` / `deleteProfilePhoto`).
pub struct EditProfileDialog {
    pub(crate) first_name_input: Entity<TextareaState>,
    pub(crate) last_name_input: Entity<TextareaState>,
    pub(crate) bio_input: Entity<TextareaState>,
    pub(crate) username_input: Entity<TextareaState>,
    /// The name and bio the form opened with; Save sends only the fields
    /// that differ from these.
    pub(crate) initial: super::super::profile_edit_save::ProfileFields,
    /// The camera, emoji and video avatar panels.
    pub(crate) sources: crate::ui::profile_photo_sources::PhotoSources,
}

impl EditProfileDialog {
    pub(crate) fn new(
        window: &mut Window,
        cx: &mut Context<QuillApp>,
        first_name: &str,
        last_name: &str,
        bio: &str,
        username: &str,
    ) -> Self {
        pub(crate) fn field(
            window: &mut Window,
            cx: &mut Context<QuillApp>,
            placeholder: &str,
            value: &str,
            rows: (usize, usize),
        ) -> Entity<TextareaState> {
            cx.new(|cx| {
                let mut state = TextareaState::new(window, cx)
                    .placeholder(placeholder)
                    .auto_grow(rows.0, rows.1)
                    .submit_on_enter(false);
                state.set_value(value, window, cx);
                state
            })
        }
        let dialog = Self {
            first_name_input: field(
                window,
                cx,
                tr(keys::Q_OPS_FIRST_NAME),
                first_name,
                SINGLE_LINE,
            ),
            last_name_input: field(
                window,
                cx,
                tr(keys::Q_OPS_LAST_NAME),
                last_name,
                SINGLE_LINE,
            ),
            bio_input: field(window, cx, tr(keys::INFO_BIO_LABEL), bio, BIO_ROWS),
            username_input: field(window, cx, "username", username, SINGLE_LINE),
            initial: super::super::profile_edit_save::ProfileFields::new(
                first_name, last_name, bio,
            ),
            sources: Default::default(),
        };
        // Save's enabled state and the bio counter follow every keystroke.
        for input in [
            &dialog.first_name_input,
            &dialog.last_name_input,
            &dialog.bio_input,
        ] {
            cx.subscribe(input, |_this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            })
            .detach();
        }
        dialog
    }

    /// The name and bio as typed now.
    pub(crate) fn current_fields(
        &self,
        cx: &App,
    ) -> super::super::profile_edit_save::ProfileFields {
        super::super::profile_edit_save::ProfileFields::new(
            &Self::text(&self.first_name_input, cx),
            &Self::text(&self.last_name_input, cx),
            &Self::text(&self.bio_input, cx),
        )
    }

    pub(crate) fn text(entity: &Entity<TextareaState>, cx: &App) -> String {
        entity.read(cx).value().to_string()
    }

    /// The editable username as typed (a leading `@` is stripped —
    /// TGX shows usernames without it in the editor).
    pub(crate) fn username_text(&self, cx: &App) -> String {
        let text = Self::text(&self.username_input, cx);
        let text = text.trim();
        text.strip_prefix('@').unwrap_or(text).to_string()
    }
}

#[cfg(test)]
mod bio_rows_tests {
    /// The bio box starts at three lines (it was one fixed 40 px box).
    #[test]
    fn bio_shows_at_least_three_rows() {
        assert!(super::BIO_ROWS.0 >= 3 && super::BIO_ROWS.1 > super::BIO_ROWS.0);
    }
}
