//! Telegram Business rows of a user profile (business hours and
//! location) and the Fragment note of the phone row menu. Behaviour
//! follows tdesktop's `info_profile_actions.cpp` and
//! `info_profile_phone_menu.cpp`; see
//! `docs/decisions/codex-profile-business-rows.md`.

use super::app::QuillApp;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::business_info::{
    self, BusinessInfo, OpeningHours, format_day, opens_in_text, week_minute,
};
use quill::local_time::{civil_local, now_unix, weekday_name};

/// Expanded schedule and chosen time zone of the hours row.
#[derive(Default)]
pub(crate) struct BusinessHoursUi {
    pub(super) expanded: bool,
    /// Show the schedule in the business's own time zone.
    pub(super) business_time: bool,
}

/// The note under the phone row menu for an anonymous Fragment number.
pub(super) fn fragment_note_menu(menu: PopupMenu, muted: Hsla) -> PopupMenu {
    menu.separator()
        .item(
            PopupMenuItem::element(move |_, _| {
                div()
                    .w(px(220.))
                    .text_xs()
                    .text_color(muted)
                    .child("This number is not tied to a SIM card and was acquired on Fragment.")
            })
            .disabled(true),
        )
        .link("Open Fragment", business_info::FRAGMENT_URL)
}

fn now_week_minute() -> u32 {
    let now = civil_local(now_unix());
    week_minute(now.weekday, now.hour, now.minute)
}

impl QuillApp {
    /// The business hours and location rows, in tdesktop's order (hours
    /// after the birthday and personal channel, location last).
    pub(super) fn business_rows(&self, user_id: i64, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(info) = self
            .session()
            .and_then(|s| s.user_full_info(user_id))
            .and_then(|i| i.extras.business.clone())
        else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        if info.opening_hours.is_some() {
            rows.push(self.business_hours_row(&info, cx));
        }
        if let Some(location) = info.location.clone() {
            rows.push(self.business_location_row(&location, cx));
        }
        rows
    }

    fn business_location_row(
        &self,
        location: &business_info::BusinessLocation,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let hover = cx.theme().secondary;
        let url = location.map_url();
        let address = location.address.clone();
        let value = if address.is_empty() {
            "Open on map".to_string()
        } else {
            address.clone()
        };
        div()
            .id("info-business-location")
            .flex()
            .flex_col()
            .px_3()
            .py_2()
            .border_t_1()
            .border_color(cx.theme().border)
            .cursor_pointer()
            .role(gpui_kit::Role::Button)
            .aria_label(format!("Location: {value}"))
            .tab_index(0)
            .hover(move |style| style.bg(hover))
            .child(super::bidi_line::aligned_block(value).text_sm())
            .child(div().text_xs().text_color(muted).child("Location"))
            .on_click(cx.listener(move |this, _, _, cx| match &url {
                Some(url) => {
                    if !quill::platform::open_external_url(url) {
                        this.connection.status_note = "Couldn't open the map".into();
                        cx.notify();
                    }
                }
                None => this.copy_profile_text(&address, "Address copied to clipboard", cx),
            }))
            .into_any_element()
    }

    fn business_hours_row(&self, info: &BusinessInfo, cx: &mut Context<Self>) -> AnyElement {
        let ui = &self.dialogs.business_hours;
        let distinct = info.has_distinct_local_hours();
        let use_business = ui.business_time && distinct;
        let expanded = ui.expanded;
        let shown: OpeningHours = if use_business {
            info.opening_hours.clone()
        } else {
            info.local_opening_hours
                .clone()
                .or(info.opening_hours.clone())
        }
        .unwrap_or_default();
        let local_minute = now_week_minute();
        let until_open = info.minutes_until_open(local_minute);
        let is_open = until_open == Some(0);
        let today = usize::from(civil_local(now_unix()).weekday.min(6));
        let days = shown.days();
        // Today's hours only mean something when the shown schedule is in
        // the viewer's zone; otherwise expanded view lists every day.
        let timing = match until_open {
            Some(n) if n > 0 && !expanded => opens_in_text(n),
            _ => format_day(&days[today]).replace('\n', ", "),
        };
        let theme = cx.theme();
        let (muted, hover, border) = (theme.muted_foreground, theme.secondary, theme.border);
        let state_color = if is_open { theme.success } else { theme.danger };
        let state = if until_open.is_none() {
            None
        } else if is_open {
            Some("Open")
        } else {
            Some("Closed")
        };
        let switch_label = if use_business {
            "my time"
        } else {
            "local time"
        };
        let mut column = div()
            .id("info-business-hours")
            .flex()
            .flex_col()
            .px_3()
            .py_2()
            .border_t_1()
            .border_color(border)
            .cursor_pointer()
            .role(gpui_kit::Role::Button)
            .aria_label(format!(
                "Business hours: {}, {timing}",
                state.unwrap_or("hours")
            ))
            .tab_index(0)
            .hover(move |style| style.bg(hover))
            .on_click(cx.listener(|this, _, _, cx| {
                let ui = &mut this.dialogs.business_hours;
                ui.expanded = !ui.expanded;
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .gap_2()
                    .text_sm()
                    .child(div().when_some(state, |this, state| {
                        this.text_color(state_color).child(state)
                    }))
                    .child(div().text_color(muted).child(timing)),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .text_xs()
                    .text_color(muted)
                    .child("Business hours")
                    .when(distinct, |this| {
                        this.child(
                            div()
                                .id("info-business-hours-zone")
                                .cursor_pointer()
                                .text_color(cx.theme().accent_foreground)
                                .child(switch_label)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    let ui = &mut this.dialogs.business_hours;
                                    ui.business_time = !use_business;
                                    ui.expanded = true;
                                    cx.notify();
                                })),
                        )
                    }),
            );
        if expanded {
            let mut list = div().flex().flex_col().gap_1().pt_2().text_sm();
            for (index, day) in days.iter().enumerate() {
                let bold = index == today;
                list = list.child(
                    div()
                        .flex()
                        .justify_between()
                        .gap_2()
                        .child(
                            div()
                                .when(bold, |this| this.font_weight(FontWeight::SEMIBOLD))
                                .child(weekday_name(index as u8)),
                        )
                        .child(div().text_right().child(format_day(day))),
                );
            }
            column = column.child(list);
        }
        column.into_any_element()
    }
}
