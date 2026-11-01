//! The Greeting Message and Away Message screens (tdesktop
//! `settings_greeting.cpp`, `settings_away_message.cpp`).

use super::{AwayDraft, GreetingDraft, Leave, MessageKind, ScheduleChoice};
use crate::ui::app::QuillApp;
use crate::ui::quick_replies::View as QuickRepliesView;
use crate::ui::scheduled::{TimePicker, picked_unix};
use gpui_kit::component::date_picker::{DatePicker, DatePickerState};
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::time_field::{HourCycle, TimePrecision};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::business_settings::{
    self as bs, AwaySchedule, AwaySettings, BusinessRecipients, GreetingSettings,
};
use quill::lang::{keys, tr};
use quill::local_time::now_unix;
use quill::quick_replies::count_label;

fn greeting_about() -> &'static str {
    tr(keys::GREETING_ABOUT)
}
fn away_about() -> &'static str {
    tr(keys::AWAY_ABOUT)
}
const PERIOD_ABOUT: &str = "Choose how many days should pass after your last interaction with \
a recipient to send them a greeting in response to their message.";
fn offline_about() -> &'static str {
    tr(keys::AWAY_OFFLINE_ONLY_ABOUT)
}

/// A date and time picker for the custom away schedule. Unlike the
/// message scheduler it allows past days (tdesktop's minimum is 2013).
fn away_picker(window: &mut Window, cx: &mut Context<QuillApp>, unix: i64) -> TimePicker {
    let state = cx.new(|cx| {
        DatePickerState::new(window, cx)
            .time_precision(TimePrecision::Minute)
            .hour_cycle(HourCycle::H23)
    });
    TimePicker::open(state, unix, window, cx)
}

impl QuillApp {
    /// The shortcut of a greeting or away message: its id and how many
    /// messages it has.
    pub(super) fn business_shortcut(&self, kind: MessageKind) -> (Option<i32>, i32) {
        self.session()
            .and_then(|s| s.settings.quick_replies.by_name(kind.shortcut()))
            .map_or((None, 0), |s| (Some(s.id), s.message_count))
    }

    /// tdesktop `_canHave`.
    fn business_can_enable(&self, kind: MessageKind) -> bool {
        let Some(state) = self.session().map(|s| &s.settings.quick_replies) else {
            return true;
        };
        let used = state
            .list()
            .into_iter()
            .filter(|s| s.message_count > 0)
            .count();
        let own = self.business_shortcut(kind).1 > 0;
        bs::can_enable(used, state.shortcut_limit(), own)
    }

    pub(super) fn new_greeting_draft(&self) -> GreetingDraft {
        match self.my_business().greeting {
            Some(saved) => GreetingDraft {
                enabled: true,
                recipients: saved.recipients.make_valid(),
                days: saved.inactivity_days,
            },
            None => GreetingDraft {
                enabled: false,
                recipients: BusinessRecipients::everyone(),
                days: bs::DEFAULT_INACTIVITY_DAYS,
            },
        }
    }

    pub(super) fn new_away_draft(&self, window: &mut Window, cx: &mut Context<Self>) -> AwayDraft {
        let saved = self.my_business().away;
        let (start, end) =
            bs::initial_custom_interval(saved.as_ref().map(|s| s.schedule), now_unix());
        let schedule = match saved.as_ref().map(|s| s.schedule) {
            Some(AwaySchedule::OutsideOpeningHours) => ScheduleChoice::OutsideHours,
            Some(AwaySchedule::Custom { .. }) => ScheduleChoice::Custom,
            _ => ScheduleChoice::Always,
        };
        AwayDraft {
            enabled: saved.is_some(),
            recipients: saved
                .as_ref()
                .map_or_else(BusinessRecipients::everyone, |s| {
                    s.recipients.clone().make_valid()
                }),
            schedule,
            start: away_picker(window, cx, start),
            end: away_picker(window, cx, end),
            offline_only: saved.is_some_and(|s| s.offline_only),
        }
    }

    pub(super) fn set_business_enabled(
        &mut self,
        kind: MessageKind,
        on: bool,
        cx: &mut Context<Self>,
    ) {
        if on && !self.business_can_enable(kind) {
            self.business_error(bs::limit_reached_line(kind.setting()), cx);
            return;
        }
        if let Some(d) = self.business_mut() {
            match kind {
                MessageKind::Greeting => {
                    if let Some(g) = d.greeting.as_mut() {
                        g.enabled = on;
                    }
                }
                MessageKind::Away => {
                    if let Some(a) = d.away.as_mut() {
                        a.enabled = on;
                    }
                }
            }
            d.local_error = None;
        }
        cx.notify();
    }

    /// "Create a Greeting Message" / "Edit messages": the quick reply
    /// editor on the shortcut, above this dialog.
    pub(super) fn edit_business_messages(
        &mut self,
        kind: MessageKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (id, _) = self.business_shortcut(kind);
        self.open_quick_replies(window, cx);
        self.quick_replies_go(
            QuickRepliesView::Shortcut {
                name: kind.shortcut().to_string(),
                id,
            },
            window,
            cx,
        );
    }

    fn business_switch_row(
        &self,
        id: &'static str,
        label: &'static str,
        on: bool,
        cx: &mut Context<Self>,
        toggle: impl Fn(&mut QuillApp, bool, &mut Context<QuillApp>) + 'static,
    ) -> Div {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .py_1()
            .child(div().text_sm().child(label))
            .child(
                Switch::new(id)
                    .checked(on)
                    .accessibility_label(label)
                    .on_click(cx.listener(move |this, &next: &bool, _, cx| {
                        toggle(this, next, cx);
                    })),
            )
    }

    fn business_messages_row(&self, kind: MessageKind, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let (_, count) = self.business_shortcut(kind);
        let label = match (count > 0, kind) {
            (true, _) => tr(keys::BUSINESS_EDIT_MESSAGES),
            (false, MessageKind::Greeting) => tr(keys::GREETING_CREATE),
            (false, MessageKind::Away) => tr(keys::AWAY_CREATE),
        };
        crate::ui::pressable::action_row(
            match kind {
                MessageKind::Greeting => "business-greeting-messages",
                MessageKind::Away => "business-away-messages",
            },
            None,
            label,
            false,
            cx,
        )
        .text_color(cx.theme().primary)
        .justify_between()
        .when(count > 0, |row| {
            row.child(div().text_color(muted).child(count_label(count)))
        })
        .on_click(cx.listener(move |this, _, window, cx| {
            this.edit_business_messages(kind, window, cx);
        }))
        .into_any_element()
    }

    pub(super) fn business_greeting_body(&mut self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let Some((enabled, days)) = self
            .dialogs
            .business_dialog
            .as_ref()
            .and_then(|d| d.greeting.as_ref())
            .map(|g| (g.enabled, g.days))
        else {
            return div();
        };
        let mut body = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_sm().text_color(muted).child(greeting_about()))
            .child(self.business_switch_row(
                "business-greeting-enable",
                tr(keys::Q_OPS_SEND_GREETING_MESSAGE),
                enabled,
                cx,
                |this, on, cx| this.set_business_enabled(MessageKind::Greeting, on, cx),
            ));
        if !enabled {
            return body;
        }
        body = body
            .child(self.business_messages_row(MessageKind::Greeting, cx))
            .child(self.business_recipients_section(super::Recipients::Greeting, cx));
        let selected = bs::INACTIVITY_DAYS.iter().position(|d| *d == days);
        body.child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .child(tr(keys::GREETING_PERIOD_TITLE)),
                )
                .child(
                    RadioGroup::horizontal("business-greeting-period")
                        .selected_index(selected)
                        .children(bs::INACTIVITY_DAYS.iter().map(|d| {
                            Radio::new(SharedString::from(format!("business-greeting-days-{d}")))
                                .label(bs::days_label(*d))
                        }))
                        .on_click(cx.listener(|this, &ix: &usize, _, cx| {
                            if let Some(g) = this.business_mut().and_then(|d| d.greeting.as_mut())
                                && let Some(&days) = bs::INACTIVITY_DAYS.get(ix.min(3))
                            {
                                g.days = days;
                            }
                            cx.notify();
                        })),
                )
                .child(div().text_xs().text_color(muted).child(PERIOD_ABOUT)),
        )
    }

    pub(super) fn business_away_body(&mut self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let Some((enabled, schedule, offline_only, start, end)) = self
            .dialogs
            .business_dialog
            .as_ref()
            .and_then(|d| d.away.as_ref())
            .map(|a| {
                (
                    a.enabled,
                    a.schedule,
                    a.offline_only,
                    a.start.clone(),
                    a.end.clone(),
                )
            })
        else {
            return div();
        };
        let body = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_sm().text_color(muted).child(away_about()))
            .child(self.business_switch_row(
                "business-away-enable",
                tr(keys::Q_OPS_SEND_AWAY_MESSAGE),
                enabled,
                cx,
                |this, on, cx| this.set_business_enabled(MessageKind::Away, on, cx),
            ));
        if !enabled {
            return body;
        }
        let choices = [
            (ScheduleChoice::Always, tr(keys::Q_OPS_SEND_ALWAYS)),
            (
                ScheduleChoice::OutsideHours,
                tr(keys::AWAY_SCHEDULE_OUTSIDE),
            ),
            (ScheduleChoice::Custom, tr(keys::Q_OPS_CUSTOM_SCHEDULE)),
        ];
        let selected = choices.iter().position(|(c, _)| *c == schedule);
        let mut schedule_section = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .child(tr(keys::Q_OPS_SCHEDULE)),
            )
            .child(
                RadioGroup::vertical("business-away-schedule")
                    .selected_index(selected)
                    .children(choices.iter().map(|(choice, label)| {
                        Radio::new(SharedString::from(format!("business-away-{choice:?}")))
                            .label(*label)
                    }))
                    .on_click(cx.listener(move |this, &ix: &usize, _, cx| {
                        if let Some(a) = this.business_mut().and_then(|d| d.away.as_mut())
                            && let Some(choice) = choices.get(ix.min(2))
                        {
                            a.schedule = choice.0;
                        }
                        cx.notify();
                    })),
            );
        if schedule == ScheduleChoice::Custom {
            for (label, state) in [
                (tr(keys::Q_OPS_START_TIME), start),
                (tr(keys::Q_OPS_END_TIME), end),
            ] {
                schedule_section = schedule_section.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .child(div().text_sm().child(label))
                        .child(
                            div().w(px(240.)).child(
                                DatePicker::new(&state.state)
                                    .placeholder(tr(keys::Q_OPS_PICK_A_DATE_AND_TIME)),
                            ),
                        ),
                );
            }
        }
        body.child(self.business_messages_row(MessageKind::Away, cx))
            .child(schedule_section)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(self.business_switch_row(
                        "business-away-offline",
                        tr(keys::Q_OPS_ONLY_IF_OFFLINE),
                        offline_only,
                        cx,
                        |this, on, cx| {
                            if let Some(a) = this.business_mut().and_then(|d| d.away.as_mut()) {
                                a.offline_only = on;
                            }
                            cx.notify();
                        },
                    ))
                    .child(div().text_xs().text_color(muted).child(offline_about())),
            )
            .child(self.business_recipients_section(super::Recipients::Away, cx))
    }

    /// Save of the greeting screen (when it is left).
    pub(super) fn save_business_greeting(&mut self, _cx: &mut Context<Self>) -> Leave {
        let Some(draft) = self
            .dialogs
            .business_dialog
            .as_ref()
            .and_then(|d| d.greeting.as_ref())
        else {
            return Leave::Go(None);
        };
        let (shortcut, count) = self.business_shortcut(MessageKind::Greeting);
        let wanted = match (draft.enabled, shortcut) {
            (false, _) => None,
            (true, Some(shortcut_id)) if count > 0 => Some(GreetingSettings {
                shortcut_id,
                recipients: draft.recipients.clone(),
                inactivity_days: draft.days,
            }),
            (true, _) => {
                return Leave::Go(Some(tr(keys::Q_OPS_THE_GREETING_MESSAGE_STAYS_OFF).into()));
            }
        };
        if wanted == self.my_business().greeting {
            return Leave::Go(None);
        }
        self.business_send(tr(keys::Q_OPS_GREETING_MESSAGE_SETTINGS), |driver| {
            driver.save_business_greeting(wanted.as_ref()).is_ok()
        })
    }

    /// Save of the away screen (when it is left). A custom range that
    /// ends before it starts keeps the screen open.
    pub(super) fn save_business_away(&mut self, cx: &mut Context<Self>) -> Leave {
        let Some(draft) = self
            .dialogs
            .business_dialog
            .as_ref()
            .and_then(|d| d.away.as_ref())
        else {
            return Leave::Go(None);
        };
        let schedule = match draft.schedule {
            ScheduleChoice::Always => AwaySchedule::Always,
            ScheduleChoice::OutsideHours => AwaySchedule::OutsideOpeningHours,
            ScheduleChoice::Custom => {
                let start = picked_unix(&draft.start, cx);
                let end = picked_unix(&draft.end, cx);
                match (start, end) {
                    (Some(start), Some(end)) if !bs::bad_custom_interval(start, end) => {
                        AwaySchedule::Custom { start, end }
                    }
                    _ if !draft.enabled => AwaySchedule::Always,
                    _ => {
                        self.business_error(tr(keys::Q_OPS_THE_END_TIME_MUST_BE), cx);
                        return Leave::Stay;
                    }
                }
            }
        };
        let (shortcut, count) = self.business_shortcut(MessageKind::Away);
        let wanted = match (draft.enabled, shortcut) {
            (false, _) => None,
            (true, Some(shortcut_id)) if count > 0 => Some(AwaySettings {
                shortcut_id,
                recipients: draft.recipients.clone(),
                schedule,
                offline_only: draft.offline_only,
            }),
            (true, _) => {
                return Leave::Go(Some(tr(keys::Q_OPS_THE_AWAY_MESSAGE_STAYS_OFF).into()));
            }
        };
        if wanted == self.my_business().away {
            return Leave::Go(None);
        }
        self.business_send(tr(keys::Q_OPS_AWAY_MESSAGE_SETTINGS), |driver| {
            driver.save_business_away(wanted.as_ref()).is_ok()
        })
    }
}
