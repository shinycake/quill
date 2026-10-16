use super::super::app::QuillApp;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::lang::{keys, tr};
/// One-line fields.
const SINGLE_LINE: (usize, usize) = (1, 1);
/// The bio shows at least three lines and grows with its content, so a
/// long bio is never cut mid-line (a fixed 40 px box showed 1.5 lines).
pub(crate) const BIO_ROWS: (usize, usize) = (3, 8);

/// A5: edit-profile dialog — the profile edit UI entry point
/// (`parity:auth-edit-name`). One dialog with per-section saves, mirroring
/// TGX's edit-profile rows: name (`setName`), bio (`setBio`), username
/// (`setUsername` + availability check + active/disabled lists), and photo
/// (`setProfilePhoto` / `deleteProfilePhoto`). Photo upload is a path
/// entry like the story composer (no native file-picker infrastructure
/// yet).
pub struct EditProfileDialog {
    pub(crate) first_name_input: Entity<TextareaState>,
    pub(crate) last_name_input: Entity<TextareaState>,
    pub(crate) bio_input: Entity<TextareaState>,
    pub(crate) username_input: Entity<TextareaState>,
    pub(crate) photo_path_input: Entity<TextareaState>,
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
        Self {
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
            photo_path_input: field(window, cx, "/path/to/photo.jpg", "", SINGLE_LINE),
            sources: Default::default(),
        }
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
