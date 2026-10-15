//! Starting a video chat or live stream from a chat: the leave-current
//! confirmation (tdesktop `Calls::Instance::confirmLeaveCurrent`,
//! calls/calls_instance.cpp), the create confirmation with its "schedule"
//! link (`CreateOrJoinConfirmation` in `ChooseJoinAsProcess::processList`,
//! calls/group/calls_choose_join_as.cpp) and `ScheduleGroupCallBox` (same
//! file) on the kit date and time picker.
//!
//! The steps share one registered dialog, each replacing the last as
//! tdesktop's boxes replace each other. Nothing is created until "Create",
//! "Schedule" or "Start Streaming" goes through; the call you are in ends
//! only then (`createGroupCall` → `destroyCurrentCall`).
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use super::super::group_call_stream_layout::StreamJoinIntent;
use super::super::scheduled::{new_date_time_picker, picked_unix};
use super::super::shell::{DialogKind, QuillShell};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::date_picker::{DatePicker, DatePickerState};
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{ActiveTheme, Sizable, WindowExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::calls::group_call_create::{
    BeforeCreate, COUNTDOWN_REFRESH_SECS, Countdown, CreateKind, CurrentCall, LeaveText,
    before_create, check_schedule, countdown, default_schedule_at, schedule_max,
};
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with, trn};
use quill::local_time::{now_unix, utc_offset_at};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

/// The open create flow: the chat, what it creates and the step on screen.
pub(crate) struct GroupCallCreate {
    pub(crate) chat_id: ChatId,
    pub(crate) kind: CreateKind,
    pub(crate) step: CreateStep,
}

pub(crate) enum CreateStep {
    /// "Leave your currently active video chat and join this video chat?"
    Leave(LeaveText),
    /// "Start a video chat in this group?" with Create and the link.
    Confirm,
    /// `ScheduleGroupCallBox`.
    Schedule(ScheduleStep),
}

pub(crate) struct ScheduleStep {
    pub(crate) picker: Entity<DatePickerState>,
    /// The picked time is outside the allowed range.
    pub(crate) error: Option<&'static str>,
    /// Re-renders the countdown every ten seconds while the box shows.
    _refresh: Task<()>,
}

impl QuillApp {
    /// "Start video chat" / "Schedule video chat" / the call button for a
    /// chat with no call, and "Start Streaming": confirm leaving the call
    /// you are in first (`confirmLeaveCurrent`), then go on.
    pub(in crate::ui) fn begin_group_call_create(
        &mut self,
        chat_id: ChatId,
        kind: CreateKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session() else {
            return;
        };
        let to_channel = session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.kind.is_channel());
        match before_create(session.current_call_for_create(), chat_id, to_channel) {
            BeforeCreate::Proceed => self.continue_group_call_create(chat_id, kind, window, cx),
            BeforeCreate::ShowCurrent => {
                self.group_call.start_dialog = None;
                self.show_group_call_window(cx);
            }
            BeforeCreate::Ask(text) => {
                self.group_call.start_dialog = Some(GroupCallCreate {
                    chat_id,
                    kind,
                    step: CreateStep::Leave(text),
                });
                cx.notify();
            }
        }
    }

    /// Past the leave confirmation: the create confirmation, the schedule
    /// box, or the stream itself.
    fn continue_group_call_create(
        &mut self,
        chat_id: ChatId,
        kind: CreateKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match kind {
            CreateKind::Now => {
                self.group_call.start_dialog = Some(GroupCallCreate {
                    chat_id,
                    kind,
                    step: CreateStep::Confirm,
                });
            }
            CreateKind::Schedule => self.open_group_call_schedule(chat_id, window, cx),
            CreateKind::Rtmp => {
                self.group_call.start_dialog = None;
                self.start_rtmp_stream_now(window, cx);
            }
        }
        cx.notify();
    }

    /// `ScheduleGroupCallBox`: opens on the next whole hour at least half
    /// an hour away; days past the seventh one ahead cannot be picked.
    pub(in crate::ui) fn open_group_call_schedule(
        &mut self,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let now = now_unix();
        let initial = default_schedule_at(now, &utc_offset_at);
        let max = schedule_max(now, &utc_offset_at);
        let picker = new_date_time_picker(window, cx, initial, max - now);
        let refresh = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(COUNTDOWN_REFRESH_SECS))
                    .await;
                let open = this.update(cx, |app, cx| {
                    let open = matches!(
                        app.group_call.start_dialog.as_ref().map(|flow| &flow.step),
                        Some(CreateStep::Schedule(_))
                    );
                    if open {
                        cx.notify();
                    }
                    open
                });
                if !matches!(open, Ok(true)) {
                    break;
                }
            }
        });
        self.group_call.start_dialog = Some(GroupCallCreate {
            chat_id,
            kind: CreateKind::Schedule,
            step: CreateStep::Schedule(ScheduleStep {
                picker,
                error: None,
                _refresh: refresh,
            }),
        });
        cx.notify();
    }

    fn close_group_call_create(&mut self, cx: &mut Context<Self>) {
        self.group_call.start_dialog = None;
        cx.notify();
    }

    /// "Schedule": `createVideoChat` with the picked `start_date`, once it
    /// is in range (`ChooseDateTimeBox::collect`).
    fn confirm_group_call_schedule(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(GroupCallCreate {
            chat_id,
            step: CreateStep::Schedule(step),
            ..
        }) = self.group_call.start_dialog.as_ref()
        else {
            return;
        };
        let chat_id = *chat_id;
        let now = now_unix();
        let picked = picked_unix(&step.picker, cx)
            .ok_or(quill::schedule::ScheduleError::Invalid)
            .and_then(|date| check_schedule(date, now, schedule_max(now, &utc_offset_at)));
        match picked {
            Ok(date) => self.send_group_call_create(chat_id, date, window, cx),
            Err(err) => {
                if let Some(GroupCallCreate {
                    step: CreateStep::Schedule(step),
                    ..
                }) = self.group_call.start_dialog.as_mut()
                {
                    step.error = Some(err.message());
                }
                cx.notify();
            }
        }
    }

    /// `createVideoChat` (now when `start_date` is 0). The call you are in
    /// ends first; the new one is joined once it is tracked, and only
    /// when the request went out.
    fn send_group_call_create(
        &mut self,
        chat_id: ChatId,
        start_date: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.group_call.start_dialog = None;
        self.end_current_call_for_create(cx);
        let sent = match self.live.as_mut() {
            Some(live) => live
                .driver
                .start_video_chat(chat_id.0, String::new(), start_date)
                .is_ok(),
            // Screenshot demos and tests have no TDLib: nothing to send.
            None => self.demo_session.is_some(),
        };
        if !sent {
            window.push_notification(
                Notification::new().message(tr(keys::Q_OPS_COULD_NOT_START_VOICE_CHAT)),
                cx,
            );
        } else if start_date == 0 {
            self.note_stream_join_intent(StreamJoinIntent::Chat(chat_id.0));
        }
        cx.notify();
    }

    /// tdesktop `destroyCurrentCall` right before a new call is created:
    /// hang up a one-to-one call, leave a joined video chat, and stop
    /// tracking a call you were only looking at or that is scheduled.
    pub(in crate::ui) fn end_current_call_for_create(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.session() else {
            return;
        };
        let one_to_one = session.current_call_for_create() == CurrentCall::OneToOne;
        let tracked = session.calls.active_group_call.as_ref();
        let joined = tracked.is_some_and(|call| call.is_joined);
        let tracked = tracked.is_some();
        if one_to_one {
            self.hang_up_call(cx);
        }
        if joined {
            self.leave_active_group_call(cx);
        } else if tracked {
            if let Some(live) = self.live.as_mut() {
                live.driver.session.leave_group_call_local();
            } else if let Some(session) = self.demo_session.as_mut() {
                session.leave_group_call_local();
            }
        }
    }

    /// The registered dialog: the step on screen.
    pub(crate) fn build_group_call_create_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::GroupCallStart, |this, _, cx| {
                this.close_group_call_create(cx);
            });
        app.update(cx, |this, cx| {
            let Some(flow) = this.group_call.start_dialog.as_ref() else {
                return dialog.overlay(true).on_close(on_close);
            };
            let channel = this
                .session()
                .and_then(|s| s.chats.get(&flow.chat_id.0))
                .is_some_and(|chat| chat.kind.is_channel());
            let (title, body, confirm) = match &flow.step {
                CreateStep::Leave(text) => (None, leave_body(*text, cx), leave_button(*text, cx)),
                CreateStep::Confirm => (
                    None,
                    create_body(flow.chat_id, channel, cx),
                    footer_button(
                        "group-call-create-confirm",
                        tr(keys::CREATE_GROUP_CREATE),
                        cx,
                        |this, window, cx| {
                            if let Some(chat_id) =
                                this.group_call.start_dialog.as_ref().map(|f| f.chat_id)
                            {
                                this.send_group_call_create(chat_id, 0, window, cx);
                            }
                        },
                    ),
                ),
                CreateStep::Schedule(step) => (
                    Some(if channel {
                        tr(keys::GROUP_CALL_SCHEDULE_TITLE_CHANNEL)
                    } else {
                        tr(keys::GROUP_CALL_SCHEDULE_TITLE)
                    }),
                    schedule_body(step, channel, cx),
                    footer_button(
                        "group-call-schedule-confirm",
                        tr(keys::SCHEDULE_BUTTON),
                        cx,
                        |this, window, cx| this.confirm_group_call_schedule(window, cx),
                    ),
                ),
            };
            let footer = div()
                .flex()
                .w_full()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("group-call-create-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_group_call_create(cx);
                            this.close_kit_dialog_if_done(DialogKind::GroupCallStart, window, cx);
                        })),
                )
                .child(confirm);
            dialog
                .overlay(true)
                .width(px(380.))
                .when_some(title, |dialog, title| {
                    dialog.title(crate::ui::shell::dialog_title(title))
                })
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body)));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

type StepAction = fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>);

/// The step's primary button; the dialog closes when the step cleared the
/// flow.
fn footer_button(
    id: &'static str,
    label: &'static str,
    cx: &mut Context<QuillApp>,
    action: StepAction,
) -> AnyElement {
    div()
        .debug_selector(move || id.into())
        .child(Button::new(id).label(label).primary().on_click(cx.listener(
            move |this, _, window, cx| {
                action(this, window, cx);
                this.close_kit_dialog_if_done(DialogKind::GroupCallStart, window, cx);
            },
        )))
        .into_any_element()
}

/// `confirmLeaveCurrent`'s question.
fn leave_body(text: LeaveText, cx: &mut Context<QuillApp>) -> AnyElement {
    let key = match text {
        LeaveText::EndCall { to_channel: false } => keys::CALL_LEAVE_TO_OTHER_SURE,
        LeaveText::EndCall { to_channel: true } => keys::CALL_LEAVE_TO_OTHER_SURE_CHANNEL,
        LeaveText::LeaveGroupCall {
            from_channel: true,
            to_channel: true,
        } => keys::GROUP_CALL_LEAVE_CHANNEL_TO_OTHER_SURE_CHANNEL,
        LeaveText::LeaveGroupCall {
            from_channel: true,
            to_channel: false,
        } => keys::GROUP_CALL_LEAVE_CHANNEL_TO_OTHER_SURE,
        LeaveText::LeaveGroupCall {
            from_channel: false,
            to_channel: true,
        } => keys::GROUP_CALL_LEAVE_TO_OTHER_SURE_CHANNEL,
        LeaveText::LeaveGroupCall {
            from_channel: false,
            to_channel: false,
        } => keys::GROUP_CALL_LEAVE_TO_OTHER_SURE,
    };
    div()
        .id("group-call-create-leave")
        .debug_selector(|| "group-call-create-leave".into())
        .pt_2()
        .text_sm()
        .text_color(cx.theme().foreground)
        .whitespace_normal()
        .child(tr(key))
        .into_any_element()
}

/// "End call" for a one-to-one call, "Leave" for a video chat.
fn leave_button(text: LeaveText, cx: &mut Context<QuillApp>) -> AnyElement {
    let label = match text {
        LeaveText::EndCall { .. } => tr(keys::CALL_BAR_HANGUP),
        LeaveText::LeaveGroupCall { .. } => tr(keys::GROUP_CALL_LEAVE),
    };
    footer_button(
        "group-call-create-leave-confirm",
        label,
        cx,
        |this, window, cx| {
            if let Some((chat_id, kind)) = this
                .group_call
                .start_dialog
                .as_ref()
                .map(|flow| (flow.chat_id, flow.kind))
            {
                this.continue_group_call_create(chat_id, kind, window, cx);
            }
        },
    )
}

/// `lng_group_call_create_sure(_channel)`, a blank line and "You can also
/// {link}." where the link opens the schedule box.
fn create_body(chat_id: ChatId, channel: bool, cx: &mut Context<QuillApp>) -> AnyElement {
    let question = if channel {
        tr(keys::GROUP_CALL_CREATE_SURE_CHANNEL)
    } else {
        tr(keys::GROUP_CALL_CREATE_SURE)
    };
    let link = if channel {
        tr(keys::GROUP_CALL_SCHEDULE_CHANNEL)
    } else {
        tr(keys::GROUP_CALL_SCHEDULE)
    };
    let or_schedule = tr(keys::GROUP_CALL_OR_SCHEDULE);
    let (before, after) = or_schedule
        .split_once("{link}")
        .unwrap_or((or_schedule, ""));
    div()
        .flex()
        .flex_col()
        .gap_3()
        .pt_2()
        .text_sm()
        .text_color(cx.theme().foreground)
        .child(
            div()
                .id("group-call-create-sure")
                .debug_selector(|| "group-call-create-sure".into())
                .whitespace_normal()
                .child(question),
        )
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .child(before)
                .child(
                    div()
                        .debug_selector(|| "group-call-create-schedule".into())
                        .child(
                            Button::new("group-call-create-schedule")
                                .label(link)
                                .link()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.open_group_call_schedule(chat_id, window, cx);
                                })),
                        ),
                )
                .child(after),
        )
        .into_any_element()
}

/// The date and time picker and the live "will start in …" line.
fn schedule_body(step: &ScheduleStep, channel: bool, cx: &mut Context<QuillApp>) -> AnyElement {
    let notified = picked_unix(&step.picker, cx).map(|date| {
        let duration = match countdown(date, now_unix()) {
            Countdown::Days(n) => trn(keys::DAYS, n),
            Countdown::Hours(n) => trn(keys::HOURS, n),
            Countdown::Minutes(n) => trn(keys::MINUTES, n),
        };
        let key = if channel {
            keys::GROUP_CALL_SCHEDULE_NOTIFIED_CHANNEL
        } else {
            keys::GROUP_CALL_SCHEDULE_NOTIFIED_GROUP
        };
        tr_with(key, &[("duration", &duration)])
    });
    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .debug_selector(|| "group-call-schedule-picker".into())
                .child(DatePicker::new(&step.picker).small()),
        )
        .when_some(step.error, |this, error| {
            this.child(
                div()
                    .id("group-call-schedule-error")
                    .debug_selector(|| "group-call-schedule-error".into())
                    .text_xs()
                    .text_color(cx.theme().danger)
                    .child(error),
            )
        })
        .when_some(notified, |this, text| {
            this.child(
                div()
                    .id("group-call-schedule-notified")
                    .debug_selector(|| "group-call-schedule-notified".into())
                    .text_sm()
                    .whitespace_normal()
                    .text_color(cx.theme().muted_foreground)
                    .child(text),
            )
        })
        .into_any_element()
}

crate::ui::shell::register_dialogs! {
    GroupCallStart => DialogSpec::new(
        400,
        |app| app.group_call.start_dialog.is_some(),
        QuillApp::build_group_call_create_dialog,
    ),
}

#[cfg(feature = "demo-capture")]
#[path = "group_call_create_demo.rs"]
mod demo;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "group_call_create_tests.rs"]
mod ui_tests;
