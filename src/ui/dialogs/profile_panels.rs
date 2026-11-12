use super::super::app::QuillApp;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::lang::{keys, tr};

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
    /// Whether `note_input` holds the contact's real note yet. The note is
    /// in the full user info, which can land after the box opens; until
    /// then an empty field means "not known", and saving it would clear
    /// the note (`setUserNote` and `addContact` take the whole note).
    pub(crate) note: ContactNote,
    pub(crate) share_phone: bool,
    /// The `addContact` Save waits on, and its refusal.
    pub(crate) save: super::ContactSave,
}

/// The edit-contact box's private note field, by whether it was filled from
/// the full user info. The box holds no request: the wait is the session's
/// keyed `user_full_infos` fetch (the box may share it with another ask), so
/// the upkeep reads that fetch to see the info land or fail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ContactNote {
    /// The field is not filled yet: it is empty and the note is fetched
    /// (`fetch_user_full_info`, keyed by `user_id`); the session's fetch
    /// says whether it is still on its way or failed.
    Unseeded,
    /// The fetch could not be sent or failed: the field is plain and Save
    /// keeps the stored note unless something was typed.
    Unavailable,
    /// The field was filled from the full user info (or the user typed
    /// over it), so Save sends what it shows.
    Seeded,
}

/// B10: birthday form for the current user (tdesktop's birthday box).
/// Day and month are required, the year is optional.
pub struct BirthdayDialog {
    pub(crate) day_input: Entity<TextareaState>,
    pub(crate) month_input: Entity<TextareaState>,
    pub(crate) year_input: Entity<TextareaState>,
    /// Why Save refused the fields as they were typed: drawn only while
    /// the fields still read that way, so correcting one takes it away.
    pub(crate) refusal: Option<BirthdayRefusal>,
}

/// A refused birthday: the day, month and year as typed, and why.
#[derive(Clone, Debug)]
pub(crate) struct BirthdayRefusal {
    typed: [String; 3],
    reason: &'static str,
}

impl BirthdayDialog {
    fn typed(&self, cx: &App) -> [String; 3] {
        [&self.day_input, &self.month_input, &self.year_input]
            .map(|input| input.read(cx).value().to_string())
    }

    pub(crate) fn refuse(&mut self, reason: &'static str, cx: &App) {
        self.refusal = Some(BirthdayRefusal {
            typed: self.typed(cx),
            reason,
        });
    }

    /// The refusal to draw now (none once the fields changed).
    pub(crate) fn shown_error(&self, cx: &App) -> Option<&'static str> {
        self.refusal
            .as_ref()
            .filter(|refusal| refusal.typed == self.typed(cx))
            .map(|refusal| refusal.reason)
    }
}

/// B10: the one dialog behind the profile and contact panels. Only one is
/// open at a time.
pub enum ProfileDialog {
    /// A box that sent its one request and waits for the answer: `dialog`
    /// is the box as it was (kept to come back to on a refusal),
    /// `request` the watched request (`SaveWatch`), `saved` the toast the
    /// answer earns. The answer closes it; a refusal puts `dialog` back,
    /// the error beside it; closing it stops the wait.
    Saving {
        dialog: Box<ProfileDialog>,
        request: quill::ids::RequestId,
        saved: crate::ui::status_note::StatusNote,
    },
    EditContact(EditContactDialog),
    Birthday(BirthdayDialog),
    /// Pick (or remove) the personal channel.
    PersonalChannel,
    /// Pick the tab your profile opens on.
    MainTab,
    /// The note that the number is changed in the phone apps.
    ChangePhone,
    /// Share `user_id`'s contact into a chat: the picker, then the confirm
    /// step for the chosen chat.
    ShareContact {
        user_id: i64,
        step: ChatStep,
    },
    /// Confirm setting, suggesting or resetting a contact's photo.
    PersonalPhoto {
        user_id: i64,
        mode: quill::profile_forms::PersonalPhotoMode,
        path: Option<String>,
    },
    /// Pick why a profile, group or channel photo (`file_id`) is reported.
    ReportPhoto {
        owner: quill::state::PhotoOwner,
        file_id: i32,
    },
    /// Send the game `game_short_name` of `bot_id` to a chat: the picker,
    /// then the confirm step for the chosen chat.
    ShareGame {
        bot_id: i64,
        game_short_name: String,
        step: ChatStep,
    },
    /// Add the bot `bot_id` to a group or channel: the chat picker, then
    /// the rights to grant or the confirmation.
    AddBot {
        bot_id: i64,
        invite: quill::bot_invite::Invite,
        step: AddBotStep,
    },
}

/// The step of a box that sends something to a chat the person picks. Back
/// returns to `Pick`; Send, Cancel or closing the box leave it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChatStep {
    /// The list of chats.
    Pick,
    /// Confirm sending into this chat.
    Confirm(i64),
}

impl ChatStep {
    /// The chat chosen, once there is one.
    pub(crate) fn target(self) -> Option<i64> {
        match self {
            Self::Pick => None,
            Self::Confirm(chat) => Some(chat),
        }
    }
}

/// The step of the "add a bot" box. The rights belong to the chosen chat,
/// so they exist only once one is picked.
#[derive(Clone, Copy, Debug)]
pub(crate) enum AddBotStep {
    Pick,
    Confirm {
        target: i64,
        rights: quill::telegram::envelope::ChatAdminRights,
    },
}

impl AddBotStep {
    pub(crate) fn target(&self) -> Option<i64> {
        match self {
            Self::Pick => None,
            Self::Confirm { target, .. } => Some(*target),
        }
    }
}

impl ProfileDialog {
    /// Which box this is, for a continuation that must find its own box
    /// again: the kind and the person it is about. Two Edit contact boxes
    /// for different people are different boxes.
    pub(crate) fn owner(&self) -> (std::mem::Discriminant<Self>, Option<i64>) {
        let user = match self {
            Self::EditContact(dialog) => Some(dialog.user_id),
            Self::ShareContact { user_id, .. } | Self::PersonalPhoto { user_id, .. } => {
                Some(*user_id)
            }
            _ => None,
        };
        (std::mem::discriminant(self), user)
    }
}

impl EditContactDialog {
    pub(crate) fn new(
        window: &mut Window,
        cx: &mut Context<QuillApp>,
        user_id: i64,
        first_name: &str,
        last_name: &str,
        note: Option<&str>,
    ) -> Self {
        let placeholder = match note {
            Some(_) => tr(keys::Q_OPS_NOTE_ONLY_VISIBLE_TO_YOU),
            None => tr(keys::Q_PEOPLE_LOADING),
        };
        Self {
            user_id,
            first_name_input: field(window, cx, tr(keys::Q_OPS_FIRST_NAME), first_name),
            last_name_input: field(window, cx, tr(keys::Q_OPS_LAST_NAME), last_name),
            note_input: field(window, cx, placeholder, note.unwrap_or_default()),
            note: match note {
                Some(_) => ContactNote::Seeded,
                None => ContactNote::Unseeded,
            },
            share_phone: false,
            save: super::ContactSave::Idle,
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
            day_input: field(window, cx, tr(keys::Q_OPS_DAY), &day),
            month_input: field(window, cx, tr(keys::Q_OPS_MONTH_1_12), &month),
            year_input: field(window, cx, tr(keys::Q_OPS_YEAR_OPTIONAL), &year),
            refusal: None,
        }
    }
}
