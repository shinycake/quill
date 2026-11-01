//! The poll's custom deadline box (tdesktop `Ui::ChooseDateTimeBox` opened
//! from "Poll ends" > "Custom"): a date and time picker under "Deadline"
//! with a "Set Deadline" button. It replaces the poll panel while open,
//! like the country box.

use super::*;
use crate::ui::dialogs::poll::DeadlineBox;
use crate::ui::scheduled::{new_date_time_picker, picked_unix};
use crate::ui::wall_clock::WallClock;
use quill::lang::{keys, tr};
use quill::poll::{POLL_DEADLINE_MIN_SECS, POLL_DEFAULT_PERIOD_SECS, PollClose};

/// Where the picker starts: the chosen deadline while it is still ahead,
/// else a day from now (or the chosen span).
pub(crate) fn deadline_start(close: PollClose, now: i64) -> i64 {
    match close {
        PollClose::Date(date) if date > now => date,
        PollClose::Period(secs) if secs > 0 => now.saturating_add(secs),
        _ => now.saturating_add(POLL_DEFAULT_PERIOD_SECS),
    }
}

/// A picked moment, held to `[now + 1 minute, now + max]`; `None` when it
/// is already past (the deadline error).
pub(crate) fn deadline_choice(picked: i64, now: i64, max: i64) -> Option<i64> {
    if picked < now.saturating_add(POLL_DEADLINE_MIN_SECS) {
        return None;
    }
    Some(picked.min(now.saturating_add(max)))
}

impl QuillApp {
    /// "Custom": open the deadline box on the current choice.
    pub(in crate::ui) fn open_poll_deadline_box(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let now = WallClock::of(cx).now();
        let max = self.poll_close_max_secs();
        let Some(close) = self.composer_ui.poll_dialog.as_ref().map(|d| d.close) else {
            return;
        };
        let picker = new_date_time_picker(window, cx, deadline_start(close, now), max);
        if let Some(dialog) = self.composer_ui.poll_dialog.as_mut() {
            dialog.deadline_box = Some(DeadlineBox { picker });
        }
        cx.notify();
    }

    pub(in crate::ui) fn close_poll_deadline_box(&mut self, cx: &mut Context<Self>) {
        if let Some(dialog) = self.composer_ui.poll_dialog.as_mut() {
            dialog.deadline_box = None;
        }
        cx.notify();
    }

    /// A deadline box is up (Esc closes it before the poll box).
    pub(in crate::ui) fn poll_deadline_box_open(&self) -> bool {
        self.composer_ui
            .poll_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.deadline_box.is_some())
    }

    /// "Set Deadline": take the picked moment, or say it already passed.
    pub(in crate::ui) fn set_poll_deadline(&mut self, cx: &mut Context<Self>) {
        let picked = self
            .composer_ui
            .poll_dialog
            .as_ref()
            .and_then(|dialog| dialog.deadline_box.as_ref())
            .and_then(|deadline| picked_unix(&deadline.picker, POLL_DEADLINE_MIN_SECS, cx));
        let max = self.poll_close_max_secs();
        let Some(picked) = picked else {
            return;
        };
        match deadline_choice(picked, WallClock::of(cx).now(), max) {
            Some(date) => {
                if let Some(dialog) = self.composer_ui.poll_dialog.as_mut() {
                    dialog.close = PollClose::Date(date);
                    dialog.deadline_box = None;
                }
                cx.notify();
            }
            None => self.set_status_note(
                StatusNote::error(tr(keys::POLLS_CREATE_DEADLINE_EXPIRED)),
                cx,
            ),
        }
    }

    /// The poll's panel while its deadline box is open.
    pub(in crate::ui) fn poll_deadline_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let deadline = self
            .composer_ui
            .poll_dialog
            .as_ref()?
            .deadline_box
            .as_ref()?;
        Some(
            div()
                .id("poll-dialog")
                .flex()
                .flex_col()
                .gap_2()
                .p_3()
                .rounded_md()
                .border_1()
                .border_color(cx.theme().primary)
                .bg(cx.theme().background)
                .child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .child(tr(keys::POLLS_CREATE_DEADLINE_TITLE)),
                )
                .child(DatePicker::new(&deadline.picker.state))
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("poll-deadline-cancel")
                                .label(tr(keys::CANCEL))
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.close_poll_deadline_box(cx);
                                })),
                        )
                        .child(
                            Button::new("poll-deadline-set")
                                .label(tr(keys::POLLS_CREATE_DEADLINE_BUTTON))
                                .primary()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.set_poll_deadline(cx);
                                })),
                        ),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{deadline_choice, deadline_start};
    use quill::poll::{POLL_DEFAULT_PERIOD_SECS, PollClose};

    #[test]
    fn the_picker_starts_where_the_choice_is() {
        assert_eq!(deadline_start(PollClose::Date(5000), 1000), 5000);
        assert_eq!(
            deadline_start(PollClose::Date(500), 1000),
            1000 + POLL_DEFAULT_PERIOD_SECS
        );
        assert_eq!(deadline_start(PollClose::Period(3600), 1000), 4600);
    }

    #[test]
    fn a_deadline_must_be_a_minute_ahead_and_within_the_cap() {
        assert_eq!(deadline_choice(1030, 1000, 86_400), None);
        assert_eq!(deadline_choice(1060, 1000, 86_400), Some(1060));
        assert_eq!(deadline_choice(900_000, 1000, 86_400), Some(87_400));
    }
}
