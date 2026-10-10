//! Scheduled messages: the date+time picker popup above the composer, the
//! scheduled-messages dialog (send now, reschedule, edit, delete) and the
//! composer's scheduled-messages button.
//!
//! Mirrors tdesktop's `ScheduleBox` / `ChooseDateTimeBox` and
//! `HistoryView::ScheduledMemento` (see
//! `docs/decisions/codex-scheduled-messages.md`).

use super::app::QuillApp;
use super::message_text::format_unix_date_time;
use super::shell::DialogKind;
use super::shell::QuillShell;
use gpui_kit::component::button::*;
use gpui_kit::component::calendar::Matcher;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::date_picker::{DatePicker, DatePickerState, DateRangePreset, DateTime};
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::time_field::{HourCycle, TimePrecision};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::composer::{ComposerEdit, ComposerScheduling};
use quill::ids::MessageId;
use quill::local_time::{civil_local, now_unix};
use quill::schedule::{
    MAX_SCHEDULE_SECS, ScheduleError, ScheduleKind, civil_to_unix, default_schedule_time,
    parse_picker_stamp, validate_send_date,
};
use quill::telegram::envelope::{
    ChatKind, MessageSchedulingState, ParsedMessage, effective_content,
};
use std::cell::RefCell;
use std::rc::Rc;

/// M1: label for a scheduled message's planned send time
/// (`messageSchedulingState`, schema 1.8.67 lines 5902/5905).
pub(super) fn scheduled_message_label(message: &ParsedMessage, kind: ScheduleKind) -> String {
    let noun = match kind {
        ScheduleKind::Reminder => "reminder",
        ScheduleKind::Schedule => "scheduled",
    };
    match message.scheduling_state {
        Some(MessageSchedulingState::SendAtDate { send_date }) => {
            format!("{noun} for {}", format_unix_date_time(send_date as i64))
        }
        Some(MessageSchedulingState::SendWhenOnline) => "send when online".to_string(),
        None => noun.to_string(),
    }
}

/// What a confirmed picker time applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ScheduleTarget {
    /// The composer's next send.
    Composer,
    /// An already scheduled message (`editMessageSchedulingState`).
    Reschedule(MessageId),
    /// Every message ticked in the scheduled-messages dialog.
    RescheduleSelected,
    /// The share box's destinations (`forwardMessages` scheduling state).
    Share,
}

/// The date+time picker popup (tdesktop's `ChooseDateTimeBox`).
pub(super) struct SchedulePicker {
    pub target: ScheduleTarget,
    pub date: Entity<DatePickerState>,
    pub error: Option<&'static str>,
}

/// `YYYY-MM-DDTHH:MM:00` of a unix time in the local zone, the shape
/// `NaiveDateTime` parses (the kit does not re-export chrono).
fn local_stamp(unix: i64) -> String {
    let c = civil_local(unix);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:00",
        c.year, c.month, c.day, c.hour, c.minute
    )
}

pub(super) fn picker_value(unix: i64) -> Option<DateTime> {
    Some(DateTime::Single(Some(local_stamp(unix).parse().ok()?)))
}

/// Local calendar day of `unix` as `YYYY-MM-DD`; compares lexically.
pub(super) fn local_day(unix: i64) -> String {
    let c = civil_local(unix);
    format!("{:04}-{:02}-{:02}", c.year, c.month, c.day)
}

/// A date+time picker that opens at `initial` and allows days from today
/// to `max_ahead_secs` ahead. Shared by the schedule popup and the
/// restrict-until picker of the member dialogs.
pub(super) fn new_date_time_picker(
    window: &mut Window,
    cx: &mut Context<QuillApp>,
    initial: i64,
    max_ahead_secs: i64,
) -> Entity<DatePickerState> {
    let now = now_unix();
    // Days outside [today, today + max_ahead] cannot be picked.
    let first_day = local_day(now);
    let last_day = local_day(now + max_ahead_secs);
    let date = cx.new(|cx| {
        DatePickerState::new(window, cx)
            .time_precision(TimePrecision::Minute)
            .hour_cycle(HourCycle::H23)
            .disabled_matcher(Matcher::custom(move |day| {
                let day = day.format("%Y-%m-%d").to_string();
                day < first_day || day > last_day
            }))
    });
    if let Some(value) = picker_value(initial) {
        date.update(cx, |state, cx| state.set_date_time(value, window, cx));
    }
    date
}

/// The unix time a date picker currently shows (local zone), if it holds
/// a complete date and time.
pub(super) fn picked_unix(date: &Entity<DatePickerState>, cx: &App) -> Option<i64> {
    date.read(cx)
        .date_time()
        .start()
        .and_then(|value| parse_picker_stamp(&value.format("%Y-%m-%d %H:%M").to_string()))
        .map(|civil| civil_to_unix(&civil))
}

/// B15: the poll dialog's absolute-deadline picker (`close_date`): days
/// from today to a year ahead, opening one day out, like tdesktop's
/// `ChooseDateTimeBox` with `min = now + 60s`, `max = now + 365d`.
pub(super) fn poll_deadline_picker(
    window: &mut Window,
    cx: &mut Context<QuillApp>,
) -> Entity<DatePickerState> {
    new_date_time_picker(
        window,
        cx,
        now_unix() + 24 * 3600,
        quill::poll::POLL_DEADLINE_MAX_SECS,
    )
}

impl QuillApp {
    /// Reminder wording in Saved Messages (the chat with yourself),
    /// schedule wording everywhere else (`SendMenu::Type::Reminder`).
    pub(super) fn schedule_kind(&self) -> ScheduleKind {
        let saved = self
            .session()
            .and_then(|s| s.open_chat.map(|id| s.is_saved_messages(id)))
            .unwrap_or(false);
        ScheduleKind::for_saved_messages(saved)
    }

    /// "Send when online" is offered in one-to-one chats with another
    /// person: not Saved Messages, not a bot (tdesktop's
    /// `CanScheduleUntilOnline`; the last-seen privacy check is left to the
    /// server, which rejects the request for hidden last-seen).
    pub(super) fn can_send_when_online(&self) -> bool {
        // The share box schedules to several chats at once.
        if self
            .schedule_picker
            .as_ref()
            .is_some_and(|picker| picker.target == ScheduleTarget::Share)
        {
            return false;
        }
        let Some(session) = self.session() else {
            return false;
        };
        let Some(chat) = session.open_chat.and_then(|id| session.chats.get(&id.0)) else {
            return false;
        };
        match chat.kind {
            ChatKind::Private { user_id } => {
                !session.is_saved_messages(chat.id) && !session.is_bot_user(user_id.0)
            }
            _ => false,
        }
    }

    /// Opens the picker above the composer at the target's current time
    /// (or ten minutes from now).
    pub(super) fn open_schedule_picker(
        &mut self,
        target: ScheduleTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let now = now_unix();
        let existing = match target {
            ScheduleTarget::Composer => match self.composer_scheduling {
                ComposerScheduling::SendAtDate(date) => Some(date),
                _ => None,
            },
            ScheduleTarget::Share | ScheduleTarget::RescheduleSelected => None,
            ScheduleTarget::Reschedule(id) => self.session().and_then(|s| {
                s.scheduled_messages
                    .iter()
                    .find(|m| m.id == id)
                    .and_then(|m| match m.scheduling_state {
                        Some(MessageSchedulingState::SendAtDate { send_date }) => {
                            Some(i64::from(send_date))
                        }
                        _ => None,
                    })
            }),
        };
        let initial = existing
            .filter(|date| validate_send_date(*date, now).is_ok())
            .unwrap_or_else(|| default_schedule_time(now));
        let date = new_date_time_picker(window, cx, initial, MAX_SCHEDULE_SECS);
        self.schedule_picker = Some(SchedulePicker {
            target,
            date,
            error: None,
        });
        self.schedule_popup_open = true;
        cx.notify();
    }

    /// Closes the picker; a reschedule returns to the scheduled list.
    pub(super) fn close_schedule_picker(&mut self, cx: &mut Context<Self>) {
        let reopen = matches!(
            self.schedule_picker.as_ref().map(|p| p.target),
            Some(ScheduleTarget::Reschedule(_) | ScheduleTarget::RescheduleSelected)
        );
        self.schedule_popup_open = false;
        self.schedule_picker = None;
        if reopen {
            self.open_scheduled_dialog(cx);
        }
        cx.notify();
    }

    /// Reads the picker, validates it like `ChooseDateTimeBox::collect`
    /// and applies it.
    pub(super) fn confirm_schedule_picker(&mut self, cx: &mut Context<Self>) {
        let Some(picker) = self.schedule_picker.as_ref() else {
            return;
        };
        let target = picker.target;
        let picked = picked_unix(&picker.date, cx)
            .ok_or(ScheduleError::Invalid)
            .and_then(|unix| validate_send_date(unix, now_unix()));
        match picked {
            Ok(send_date) => {
                self.apply_schedule(target, ComposerScheduling::SendAtDate(send_date), cx)
            }
            Err(err) => {
                if let Some(picker) = self.schedule_picker.as_mut() {
                    picker.error = Some(err.message());
                }
                cx.notify();
            }
        }
    }

    /// Applies a chosen send time to the composer or to a scheduled message.
    pub(super) fn apply_schedule(
        &mut self,
        target: ScheduleTarget,
        scheduling: ComposerScheduling,
        cx: &mut Context<Self>,
    ) {
        let kind = self.schedule_kind();
        self.schedule_popup_open = false;
        self.schedule_picker = None;
        match target {
            ScheduleTarget::Composer => {
                self.composer_scheduling = scheduling;
                self.status_note = match (scheduling, kind) {
                    (ComposerScheduling::SendAtDate(date), ScheduleKind::Reminder) => {
                        format!("reminder set for {}", format_unix_date_time(date))
                    }
                    (ComposerScheduling::SendAtDate(date), ScheduleKind::Schedule) => {
                        format!("scheduled for {}", format_unix_date_time(date))
                    }
                    _ => "will send when the contact is online".into(),
                };
                cx.notify();
            }
            ScheduleTarget::Reschedule(message_id) => {
                self.edit_scheduled_state(message_id, scheduling, cx);
                self.open_scheduled_dialog(cx);
            }
            ScheduleTarget::RescheduleSelected => {
                for id in std::mem::take(&mut self.scheduled_selected) {
                    self.edit_scheduled_state(id, scheduling, cx);
                }
                self.open_scheduled_dialog(cx);
            }
            ScheduleTarget::Share => {
                self.submit_share(quill::share_box::ShareSend::Scheduled(scheduling), cx);
            }
        }
    }

    /// `editMessageSchedulingState` for a scheduled message: a new time, or
    /// `ComposerScheduling::None` to send it now.
    pub(super) fn edit_scheduled_state(
        &mut self,
        message_id: MessageId,
        scheduling: ComposerScheduling,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let Some(chat_id) = live.driver.session.open_chat else {
            return;
        };
        let sending_now = scheduling == ComposerScheduling::None;
        match live
            .driver
            .edit_scheduled_message(chat_id, message_id, scheduling)
        {
            Ok(_) => {
                self.status_note = if sending_now {
                    "sending…".into()
                } else {
                    "rescheduling…".into()
                };
            }
            Err(_) => {
                self.status_note = if sending_now {
                    "could not send the message now".into()
                } else {
                    "could not reschedule the message".into()
                };
            }
        }
        cx.notify();
    }

    /// The date+time picker popup above the composer (tdesktop's
    /// `ScheduleBox`): title, date and time field, "Send when online" where
    /// the chat allows it, Cancel and Schedule/Remind.
    pub(super) fn schedule_popup(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(picker) = self.schedule_picker.as_ref() else {
            return div().into_any_element();
        };
        let kind = self.schedule_kind();
        let now = now_unix();
        let presets: Vec<DateRangePreset> = [
            ("In 1 hour", 3600),
            ("In 8 hours", 8 * 3600),
            ("In 24 hours", 24 * 3600),
        ]
        .into_iter()
        .filter_map(|(label, secs)| {
            picker_value(now + secs).map(|value| DateRangePreset::date_time(label, value))
        })
        .collect();
        let target = picker.target;
        let mut footer = div().flex().items_center().gap_2();
        if self.can_send_when_online() {
            footer = footer.child(
                Button::new("schedule-when-online")
                    .label("Send when online")
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.apply_schedule(target, ComposerScheduling::SendWhenOnline, cx);
                    })),
            );
        }
        footer = footer
            .child(div().flex_1())
            .child(
                Button::new("schedule-cancel")
                    .label("Cancel")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.close_schedule_picker(cx))),
            )
            .child(
                Button::new("schedule-confirm")
                    .label(kind.submit_label())
                    .primary()
                    .on_click(cx.listener(|this, _, _, cx| this.confirm_schedule_picker(cx))),
            );
        div()
            .id("schedule-popup")
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().popover)
            .child(div().text_sm().font_semibold().child(kind.picker_title()))
            .child(
                DatePicker::new(&picker.date)
                    .presets(presets)
                    .placeholder("Pick a date and time"),
            )
            .when_some(picker.error, |this, error| {
                this.child(
                    div()
                        .id("schedule-error")
                        .text_xs()
                        .text_color(cx.theme().danger)
                        .child(error),
                )
            })
            .child(footer)
            .into_any_element()
    }

    /// The composer's scheduled-messages button
    /// (`updateChatHasScheduledMessages`), shown only while the open chat
    /// has scheduled messages.
    pub(super) fn scheduled_messages_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let chat_id = session.open_chat?;
        if !session.chat_has_scheduled_messages(chat_id) {
            return None;
        }
        let title = match self.schedule_kind() {
            ScheduleKind::Reminder => "Reminders",
            ScheduleKind::Schedule => "Scheduled messages",
        };
        Some(
            Button::new("composer-scheduled")
                .icon(gpui_kit::assets::IconName::CalendarClock)
                .ghost()
                .tooltip(title)
                .accessibility_label(title)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.scheduled_selected.clear();
                    this.open_scheduled_dialog(cx);
                }))
                .into_any_element(),
        )
    }

    /// kit Phase 2 (redo): scheduled messages hosted in a kit `Dialog`
    /// via `window.open_dialog`. Esc / backdrop / ✕ clear state via
    /// `on_close`.
    pub(super) fn build_scheduled_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::Scheduled, |this, _, cx| {
                this.scheduled_dialog_open = false;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let title = this.schedule_kind().list_title();
            let dialog = dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(title));
            let body = this.scheduled_dialog_body(cx);
            dialog
                .content(crate::ui::shell::scrollable_dialog_content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .on_close(on_close)
        })
    }

    /// The scheduled messages that are ticked and still scheduled.
    fn selected_scheduled(&self) -> Vec<MessageId> {
        let mut selected = self.scheduled_selected.clone();
        let existing: Vec<MessageId> = self
            .session()
            .map(|s| s.scheduled_messages.iter().map(|m| m.id).collect())
            .unwrap_or_default();
        quill::selection_pin::prune_selected(&mut selected, &existing);
        selected
    }

    /// "Send now" for the ticked messages, after Telegram Desktop's
    /// `lng_scheduled_send_now_many` question.
    fn confirm_send_selected_now(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids = self.selected_scheduled();
        if ids.is_empty() {
            return;
        }
        let question = quill::selection_pin::send_now_question(ids.len());
        let app = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (app, ids) = (app.clone(), ids.clone());
            alert
                .description(question.clone())
                .ok_text("Send")
                .cancel_text("Cancel")
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    let ids = ids.clone();
                    let _ = app.update(cx, |this, cx| {
                        for id in ids {
                            this.edit_scheduled_state(id, ComposerScheduling::None, cx);
                        }
                        this.scheduled_selected.clear();
                        cx.notify();
                    });
                    true
                })
        });
    }

    /// "Delete" for the ticked messages
    /// (`lng_selected_delete_sure*`).
    fn confirm_delete_selected_scheduled(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids = self.selected_scheduled();
        if ids.is_empty() {
            return;
        }
        let question = quill::selection_pin::delete_question(ids.len());
        let app = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (app, ids) = (app.clone(), ids.clone());
            alert
                .description(question.clone())
                .ok_text("Delete")
                .cancel_text("Cancel")
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    let ids = ids.clone();
                    let _ = app.update(cx, |this, cx| {
                        for id in ids {
                            this.delete_scheduled_message(id, cx);
                        }
                        this.scheduled_selected.clear();
                        cx.notify();
                    });
                    true
                })
        });
    }

    /// The bar above the list while messages are ticked: the count, Send
    /// now, Reschedule, Delete and Clear.
    fn scheduled_selection_bar(&self, count: usize, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("scheduled-selection-bar")
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1()
            .px_2()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .child(format!("{count} selected")),
            )
            .child(
                Button::new("scheduled-selection-send")
                    .label("Send now")
                    .ghost()
                    .small()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.confirm_send_selected_now(window, cx);
                    })),
            )
            .child(
                Button::new("scheduled-selection-reschedule")
                    .label("Reschedule")
                    .ghost()
                    .small()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.scheduled_dialog_open = false;
                        this.open_schedule_picker(ScheduleTarget::RescheduleSelected, window, cx);
                        this.close_kit_dialog_if_done(DialogKind::Scheduled, window, cx);
                    })),
            )
            .child(
                Button::new("scheduled-selection-delete")
                    .label("Delete")
                    .ghost()
                    .small()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.confirm_delete_selected_scheduled(window, cx);
                    })),
            )
            .child(
                Button::new("scheduled-selection-clear")
                    .label("Clear")
                    .ghost()
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.scheduled_selected.clear();
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    /// kit Phase 2 (redo): list body extracted from the old
    /// `scheduled_dialog` — kept pure (no custom scrim/panel).
    pub(super) fn scheduled_dialog_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let kind = self.schedule_kind();
        let messages: Vec<ParsedMessage> = self
            .session()
            .map(|session| session.scheduled_messages.clone())
            .unwrap_or_default();
        let selected = self.selected_scheduled();
        let bar = (!selected.is_empty()).then(|| self.scheduled_selection_bar(selected.len(), cx));
        let mut list = div()
            .id("scheduled-list")
            .flex()
            .flex_col()
            .gap_1()
            .max_h(px(360.))
            .overflow_y_scroll();
        if messages.is_empty() {
            list = list.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(kind.empty_list()),
            );
        }
        for message in messages {
            let id = message.id;
            let preview = effective_content(&message.content, message.ephemeral.as_ref()).preview();
            let label = scheduled_message_label(&message, kind);
            // M1: scheduled sends are the user's own — editing routes
            // through the same composer edit flow with `scheduled: true`
            // so `edit_snapshot` validates against the scheduled list.
            let edit = ComposerEdit::from_own_content(
                message.chat_id,
                message.id,
                true,
                false,
                &message.content,
            )
            .map(|mut edit| {
                edit.scheduled = true;
                edit
            });
            let mut actions = div().flex().flex_wrap().items_center().justify_end();
            actions = actions
                .child(
                    Button::new(format!("scheduled-send-now-{}", id.0))
                        .label("Send now")
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.edit_scheduled_state(id, ComposerScheduling::None, cx);
                        })),
                )
                .child(
                    Button::new(format!("scheduled-reschedule-{}", id.0))
                        .label("Reschedule")
                        .ghost()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.scheduled_dialog_open = false;
                            this.open_schedule_picker(ScheduleTarget::Reschedule(id), window, cx);
                            this.close_kit_dialog_if_done(DialogKind::Scheduled, window, cx);
                        })),
                );
            if let Some(edit) = edit {
                actions = actions.child(
                    Button::new(format!("scheduled-edit-{}", id.0))
                        .label("Edit")
                        .ghost()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.scheduled_dialog_open = false;
                            this.begin_edit(edit.clone(), window, cx);
                            this.close_kit_dialog_if_done(DialogKind::Scheduled, window, cx);
                        })),
                );
            }
            actions = actions.child(
                Button::new(format!("scheduled-delete-{}", id.0))
                    .label("Delete")
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.delete_scheduled_message(id, cx);
                    })),
            );
            list = list.child(
                div()
                    .id(("scheduled-row", id.0 as u64))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(cx.theme().sidebar)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Checkbox::new(format!("scheduled-pick-{}", id.0))
                                    .checked(selected.contains(&id))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        quill::selection_pin::toggle_id(
                                            &mut this.scheduled_selected,
                                            id,
                                        );
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .min_w_0()
                                    .w_full()
                                    .child(div().text_sm().child(preview))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(label),
                                    ),
                            ),
                    )
                    .child(actions),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap_2()
            .children(bar)
            .child(list)
            .into_any_element()
    }
}
