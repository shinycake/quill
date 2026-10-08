//! "Jump to date": the calendar box opened from the history's date pill and
//! the in-chat search bar (tdesktop `Ui::CalendarBox`). A media tab in the
//! search highlights the days that have such messages
//! (`getChatMessageCalendar`); any day can be picked — the jump lands on
//! that day's first message (`getChatMessageByDate`).

use super::QuillApp;
use super::{DialogKind, QuillShell};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::local_time::{now_unix, weekday_name};
use quill::search_filters::{SearchMediaKind, YearMonth, day_number, local_day_number};
use std::cell::RefCell;
use std::rc::Rc;

/// Telegram launched on 2013-08-14; nothing is older.
const EARLIEST: YearMonth = YearMonth {
    year: 2013,
    month: 8,
};
const CELL: f32 = 36.;

impl QuillApp {
    pub(super) fn open_jump_date_ui(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if live.driver.open_history_calendar().is_err() {
                self.status_note = "open a chat to jump to a date".into();
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.open_history_calendar(now_unix());
        }
        cx.notify();
    }

    pub(super) fn close_jump_date_ui(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.close_history_calendar();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.close_history_calendar();
        }
        cx.notify();
    }

    fn jump_date_month(&mut self, month: YearMonth, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.show_calendar_month(month);
        } else if let Some(calendar) = self
            .demo_session
            .as_mut()
            .and_then(|s| s.history_calendar.as_mut())
        {
            calendar.show_month(month);
        }
        cx.notify();
    }

    fn jump_date_pick(&mut self, day: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.jump_to_date(day) {
                Ok(_) => "jumping to date…".into(),
                Err(_) => "could not jump to that date".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            session.close_history_calendar();
        }
        cx.notify();
    }

    pub(crate) fn build_jump_date_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::JumpToDate, |this, _, cx| {
                this.close_jump_date_ui(cx);
            });
        app.update(cx, |this, cx| {
            let body = this.jump_date_body(cx);
            let body = Rc::new(RefCell::new(Some(body)));
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title("Jump to date"))
                .content(move |content, _, _| {
                    let body = body
                        .borrow_mut()
                        .take()
                        .unwrap_or_else(|| div().into_any_element());
                    content.child(body)
                })
                .on_close(on_close)
        })
    }

    pub(super) fn jump_date_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(calendar) = self.session().and_then(|s| s.history_calendar.clone()) else {
            return div().into_any_element();
        };
        let today = local_day_number(now_unix());
        let month = calendar.month;
        let current = YearMonth::of(now_unix());
        let can_prev = month > EARLIEST;
        let can_next = month < current;
        let theme = cx.theme().clone();

        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .child(
                Button::new("jump-date-prev")
                    .icon(gpui_kit::assets::IconName::ChevronLeft)
                    .ghost()
                    .disabled(!can_prev)
                    .tooltip("Previous month")
                    .accessibility_label("Previous month")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.jump_date_month(month.prev(), cx);
                    })),
            )
            .child(
                div()
                    .id("jump-date-title")
                    .font_semibold()
                    .role(Role::Heading)
                    .aria_label(month.title())
                    .child(month.title()),
            )
            .child(
                Button::new("jump-date-next")
                    .icon(gpui_kit::assets::IconName::ChevronRight)
                    .ghost()
                    .disabled(!can_next)
                    .tooltip("Next month")
                    .accessibility_label("Next month")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.jump_date_month(month.next(), cx);
                    })),
            );

        let mut weekdays = div().flex();
        for weekday in 0..7u8 {
            weekdays = weekdays.child(
                div()
                    .w(px(CELL))
                    .flex()
                    .justify_center()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(weekday_name(weekday)[..2].to_string()),
            );
        }

        let mut grid = div().flex().flex_col().gap_0p5();
        for week in month.grid() {
            let mut row = div().flex();
            for cell in week {
                let Some(day) = cell else {
                    row = row.child(div().size(px(CELL)));
                    continue;
                };
                let dn = day_number(month.year, month.month, day);
                let future = dn > today;
                let marked = calendar.day(dn).is_some();
                let label = if marked {
                    format!("{day} (has messages)")
                } else {
                    day.to_string()
                };
                let mut cell = div()
                    .id(("jump-date-day", dn as u64))
                    .size(px(CELL))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .text_sm()
                    .role(Role::Button)
                    .aria_label(label)
                    .child(day.to_string());
                if dn == today {
                    cell = cell.border_1().border_color(theme.primary);
                }
                if future {
                    cell = cell.text_color(theme.muted_foreground.opacity(0.4));
                } else {
                    cell = cell
                        .cursor_pointer()
                        .tab_index(0)
                        .hover(|s| s.bg(theme.secondary_hover))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.jump_date_pick(dn, cx);
                        }));
                    if marked {
                        cell = cell
                            .bg(theme.primary.opacity(0.16))
                            .text_color(theme.primary)
                            .font_semibold();
                    }
                }
                row = row.child(cell);
            }
            grid = grid.child(row);
        }

        let hint = if calendar.loading {
            "Loading days…".to_string()
        } else if calendar.media == SearchMediaKind::All {
            "Pick a day to see its first message.".to_string()
        } else {
            format!(
                "Days with {} are highlighted.",
                calendar.media.label().to_lowercase()
            )
        };
        let note = self.session().and_then(|s| s.date_jump_note.clone());
        div()
            .id("jump-date")
            .mx_auto()
            .flex()
            .flex_col()
            .gap_2()
            .w(px(CELL * 7.))
            .child(header)
            .child(weekdays)
            .child(grid)
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(note.unwrap_or(hint)),
            )
            .into_any_element()
    }
}
