//! Settings -> Telegram Business: the hub and its greeting message, away
//! message, opening hours and location screens, with the shared
//! recipients picker. Rules and requests: `quill::business_settings` and
//! `quill::business_hours_edit`. Behaviour follows tdesktop's
//! `settings/sections/settings_business.cpp` and `settings/business/`;
//! see `docs/decisions/codex-business-settings.md`.
//!
//! Quick replies keep their own dialog: the greeting and away messages
//! open it on the `hello` / `away` shortcut, above this one, and closing
//! it comes back here.

mod build;
mod hours;
mod location;
mod messages;
mod recipients;
#[cfg(all(test, feature = "demo-capture"))]
mod tests;

use super::app::QuillApp;
use gpui_kit::component::date_picker::DatePickerState;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::time_field::TimeFieldState;
use gpui_kit::*;
use quill::business_hours_edit::Interval;
use quill::business_info::BusinessInfo;
use quill::business_settings::{BusinessChats, BusinessRecipients};

/// Which screen of the dialog is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum View {
    Hub,
    Greeting,
    Away,
    Hours,
    /// The editor of one weekday (Monday 0).
    Day(usize),
    TimeZone,
    Location,
    /// The include / exclude chats picker of the greeting or away screen.
    Picker,
}

impl View {
    /// Where Back goes.
    fn parent(self, picker_for: Option<MessageKind>) -> View {
        match self {
            View::Day(_) | View::TimeZone => View::Hours,
            View::Picker => match picker_for {
                Some(MessageKind::Away) => View::Away,
                _ => View::Greeting,
            },
            _ => View::Hub,
        }
    }
}

/// The greeting or the away message: both have messages in a quick reply
/// shortcut and a recipients list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MessageKind {
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

/// tdesktop's away schedule radio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ScheduleChoice {
    Always,
    OutsideHours,
    Custom,
}

pub(super) struct GreetingDraft {
    pub(super) enabled: bool,
    pub(super) recipients: BusinessRecipients,
    pub(super) days: i32,
}

pub(super) struct AwayDraft {
    pub(super) enabled: bool,
    pub(super) recipients: BusinessRecipients,
    pub(super) schedule: ScheduleChoice,
    pub(super) start: Entity<DatePickerState>,
    pub(super) end: Entity<DatePickerState>,
    pub(super) offline_only: bool,
}

pub(super) struct HoursDraft {
    pub(super) enabled: bool,
    /// Minutes of the week, normalized.
    pub(super) week: Vec<Interval>,
    /// Empty until a zone is known; the closest zone is shown then.
    pub(super) time_zone_id: String,
}

/// One set of hours in the day editor: opening and closing clock times.
pub(super) struct DayRow {
    pub(super) open: Entity<TimeFieldState>,
    pub(super) close: Entity<TimeFieldState>,
}

pub(super) struct LocationDraft {
    pub(super) address: Entity<InputState>,
    /// The saved map point, kept until it is switched off.
    pub(super) point_e6: Option<(i32, i32)>,
}

pub(super) struct PickerDraft {
    pub(super) target: MessageKind,
    pub(super) include: bool,
    pub(super) chats: BusinessChats,
    pub(super) search: Entity<InputState>,
}

pub(crate) struct BusinessDialog {
    pub(super) view: View,
    pub(super) greeting: Option<GreetingDraft>,
    pub(super) away: Option<AwayDraft>,
    pub(super) hours: Option<HoursDraft>,
    pub(super) day_rows: Vec<DayRow>,
    pub(super) location: Option<LocationDraft>,
    pub(super) picker: Option<PickerDraft>,
    pub(super) zone_search: Entity<InputState>,
    pub(super) local_error: Option<String>,
}

/// A text field whose edits re-render the dialog (search filters, the
/// address counter).
pub(super) fn live_input(
    window: &mut Window,
    cx: &mut Context<QuillApp>,
    placeholder: &'static str,
) -> Entity<InputState> {
    let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
    cx.subscribe(&input, |_, _, event: &InputEvent, cx| {
        if matches!(event, InputEvent::Change) {
            cx.notify();
        }
    })
    .detach();
    input
}

impl QuillApp {
    pub(super) fn open_business(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let zone_search = live_input(window, cx, "Search");
        self.dialogs.business_dialog = Some(BusinessDialog {
            view: View::Hub,
            greeting: None,
            away: None,
            hours: None,
            day_rows: Vec::new(),
            location: None,
            picker: None,
            zone_search,
            local_error: None,
        });
        let me = self.session().and_then(|s| s.my_user_id);
        if let Some(live) = self.live.as_mut() {
            live.driver.session.settings.business.error = None;
            let _ = live.driver.ensure_quick_reply_shortcuts();
            if let Some(me) = me {
                let _ = live.driver.fetch_user_full_info(me);
            }
        }
        cx.notify();
    }

    pub(super) fn close_business(&mut self, cx: &mut Context<Self>) {
        if self.dialogs.business_dialog.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn business_mut(&mut self) -> Option<&mut BusinessDialog> {
        self.dialogs.business_dialog.as_mut()
    }

    /// The own account's business info (empty when none is set or the
    /// full info has not arrived).
    pub(super) fn my_business(&self) -> BusinessInfo {
        self.session()
            .and_then(|s| s.my_user_id.and_then(|me| s.user_full_info(me)))
            .and_then(|info| info.extras.business.clone())
            .unwrap_or_default()
    }

    pub(super) fn business_premium(&self) -> bool {
        self.session().is_some_and(|s| s.my_is_premium())
    }

    /// Show another screen, building its draft from the saved values.
    pub(super) fn business_go(&mut self, view: View, window: &mut Window, cx: &mut Context<Self>) {
        match view {
            View::Greeting if self.business_draft_missing(view) => {
                let draft = self.new_greeting_draft();
                if let Some(d) = self.business_mut() {
                    d.greeting = Some(draft);
                }
            }
            View::Away if self.business_draft_missing(view) => {
                let draft = self.new_away_draft(window, cx);
                if let Some(d) = self.business_mut() {
                    d.away = Some(draft);
                }
            }
            View::Hours => {
                if self.business_draft_missing(view) {
                    let draft = self.new_hours_draft();
                    if let Some(d) = self.business_mut() {
                        d.hours = Some(draft);
                    }
                }
                if let Some(live) = self.live.as_mut() {
                    let _ = live.driver.ensure_time_zones();
                }
            }
            View::Day(day) => self.begin_day_edit(day, window, cx),
            View::TimeZone => {
                if let Some(d) = self.business_mut() {
                    let search = d.zone_search.clone();
                    search.update(cx, |input, cx| input.set_value("", window, cx));
                }
            }
            View::Location if self.business_draft_missing(view) => {
                let draft = self.new_location_draft(window, cx);
                if let Some(d) = self.business_mut() {
                    d.location = Some(draft);
                }
            }
            _ => {}
        }
        if let Some(d) = self.business_mut() {
            // Leaving an editor for the hub drops its unsaved draft.
            if view == View::Hub {
                d.greeting = None;
                d.away = None;
                d.hours = None;
                d.location = None;
                d.picker = None;
            }
            d.view = view;
            d.local_error = None;
        }
        cx.notify();
    }

    fn business_draft_missing(&self, view: View) -> bool {
        let Some(d) = self.dialogs.business_dialog.as_ref() else {
            return false;
        };
        match view {
            View::Greeting => d.greeting.is_none(),
            View::Away => d.away.is_none(),
            View::Hours => d.hours.is_none(),
            View::Location => d.location.is_none(),
            _ => false,
        }
    }

    pub(super) fn business_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = self.dialogs.business_dialog.as_ref() else {
            return;
        };
        let parent = d.view.parent(d.picker.as_ref().map(|p| p.target));
        if d.view == View::Picker
            && let Some(d) = self.business_mut()
        {
            d.picker = None;
        }
        self.business_go(parent, window, cx);
    }

    pub(super) fn business_error(&mut self, line: impl Into<String>, cx: &mut Context<Self>) {
        if let Some(d) = self.business_mut() {
            d.local_error = Some(line.into());
        }
        cx.notify();
    }

    /// Saving needs a live session; demo mode says so.
    pub(super) fn business_live(&mut self, cx: &mut Context<Self>) -> bool {
        if self.live.is_some() {
            return true;
        }
        self.business_error("Business settings need a live session.", cx);
        false
    }

    /// A hub row was chosen. Without Premium tdesktop shows the Premium
    /// screen instead (it opens above this dialog).
    pub(super) fn business_open_feature(
        &mut self,
        view: Option<View>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.business_premium() {
            self.open_premium(cx);
            return;
        }
        match view {
            Some(view) => self.business_go(view, window, cx),
            None => self.open_quick_replies(window, cx),
        }
    }
}

crate::ui::shell::register_dialogs! {
    Business => DialogSpec::new(
        5438,
        |app| app.dialogs.business_dialog.is_some(),
        QuillApp::build_business_dialog,
    ),
}
