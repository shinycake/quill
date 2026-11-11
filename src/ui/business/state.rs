//! The Business dialog's screens and the unsaved drafts they edit.

use crate::ui::scheduled::TimePicker;
use gpui_kit::component::input::{InputState, TextareaState};
use gpui_kit::component::time_field::TimeFieldState;
use gpui_kit::*;
use quill::business_bots::BotRights;
use quill::business_hours_edit::Interval;
use quill::business_info::{BusinessLocation, OpeningHours};
use quill::business_settings::{AwaySettings, BusinessChats, BusinessRecipients, GreetingSettings};
use quill::business_start_page::StartPage;
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
    /// Whether this screen draws a refusal of `area` (the hub draws all of
    /// them: a save refused after the screen was left shows there).
    pub(crate) fn shows(self, area: quill::state::BusinessArea) -> bool {
        use quill::business_settings::BusinessSettingKind as Kind;
        use quill::state::BusinessArea as Area;
        match (self, area) {
            (View::Hub, _) => true,
            (View::Greeting, Area::Save(Kind::Greeting))
            | (View::Away, Area::Save(Kind::Away))
            | (View::Hours | View::Day(_) | View::TimeZone, Area::Save(Kind::OpeningHours))
            | (View::Location, Area::Save(Kind::Location))
            | (View::StartPage | View::StickerPick, Area::Save(Kind::StartPage))
            | (View::ChatLinks | View::LinkEdit, Area::ChatLinks)
            | (View::Chatbots, Area::Chatbot) => true,
            (View::Picker, Area::Save(Kind::Greeting | Kind::Away)) => true,
            _ => false,
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

// Each screen's draft keeps the saved value it was built from (`base`).
// Leaving saves only when the draft differs from that value, never from
// the server's value now: a screen left untouched writes nothing, even
// when the full info arrived (or changed on another device) after it was
// built. tdesktop never writes a section that was not changed.

pub(crate) struct GreetingDraft {
    pub(super) base: Option<GreetingSettings>,
    pub(super) enabled: bool,
    pub(super) recipients: BusinessRecipients,
    pub(super) days: i32,
}

pub(crate) struct AwayDraft {
    pub(super) base: Option<AwaySettings>,
    pub(super) enabled: bool,
    pub(super) recipients: BusinessRecipients,
    pub(super) schedule: ScheduleChoice,
    pub(super) start: TimePicker,
    pub(super) end: TimePicker,
    pub(super) offline_only: bool,
}

pub(crate) struct HoursDraft {
    pub(super) base: Option<OpeningHours>,
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
    pub(super) base: Option<BusinessLocation>,
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
    pub(super) base: StartPage,
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

/// Where the connected bot stands: not known yet, none, or connected (or
/// just added).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BotLink {
    /// The server's bot has not been reported; nothing can save.
    Loading,
    /// The username field shows.
    Empty,
    Connected(i64),
}

impl BotLink {
    pub(super) fn is_loaded(self) -> bool {
        self != Self::Loading
    }

    pub(super) fn connected(self) -> Option<i64> {
        match self {
            Self::Connected(id) => Some(id),
            _ => None,
        }
    }
}

/// The box over the Chatbots screen, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BotNotice {
    None,
    /// tdesktop's warning box after turning on a risky right.
    Warning(String),
    /// "No Bot Added": leaving while found bots were not added.
    ConfirmLeave,
}

pub(crate) struct ChatbotDraft {
    pub(super) username: Entity<InputState>,
    pub(super) bot: BotLink,
    pub(super) recipients: BusinessRecipients,
    pub(super) rights: BotRights,
    pub(super) notice: BotNotice,
}

/// The one line under the screen: a refusal or a passing confirmation
/// ("Chat link copied to clipboard."). A new one replaces the old.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) enum Line {
    #[default]
    None,
    Error(String),
    Note(String),
}

pub(crate) struct BusinessDialog {
    pub(super) screen: super::screen::Screen,
    pub(super) zone_search: Entity<InputState>,
    pub(super) line: Line,
}

impl BusinessDialog {
    pub(super) fn view(&self) -> View {
        self.screen.view()
    }

    pub(super) fn local_error(&self) -> Option<&str> {
        match &self.line {
            Line::Error(line) => Some(line),
            _ => None,
        }
    }

    pub(super) fn note(&self) -> Option<&str> {
        match &self.line {
            Line::Note(line) => Some(line),
            _ => None,
        }
    }

    /// Drop a refusal; a confirmation stays.
    pub(super) fn clear_error(&mut self) {
        if matches!(self.line, Line::Error(_)) {
            self.line = Line::None;
        }
    }

    pub(super) fn take_error(&mut self) -> Option<String> {
        match std::mem::take(&mut self.line) {
            Line::Error(line) => Some(line),
            other => {
                self.line = other;
                None
            }
        }
    }
}
