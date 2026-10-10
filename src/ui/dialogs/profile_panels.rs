use super::super::app::QuillApp;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;

fn field(
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

/// B10: edit-contact box (tdesktop `EditContactBox`): first/last name, the
/// private note and "Share my phone number".
pub struct EditContactDialog {
    pub(crate) user_id: i64,
    pub(crate) first_name_input: Entity<TextareaState>,
    pub(crate) last_name_input: Entity<TextareaState>,
    pub(crate) note_input: Entity<TextareaState>,
    pub(crate) share_phone: bool,
}

/// B10: birthday form for the current user (tdesktop's birthday box).
/// Day and month are required, the year is optional.
pub struct BirthdayDialog {
    pub(crate) day_input: Entity<TextareaState>,
    pub(crate) month_input: Entity<TextareaState>,
    pub(crate) year_input: Entity<TextareaState>,
    pub(crate) error: Option<&'static str>,
}

/// B10: the one dialog behind the profile and contact panels. Only one is
/// open at a time.
pub enum ProfileDialog {
    EditContact(EditContactDialog),
    Birthday(BirthdayDialog),
    /// Pick (or remove) the personal channel.
    PersonalChannel,
    /// Pick the chat to share `user_id`'s contact into; `target` is the
    /// chosen chat awaiting the confirm step.
    ShareContact {
        user_id: i64,
        target: Option<i64>,
    },
    /// Confirm setting, suggesting or resetting a contact's photo.
    PersonalPhoto {
        user_id: i64,
        mode: quill::profile_forms::PersonalPhotoMode,
        path: Option<String>,
    },
    /// Pick why a profile photo (`file_id`) is reported.
    ReportPhoto {
        user_id: i64,
        file_id: i32,
    },
    /// Send the game `game_short_name` of `bot_id` to a chat (`target` is
    /// the chosen chat awaiting the confirm step).
    ShareGame {
        bot_id: i64,
        game_short_name: String,
        target: Option<i64>,
    },
    /// Add the bot `bot_id` to a group or channel: the chat picker, then
    /// (`target` set) the rights to grant or the confirmation.
    AddBot {
        bot_id: i64,
        invite: quill::bot_invite::Invite,
        target: Option<i64>,
        rights: quill::telegram::envelope::ChatAdminRights,
    },
}

impl EditContactDialog {
    pub(crate) fn new(
        window: &mut Window,
        cx: &mut Context<QuillApp>,
        user_id: i64,
        first_name: &str,
        last_name: &str,
        note: &str,
    ) -> Self {
        Self {
            user_id,
            first_name_input: field(window, cx, "First name", first_name),
            last_name_input: field(window, cx, "Last name", last_name),
            note_input: field(window, cx, "Note (only visible to you)", note),
            share_phone: false,
        }
    }
}

impl BirthdayDialog {
    pub(crate) fn new(
        window: &mut Window,
        cx: &mut Context<QuillApp>,
        current: Option<(u8, u8, Option<i32>)>,
    ) -> Self {
        let (day, month, year) = current.map_or_else(Default::default, |(day, month, year)| {
            (
                day.to_string(),
                month.to_string(),
                year.map(|y| y.to_string()).unwrap_or_default(),
            )
        });
        Self {
            day_input: field(window, cx, "Day", &day),
            month_input: field(window, cx, "Month (1-12)", &month),
            year_input: field(window, cx, "Year (optional)", &year),
            error: None,
        }
    }
}
