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
        self.open_box(crate::ui::shell::DialogKind::Business);
        if let Some(live) = self.live.as_mut() {
            live.driver.session.settings.business.error = None;
            let _ = live.driver.ensure_quick_reply_shortcuts();
            // The hub's row order, and the clips of the feature previews.
            let _ = live.driver.maybe_fetch_business_features();
            let _ = live.driver.maybe_fetch_premium();
            // The screens that save wait for it (`business_waits_for_info`).
            let _ = live.driver.fetch_business_own_info();
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
                self.set_note(StatusNote::error(line));
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

    /// The own account's saved business info, or `None` while its full
    /// info has not arrived. A screen's draft is built only from the saved
    /// info: one built from the empty value would show Off and the default
    /// hours, and a change on it would save those defaults over the real
    /// settings. Without an own user (the demo) nothing is waited for.
    pub(super) fn saved_business(&self) -> Option<BusinessInfo> {
        let session = self.session()?;
        let Some(me) = session.my_user_id else {
            return Some(BusinessInfo::default());
        };
        session
            .user_full_info(me)
            .map(|info| info.extras.business.clone().unwrap_or_default())
    }

    pub(super) fn business_premium(&self) -> bool {
        self.session().is_some_and(|s| s.my_is_premium())
    }

    /// Show another screen, building its draft from the saved values.
    pub(super) fn business_go(&mut self, view: View, window: &mut Window, cx: &mut Context<Self>) {
        let missing = self.business_draft_missing(view);
        match view {
            View::Greeting | View::Away | View::Location | View::StartPage | View::Hours => {
                // Before the full info arrives the screen shows "Loading…";
                // the frame upkeep builds it when the info lands
                // (`business_info_upkeep`).
                if missing {
                    self.business_fill_draft(view, window, cx);
                }
                if view == View::Hours
                    && let Some(live) = self.live.as_mut()
                {
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

    /// Build the draft of a screen that saves from the full info, once it
    /// has arrived. `false` while it is still on its way: the screen shows
    /// "Loading…" with nothing to change, so nothing can save.
    fn business_fill_draft(
        &mut self,
        view: View,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(saved) = self.saved_business() else {
            return false;
        };
        let away = (view == View::Away).then(|| Self::new_away_draft(&saved, window, cx));
        let location =
            (view == View::Location).then(|| Self::new_location_draft(&saved, window, cx));
        let start_page =
            (view == View::StartPage).then(|| self.new_start_page_draft(&saved, window, cx));
        let Some(d) = self.business_mut() else {
            return true;
        };
        match view {
            View::Greeting => d.greeting = Some(Self::new_greeting_draft(&saved)),
            View::Away => d.away = away,
            View::Hours => d.hours = Some(Self::new_hours_draft(&saved)),
            View::Location => d.location = location,
            View::StartPage => d.start_page = start_page,
            _ => {}
        }
        true
    }

    /// Frame upkeep of the window hosting the Business box: a screen
    /// waiting for the full info is built from it once it is stored. A
    /// pure function of the state: no draft and the saved value present.
    pub(in crate::ui) fn business_info_upkeep(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self.business_view() else {
            return;
        };
        if self.business_waits_for_info(view) && self.business_fill_draft(view, window, cx) {
            cx.notify();
        }
    }

    /// Why the own full info a waiting screen needs did not come, when
    /// its request failed: the screen shows it with a Retry.
    pub(super) fn business_info_error(&self) -> Option<String> {
        self.session()
            .and_then(|s| s.business_own_info().error().map(str::to_string))
    }

    /// A waiting screen's Retry: the own full info, asked again.
    pub(super) fn retry_business_info(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.fetch_business_own_info();
        }
        cx.notify();
    }

    /// Whether this screen shows "Loading…" until the full info arrives.
    pub(super) fn business_waits_for_info(&self, view: View) -> bool {
        matches!(
            view,
            View::Greeting | View::Away | View::Hours | View::Location | View::StartPage
        ) && self.business_draft_missing(view)
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
    )
    .close(|app, window, cx| {
        app.close_business_saving(false, window, cx);
    }),
}
