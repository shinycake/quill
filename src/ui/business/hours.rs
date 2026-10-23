//! The Business Hours screen, its weekday editor and the time zone list
//! (tdesktop `settings_working_hours.cpp`).

use super::{DayRow, HoursDraft, Leave, View};
use crate::ui::app::QuillApp;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::input::Input;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::time_field::{TimeField, TimeFieldState};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::business_hours_edit::{
    self as edit, DAY, Interval, WEEKDAYS, day_from_clock, day_summary, extract_day,
    next_added_interval, replace_day,
};
use quill::business_info::OpeningHours;
use quill::business_settings::{TimeZone, closest_time_zone};
use quill::lang::{keys, tr};
use quill::local_time::{now_unix, utc_offset_at};

fn about() -> &'static str {
    tr(keys::HOURS_ABOUT)
}
fn day_about() -> &'static str {
    tr(keys::HOURS_ABOUT_DAY)
}

fn time_field(
    window: &mut Window,
    cx: &mut Context<QuillApp>,
    minute: i32,
) -> Entity<TimeFieldState> {
    let state = cx.new(|cx| TimeFieldState::new(window, cx));
    let minute = minute.rem_euclid(DAY);
    let text = format!("{:02}:{:02}:00", minute / 60, minute % 60);
    if let Ok(time) = text.parse() {
        state.update(cx, |s, cx| s.set_time(time, window, cx));
    }
    state
}

/// Minute of the day a time field shows.
fn field_minute(state: &Entity<TimeFieldState>, cx: &App) -> i32 {
    let text = state
        .read(cx)
        .time()
        .format(tr(keys::Q_OPS_H_M))
        .to_string();
    let mut parts = text.split(':').filter_map(|p| p.parse::<i32>().ok());
    let hour = parts.next().unwrap_or(0);
    let minute = parts.next().unwrap_or(0);
    hour * 60 + minute
}

impl QuillApp {
    pub(super) fn new_hours_draft(&self) -> HoursDraft {
        let saved = self.my_business().opening_hours;
        HoursDraft {
            enabled: saved.is_some(),
            week: saved
                .as_ref()
                .map(|h| {
                    h.intervals
                        .iter()
                        .map(|&(s, e)| (s as i32, e as i32))
                        .collect()
                })
                .unwrap_or_default(),
            time_zone_id: saved.map(|h| h.time_zone_id).unwrap_or_default(),
        }
    }

    fn business_time_zones(&self) -> Vec<TimeZone> {
        self.session()
            .and_then(|s| s.settings.business.time_zones.clone())
            .unwrap_or_default()
    }

    /// The zone the hours are in: the chosen one when it is in TDLib's
    /// list, otherwise the one closest to the local offset (tdesktop
    /// `FindClosestTimezoneId`).
    fn business_zone(&self) -> Option<TimeZone> {
        let id = self
            .dialogs
            .business_dialog
            .as_ref()
            .and_then(|d| d.hours.as_ref())
            .map(|h| h.time_zone_id.clone())
            .unwrap_or_default();
        let zones = self.business_time_zones();
        zones
            .iter()
            .find(|z| z.id == id)
            .or_else(|| closest_time_zone(&zones, utc_offset_at(now_unix())))
            .cloned()
    }

    fn business_week(&self) -> Vec<Interval> {
        self.dialogs
            .business_dialog
            .as_ref()
            .and_then(|d| d.hours.as_ref())
            .map(|h| h.week.clone())
            .unwrap_or_default()
    }

    fn set_business_day(&mut self, day: usize, intervals: &[Interval], cx: &mut Context<Self>) {
        if let Some(h) = self.business_mut().and_then(|d| d.hours.as_mut()) {
            h.week = replace_day(&h.week, day, intervals);
        }
        cx.notify();
    }

    pub(super) fn begin_day_edit(
        &mut self,
        day: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let intervals = extract_day(&self.business_week(), day);
        let rows = intervals
            .iter()
            .map(|&(open, close)| DayRow {
                open: time_field(window, cx, open),
                close: time_field(window, cx, close),
            })
            .collect();
        if let Some(d) = self.business_mut() {
            d.day_rows = rows;
        }
    }

    fn business_day_clock(&self, cx: &App) -> Vec<(i32, i32)> {
        self.dialogs
            .business_dialog
            .as_ref()
            .map(|d| {
                d.day_rows
                    .iter()
                    .map(|r| (field_minute(&r.open, cx), field_minute(&r.close, cx)))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn add_business_day_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = day_from_clock(&self.business_day_clock(cx)).unwrap_or_default();
        let Some((open, close)) = next_added_interval(&current) else {
            return;
        };
        let row = DayRow {
            open: time_field(window, cx, open),
            close: time_field(window, cx, close),
        };
        if let Some(d) = self.business_mut() {
            d.day_rows.push(row);
            d.local_error = None;
        }
        cx.notify();
    }

    /// Save of the weekday editor: back to the week with the new hours.
    pub(super) fn save_business_day(
        &mut self,
        day: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match day_from_clock(&self.business_day_clock(cx)) {
            Ok(intervals) => {
                self.set_business_day(day, &intervals, cx);
                self.business_go(View::Hours, window, cx);
            }
            Err(problem) => self.business_error(problem.line(), cx),
        }
    }

    pub(super) fn business_hours_body(&mut self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let Some(enabled) = self
            .dialogs
            .business_dialog
            .as_ref()
            .and_then(|d| d.hours.as_ref())
            .map(|h| h.enabled)
        else {
            return div();
        };
        let mut body = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_sm().text_color(muted).child(about()))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_sm().child(tr(keys::HOURS_SHOW)))
                    .child(
                        Switch::new("business-hours-enable")
                            .checked(enabled)
                            .accessibility_label(tr(keys::HOURS_SHOW))
                            .on_click(cx.listener(|this, &on: &bool, _, cx| {
                                if let Some(h) = this.business_mut().and_then(|d| d.hours.as_mut())
                                {
                                    h.enabled = on;
                                }
                                cx.notify();
                            })),
                    ),
            );
        if !enabled {
            return body;
        }
        let week = self.business_week();
        let hover = cx.theme().secondary;
        let mut days = div().flex().flex_col();
        for (index, name) in WEEKDAYS.iter().enumerate() {
            let intervals = extract_day(&week, index);
            let open = !intervals.is_empty();
            days = days.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .id(("business-day", index as u64))
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .cursor_pointer()
                            .role(gpui_kit::Role::Button)
                            .aria_label(format!("{name}: {}", day_summary(&intervals)))
                            .tab_index(0)
                            .hover(move |s| s.bg(hover))
                            .child(div().text_sm().child(*name))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(day_summary(&intervals)),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.business_go(View::Day(index), window, cx);
                            })),
                    )
                    .child(
                        Switch::new(("business-day-open", index as u64))
                            .checked(open)
                            .accessibility_label(format!("{name} open"))
                            .on_click(cx.listener(move |this, &on: &bool, _, cx| {
                                let all_day: &[Interval] = if on { &[(0, DAY)] } else { &[] };
                                this.set_business_day(index, all_day, cx);
                            })),
                    ),
            );
        }
        let zone = self
            .business_zone()
            .map_or_else(|| tr(keys::Q_OPS_LOADING).to_string(), |z| z.full_name());
        body = body.child(days).child(
            crate::ui::pressable::action_row(
                "business-time-zone",
                Some(IconName::Clock),
                tr(keys::HOURS_TIME_ZONE),
                false,
                cx,
            )
            .justify_between()
            .child(div().flex_1())
            .child(div().text_color(muted).truncate().child(zone))
            .on_click(cx.listener(|this, _, window, cx| {
                this.business_go(View::TimeZone, window, cx);
            })),
        );
        body
    }

    pub(super) fn business_day_body(&mut self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let Some(rows) = self.dialogs.business_dialog.as_ref().map(|d| {
            d.day_rows
                .iter()
                .map(|r| (r.open.clone(), r.close.clone()))
                .collect::<Vec<_>>()
        }) else {
            return div();
        };
        let clock = self.business_day_clock(cx);
        let mut body = div().flex().flex_col().gap_3();
        for (index, (open, close)) in rows.into_iter().enumerate() {
            let next_day = clock
                .get(index)
                .is_some_and(|&(o, c)| c.rem_euclid(DAY) <= o.rem_euclid(DAY));
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .pb_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(div().text_sm().child(tr(keys::HOURS_OPENING)))
                            .child(TimeField::new(&open)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .gap_1()
                                    .child(div().text_sm().child(tr(keys::HOURS_CLOSING)))
                                    .when(next_day, |this| {
                                        this.child(
                                            div()
                                                .text_sm()
                                                .text_color(muted)
                                                .child(tr(keys::Q_OPS_NEXT_DAY)),
                                        )
                                    }),
                            )
                            .child(TimeField::new(&close)),
                    )
                    .child(
                        div().flex().child(
                            Button::new(("business-day-remove", index as u64))
                                .label(tr(keys::HOURS_REMOVE))
                                .ghost()
                                .small()
                                .text_color(cx.theme().danger)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(d) = this.business_mut()
                                        && index < d.day_rows.len()
                                    {
                                        d.day_rows.remove(index);
                                    }
                                    cx.notify();
                                })),
                        ),
                    ),
            );
        }
        let can_add = day_from_clock(&clock)
            .map(|intervals| next_added_interval(&intervals).is_some())
            .unwrap_or(false);
        if can_add {
            body = body.child(
                crate::ui::pressable::action_row(
                    "business-day-add",
                    Some(IconName::Plus),
                    tr(keys::HOURS_ADD_BUTTON),
                    false,
                    cx,
                )
                .text_color(cx.theme().primary)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.add_business_day_row(window, cx);
                })),
            );
        }
        body.child(div().text_xs().text_color(muted).child(day_about()))
    }

    pub(super) fn business_zone_body(&mut self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let Some(search) = self
            .dialogs
            .business_dialog
            .as_ref()
            .map(|d| d.zone_search.clone())
        else {
            return div();
        };
        let current = self.business_zone().map(|z| z.id);
        let query = search.read(cx).value().trim().to_lowercase();
        let zones = self.business_time_zones();
        let mut list = div()
            .id("business-zone-list")
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .max_h(px(360.));
        if zones.is_empty() {
            list = list.child(
                div()
                    .py_2()
                    .text_sm()
                    .text_color(muted)
                    .child(tr(keys::Q_OPS_LOADING)),
            );
        }
        for (index, zone) in zones
            .into_iter()
            .filter(|z| query.is_empty() || z.full_name().to_lowercase().contains(&query))
            .enumerate()
        {
            let chosen = current.as_deref() == Some(zone.id.as_str());
            let id = zone.id.clone();
            list = list.child(
                crate::ui::pressable::action_row(
                    ("business-zone", index as u64),
                    chosen.then_some(IconName::Check),
                    zone.full_name(),
                    false,
                    cx,
                )
                .when(!chosen, |row| row.pl(px(36.)))
                .on_click(cx.listener(move |this, _, window, cx| {
                    if let Some(h) = this.business_mut().and_then(|d| d.hours.as_mut()) {
                        h.time_zone_id = id.clone();
                    }
                    this.business_go(View::Hours, window, cx);
                })),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(Input::new(&search).aria_label(tr(keys::Q_OPS_SEARCH_TIME_ZONES)))
            .child(list)
    }

    /// Save of the hours screen (when it is left). Off, or on with no
    /// open day, removes the hours (tdesktop sends an empty
    /// `WorkingHours` then).
    pub(super) fn save_business_hours(&mut self, cx: &mut Context<Self>) -> Leave {
        let Some((enabled, week)) = self
            .dialogs
            .business_dialog
            .as_ref()
            .and_then(|d| d.hours.as_ref())
            .map(|h| (h.enabled, h.week.clone()))
        else {
            return Leave::Go(None);
        };
        let wanted = if enabled && !week.is_empty() {
            let Some(zone) = self.business_zone() else {
                self.business_error(tr(keys::Q_OPS_THE_TIME_ZONES_ARE_STILL), cx);
                return Leave::Stay;
            };
            let raw = edit::normalized(week)
                .into_iter()
                .filter_map(|(s, e)| Some((u32::try_from(s).ok()?, u32::try_from(e).ok()?)))
                .collect();
            Some(OpeningHours::new(zone.id, raw))
        } else {
            None
        };
        if wanted == self.my_business().opening_hours {
            return Leave::Go(None);
        }
        self.business_send(tr(keys::Q_OPS_BUSINESS_HOURS_2), |driver| {
            driver.save_business_hours(wanted.as_ref()).is_ok()
        })
    }
}
