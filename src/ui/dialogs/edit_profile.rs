use super::super::app::QuillApp;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
/// A5: edit-profile dialog — the profile edit UI entry point
/// (`parity:auth-edit-name`). One dialog with per-section saves, mirroring
/// TGX's edit-profile rows: name (`setName`), bio (`setBio`), username
/// (`setUsername` + availability check + active/disabled lists), and photo
/// (`setProfilePhoto` / `deleteProfilePhoto`). Photo upload is a path
/// entry like the story composer (no native file-picker infrastructure
/// yet).
///
/// A12: `accent_selection` — the accent color picked in the dialog
/// (`setProfileAccentColor`); -1 = none. Initialized from the user's
/// current `profile_accent_color_id`.
pub struct EditProfileDialog {
    pub(crate) first_name_input: Entity<TextareaState>,
    pub(crate) last_name_input: Entity<TextareaState>,
    pub(crate) bio_input: Entity<TextareaState>,
    pub(crate) username_input: Entity<TextareaState>,
    pub(crate) photo_path_input: Entity<TextareaState>,
    pub(crate) accent_selection: i32,
}

impl EditProfileDialog {
    pub(crate) fn new(
        window: &mut Window,
        cx: &mut Context<QuillApp>,
        first_name: &str,
        last_name: &str,
        bio: &str,
        username: &str,
        accent_selection: i32,
    ) -> Self {
        pub(crate) fn field(
            window: &mut Window,
            cx: &mut Context<QuillApp>,
            placeholder: &str,
            value: &str,
        ) -> Entity<TextareaState> {
            cx.new(|cx| {
                let mut state = TextareaState::new(window, cx)
                    .placeholder(placeholder)
                    .auto_grow(1, 1)
                    .submit_on_enter(false);
                state.set_value(value, window, cx);
                state
            })
        }
        Self {
            first_name_input: field(window, cx, "First name", first_name),
            last_name_input: field(window, cx, "Last name", last_name),
            bio_input: field(window, cx, "Bio", bio),
            username_input: field(window, cx, "username", username),
            photo_path_input: field(window, cx, "/path/to/photo.jpg", ""),
            accent_selection,
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
