//! Settings -> Telegram Business: the hub and its screens (greeting and
//! away messages, opening hours, location, start page, links to chat,
//! chatbots), with the shared recipients picker. Rules and requests:
//! `quill::business_settings`, `quill::business_hours_edit`,
//! `quill::business_start_page`, `quill::business_chat_links` and
//! `quill::business_bots`. Behaviour follows tdesktop's
//! `settings/sections/settings_business.cpp` and `settings/business/`;
//! see `docs/decisions/codex-business-settings.md` and `-2.md`.
//!
//! Like tdesktop, a screen saves when it is left (Back, Close, Esc).
//! Quick replies keep their own dialog: the greeting and away messages
//! open it on the `hello` / `away` shortcut, above this one, and closing
//! it comes back here.
#![deny(clippy::indexing_slicing)]

use crate::ui::status_note::StatusNote;
mod build;
mod chat_links;
mod chatbots;
mod hours;
mod intro_view;
mod location;
mod messages;
mod recipients;
mod start_page;
mod state;
#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod tests;

pub(crate) use state::{
    AwayDraft, BusinessDialog, ChatbotDraft, DayRow, GreetingDraft, HoursDraft, LinkEditDraft,
    LocationDraft, MessageKind, PickerDraft, Recipients, ScheduleChoice, StartPageDraft, View,
};

use super::app::QuillApp;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use quill::business_info::BusinessInfo;
use quill::lang::{keys, tr, tr_with};
use quill::premium_hub::BusinessFeature;

/// Whether a screen may be left after its save ran.
pub(super) enum Leave {
    /// Leave; the optional line shows on the next screen.
    Go(Option<String>),
    /// Stay: a problem the person has to fix is shown.
    Stay,
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

/// A text field that keeps at most `max` characters (tdesktop's
/// `setMaxLength`).
pub(super) fn limited_input(
    window: &mut Window,
    cx: &mut Context<QuillApp>,
    placeholder: &'static str,
    value: &str,
    max: usize,
) -> Entity<InputState> {
    let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
    let start: String = value.chars().take(max).collect();
    input.update(cx, |input, cx| input.set_value(start, window, cx));
    cx.subscribe_in(
        &input,
        window,
        move |_, input, event: &InputEvent, window, cx| {
            if !matches!(event, InputEvent::Change) {
                return;
            }
            let text = input.read(cx).value().to_string();
            if text.chars().count() > max {
                let cut: String = text.chars().take(max).collect();
                input.update(cx, |input, cx| input.set_value(cut, window, cx));
            }
            cx.notify();
        },
    )
    .detach();
    input
}

impl QuillApp {
    pub(super) fn open_business(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let zone_search = live_input(window, cx, tr(keys::Q_OPS_SEARCH));
        self.dialogs.business_dialog = Some(BusinessDialog {
            view: View::Hub,
            greeting: None,
            away: None,
            hours: None,
            day_rows: Vec::new(),
            location: None,
            picker: None,
            start_page: None,
            link_edit: None,
            confirm_delete: None,
            chatbot: None,
            zone_search,
            local_error: None,
            note: None,
        });
        let me = self.session().and_then(|s| s.my_user_id);
        if let Some(live) = self.live.as_mut() {
            live.driver.session.settings.business.error = None;
            let _ = live.driver.ensure_quick_reply_shortcuts();
            // The hub's row order, and the clips of the feature previews.
            let _ = live.driver.maybe_fetch_business_features();
            let _ = live.driver.maybe_fetch_premium();
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

    /// The dialog is closing (Close, Esc, the close button or the
    /// backdrop): the open screen saves first. With `can_stay` a problem
    /// keeps the dialog open; otherwise it is reported in the status line.
    pub(super) fn close_business_saving(
        &mut self,
        can_stay: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(view) = self.dialogs.business_dialog.as_ref().map(|d| d.view) else {
            return true;
        };
        if let Leave::Stay = self.business_leave(view, window, cx) {
            if can_stay {
                return false;
            }
            if let Some(line) = self.business_mut().and_then(|d| d.local_error.take()) {
                self.connection.set_note(StatusNote::error(line));
            }
        }
        self.close_business(cx);
        true
    }

    pub(super) fn business_mut(&mut self) -> Option<&mut BusinessDialog> {
        self.dialogs.business_dialog.as_mut()
    }

    pub(super) fn business_view(&self) -> Option<View> {
        self.dialogs.business_dialog.as_ref().map(|d| d.view)
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
        let missing = self.business_draft_missing(view);
        match view {
            View::Greeting if missing => {
                let draft = self.new_greeting_draft();
                if let Some(d) = self.business_mut() {
                    d.greeting = Some(draft);
                }
            }
            View::Away if missing => {
                let draft = self.new_away_draft(window, cx);
                if let Some(d) = self.business_mut() {
                    d.away = Some(draft);
                }
            }
            View::Hours => {
                if missing {
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
            View::Location if missing => {
                let draft = self.new_location_draft(window, cx);
                if let Some(d) = self.business_mut() {
                    d.location = Some(draft);
                }
            }
            View::StartPage if missing => {
                let draft = self.new_start_page_draft(window, cx);
                if let Some(d) = self.business_mut() {
                    d.start_page = Some(draft);
                }
            }
            View::StickerPick => self.open_start_page_stickers(cx),
            View::ChatLinks => {
                if let Some(live) = self.live.as_mut() {
                    let _ = live.driver.ensure_chat_links();
                }
            }
            View::Chatbots if missing => {
                let draft = self.new_chatbot_draft(window, cx);
                if let Some(d) = self.business_mut() {
                    d.chatbot = Some(draft);
                }
                self.business_bot_watch(window, cx);
            }
            _ => {}
        }
        if let Some(d) = self.business_mut() {
            // Back on the hub, every draft is gone.
            if view == View::Hub {
                d.greeting = None;
                d.away = None;
                d.hours = None;
                d.location = None;
                d.picker = None;
                d.start_page = None;
                d.chatbot = None;
            }
            if !matches!(view, View::LinkEdit) {
                d.link_edit = None;
            }
            d.confirm_delete = None;
            d.view = view;
            d.local_error = None;
            d.note = None;
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
            View::StartPage => d.start_page.is_none(),
            View::Chatbots => d.chatbot.is_none(),
            _ => false,
        }
    }

    /// Save the screen being left.
    pub(super) fn business_leave(
        &mut self,
        view: View,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Leave {
        match view {
            View::Greeting => self.save_business_greeting(cx),
            View::Away => self.save_business_away(cx),
            View::Hours => self.save_business_hours(cx),
            View::Location => self.save_business_location(cx),
            View::StartPage => self.save_business_start_page(cx),
            View::Chatbots => self.save_business_chatbot(window, cx),
            _ => Leave::Go(None),
        }
    }

    pub(super) fn business_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = self.dialogs.business_dialog.as_ref() else {
            return;
        };
        let view = d.view;
        let parent = view.parent(d.picker.as_ref().map(|p| p.target));
        let note = if view.saves_on_leave() {
            match self.business_leave(view, window, cx) {
                Leave::Stay => return,
                Leave::Go(note) => note,
            }
        } else {
            None
        };
        if view == View::Picker
            && let Some(d) = self.business_mut()
        {
            d.picker = None;
        }
        self.business_go(parent, window, cx);
        if note.is_some()
            && let Some(d) = self.business_mut()
        {
            d.note = note;
        }
    }

    pub(super) fn business_error(&mut self, line: impl Into<String>, cx: &mut Context<Self>) {
        if let Some(d) = self.business_mut() {
            d.local_error = Some(line.into());
        }
        cx.notify();
    }

    pub(super) fn business_note(&mut self, line: impl Into<String>, cx: &mut Context<Self>) {
        if let Some(d) = self.business_mut() {
            d.note = Some(line.into());
        }
        cx.notify();
    }

    /// Send a changed setting; demo mode and a refused send leave with a
    /// line saying so.
    pub(super) fn business_send(
        &mut self,
        what: &str,
        send: impl FnOnce(&mut quill::connect::ConnectDriver<quill::connect::LiveSender>) -> bool,
    ) -> Leave {
        match self.live.as_mut() {
            None => Leave::Go(Some(Self::DEMO_LINE.into())),
            Some(live) => {
                if send(&mut live.driver) {
                    Leave::Go(None)
                } else {
                    Leave::Go(Some(tr_with(
                        keys::Q_OPS_COULD_NOT_SEND_THE,
                        &[("what", what)],
                    )))
                }
            }
        }
    }

    /// The line shown when a save cannot be sent in demo mode.
    pub(super) const DEMO_LINE: &'static str = "Business settings need a live session.";

    /// A hub row was chosen. Without Premium tdesktop shows the feature's
    /// preview to buy instead (it opens above this dialog); the chatbots
    /// row works without it, and folder tags always show their preview.
    pub(super) fn business_open_feature(
        &mut self,
        feature: BusinessFeature,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = match feature {
            BusinessFeature::GreetingMessage => Some(View::Greeting),
            BusinessFeature::AwayMessage => Some(View::Away),
            BusinessFeature::QuickReplies => None,
            BusinessFeature::OpeningHours => Some(View::Hours),
            BusinessFeature::Location => Some(View::Location),
            BusinessFeature::AccountLinks => Some(View::ChatLinks),
            BusinessFeature::StartPage => Some(View::StartPage),
            BusinessFeature::Bots => Some(View::Chatbots),
            BusinessFeature::ChatFolderTags
            | BusinessFeature::EmojiStatus
            | BusinessFeature::UpgradedStories => {
                self.open_premium_to_buy(feature, cx);
                return;
            }
        };
        if feature != BusinessFeature::Bots && !self.business_premium() {
            self.open_premium_to_buy(feature, cx);
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
