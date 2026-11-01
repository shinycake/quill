//! The Business dialog's screens and the unsaved drafts they edit.

use crate::ui::scheduled::TimePicker;
use gpui_kit::component::input::{InputState, TextareaState};
use gpui_kit::component::time_field::TimeFieldState;
use gpui_kit::*;
use quill::business_bots::BotRights;
use quill::business_hours_edit::Interval;
use quill::business_settings::{BusinessChats, BusinessRecipients};
use quill::telegram::envelope::StickerItem;

/// Which screen of the dialog is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum View {
    Hub,
    Greeting,
    Away,
    Hours,
    /// The editor of one weekday (Monday 0).
    Day(usize),
    TimeZone,
    Location,
    /// The include / exclude chats picker of a recipients list.
    Picker,
    StartPage,
    /// The sticker panel choosing the start page sticker.
    StickerPick,
    ChatLinks,
    /// The new / edit link box.
    LinkEdit,
    Chatbots,
}

impl View {
    /// Where Back goes.
    pub(super) fn parent(self, picker_for: Option<Recipients>) -> View {
        match self {
            View::Day(_) | View::TimeZone => View::Hours,
            View::Picker => match picker_for {
                Some(Recipients::Away) => View::Away,
                Some(Recipients::Bot) => View::Chatbots,
                _ => View::Greeting,
            },
            View::StickerPick => View::StartPage,
            View::LinkEdit => View::ChatLinks,
            _ => View::Hub,
        }
    }

    /// Screens that save when they are left (tdesktop saves a section in
    /// its destructor).
    pub(super) fn saves_on_leave(self) -> bool {
        matches!(
            self,
            View::Greeting
                | View::Away
                | View::Hours
                | View::Location
                | View::StartPage
                | View::Chatbots
        )
    }
}

/// The greeting or the away message: both have messages in a quick reply
/// shortcut and a recipients list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MessageKind {
    Greeting,
    Away,
}

impl MessageKind {
    pub(super) fn shortcut(self) -> &'static str {
        match self {
            Self::Greeting => quill::business_settings::GREETING_SHORTCUT,
            Self::Away => quill::business_settings::AWAY_SHORTCUT,
        }
    }

    pub(super) fn setting(self) -> quill::business_settings::BusinessSettingKind {
        match self {
            Self::Greeting => quill::business_settings::BusinessSettingKind::Greeting,
            Self::Away => quill::business_settings::BusinessSettingKind::Away,
        }
    }
}

/// Whose recipients a list edits. The bot's also has exclusions in "Only
/// Selected Chats" (tdesktop `BusinessRecipientsType::Bots`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Recipients {
    Greeting,
    Away,
    Bot,
}

impl From<MessageKind> for Recipients {
    fn from(kind: MessageKind) -> Self {
        match kind {
            MessageKind::Greeting => Recipients::Greeting,
            MessageKind::Away => Recipients::Away,
        }
    }
}

/// tdesktop's away schedule radio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScheduleChoice {
    Always,
    OutsideHours,
    Custom,
}

pub(crate) struct GreetingDraft {
    pub(super) enabled: bool,
    pub(super) recipients: BusinessRecipients,
    pub(super) days: i32,
}

pub(crate) struct AwayDraft {
    pub(super) enabled: bool,
    pub(super) recipients: BusinessRecipients,
    pub(super) schedule: ScheduleChoice,
    pub(super) start: TimePicker,
    pub(super) end: TimePicker,
    pub(super) offline_only: bool,
}

pub(crate) struct HoursDraft {
    pub(super) enabled: bool,
    /// Minutes of the week, normalized.
    pub(super) week: Vec<Interval>,
    /// Empty until a zone is known; the closest zone is shown then.
    pub(super) time_zone_id: String,
}

/// One set of hours in the day editor: opening and closing clock times.
pub(crate) struct DayRow {
    pub(super) open: Entity<TimeFieldState>,
    pub(super) close: Entity<TimeFieldState>,
}

pub(crate) struct LocationDraft {
    pub(super) address: Entity<InputState>,
    /// The saved map point, kept until it is switched off.
    pub(super) point_e6: Option<(i32, i32)>,
}

pub(crate) struct PickerDraft {
    pub(super) target: Recipients,
    pub(super) include: bool,
    pub(super) chats: BusinessChats,
    pub(super) search: Entity<InputState>,
}

pub(crate) struct StartPageDraft {
    pub(super) title: Entity<InputState>,
    pub(super) message: Entity<InputState>,
    pub(super) sticker: Option<StickerItem>,
}

pub(crate) struct LinkEditDraft {
    /// `None` for a new link.
    pub(super) link: Option<String>,
    pub(super) text: Entity<TextareaState>,
    pub(super) name: Entity<InputState>,
}

pub(crate) struct ChatbotDraft {
    pub(super) username: Entity<InputState>,
    /// The connected (or just added) bot; the username field shows when
    /// there is none.
    pub(super) committed: Option<i64>,
    pub(super) recipients: BusinessRecipients,
    pub(super) rights: BotRights,
    /// Whether the server's bot was known when the draft was made.
    pub(super) loaded: bool,
    /// tdesktop's warning box after turning on a risky right.
    pub(super) warning: Option<String>,
    /// "No Bot Added": leaving while found bots were not added.
    pub(super) confirm_leave: bool,
}

pub(crate) struct BusinessDialog {
    pub(super) view: View,
    pub(super) greeting: Option<GreetingDraft>,
    pub(super) away: Option<AwayDraft>,
    pub(super) hours: Option<HoursDraft>,
    pub(super) day_rows: Vec<DayRow>,
    pub(super) location: Option<LocationDraft>,
    pub(super) picker: Option<PickerDraft>,
    pub(super) start_page: Option<StartPageDraft>,
    pub(super) link_edit: Option<LinkEditDraft>,
    /// The link waiting for "Delete" to be confirmed.
    pub(super) confirm_delete: Option<String>,
    pub(super) chatbot: Option<ChatbotDraft>,
    pub(super) zone_search: Entity<InputState>,
    pub(super) local_error: Option<String>,
    /// A passing confirmation ("Chat link copied to clipboard.").
    pub(super) note: Option<String>,
}
