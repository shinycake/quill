//! A bot's affiliate program setup (tdesktop `BotStarRef::Setup`): the
//! commission and duration sliders, "View existing programs", "End
//! Affiliate Program" and the Start / Update button.

use super::*;
use crate::ui::chat_theme::danger;
use crate::ui::os_open::open_link_in;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::prelude::FluentBuilder;
use quill::earn::*;
use quill::lang::{keys, tr, tr_with};

/// tdesktop `lng_star_ref_tos_url`.
pub(super) const STARREF_TOS_URL: &str = "https://telegram.org/tos/mini-apps";

/// Which setup slider an event came from.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Knob {
    Commission,
    Duration,
}

/// The stop index of `value`, or the nearest lower one.
fn index_of(stops: &[i32], value: i32) -> usize {
    stops.iter().rposition(|stop| *stop <= value).unwrap_or(0)
}

/// A slider position to a stop index, clamped to the stops.
fn slider_index(position: f32, count: usize) -> usize {
    let last = count.saturating_sub(1);
    if !position.is_finite() || position <= 0.0 {
        return 0;
    }
    // Positions are whole steps between 0 and `last`.
    (position.round() as usize).min(last)
}

/// The duration slider's value to the program's months (999 is forever,
/// which TDLib spells 0).
fn months_of(stop: i32) -> i32 {
    if stop == DURATION_FOREVER { 0 } else { stop }
}

/// The program's months to the duration slider's value.
fn stop_of_months(months: i32) -> i32 {
    if months == 0 {
        DURATION_FOREVER
    } else {
        months
    }
}

impl QuillApp {
    /// Create the sliders the first frame the setup page has its values
    /// (`SliderState` subscriptions need the window).
    pub(super) fn ensure_setup_sliders(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.earn.commission.is_some() && self.earn.duration.is_some() {
            return;
        }
        let Some(earn) = self.earn_state() else {
            return;
        };
        let Some(setup) = earn.setup.as_ref() else {
            return;
        };
        let (min, max) = earn.limits.commission_range();
        let commission = commission_stops(min, max, setup.params.commission_per_mille);
        let duration = duration_stops(setup.params.month_count);
        let commission_at = index_of(&commission, setup.params.commission_per_mille);
        let duration_at = index_of(&duration, stop_of_months(setup.params.month_count));
        let commission_slider = self.setup_slider(
            Knob::Commission,
            commission.len(),
            commission_at,
            window,
            cx,
        );
        let duration_slider =
            self.setup_slider(Knob::Duration, duration.len(), duration_at, window, cx);
        self.earn.commission = Some((commission_slider, commission));
        self.earn.duration = Some((duration_slider, duration));
    }

    fn setup_slider(
        &mut self,
        knob: Knob,
        count: usize,
        at: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<SliderState> {
        let last = count.saturating_sub(1) as f32;
        let entity = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(last.max(1.0))
                .step(1.0)
                .default_value(at as f32)
        });
        cx.subscribe_in(
            &entity,
            window,
            move |this, slider, event: &SliderEvent, window, cx| {
                let value = match event {
                    SliderEvent::Change(value) | SliderEvent::Release(value) => value.end(),
                };
                this.setup_slider_moved(knob, slider, value, window, cx);
            },
        )
        .detach();
        entity
    }

    /// A slider moved: store the stop; a running program cannot go below
    /// what it has (tdesktop `forbidLessThanValue`).
    fn setup_slider_moved(
        &mut self,
        knob: Knob,
        slider: &Entity<SliderState>,
        position: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let stops = match knob {
            Knob::Commission => self.earn.commission.as_ref().map(|(_, s)| s.clone()),
            Knob::Duration => self.earn.duration.as_ref().map(|(_, s)| s.clone()),
        };
        let Some(stops) = stops else {
            return;
        };
        let Some(setup) = self.earn_mut().and_then(|e| e.setup.as_mut()) else {
            return;
        };
        let floor = match (setup.exists(), setup.program) {
            (true, Some(program)) => match knob {
                Knob::Commission => index_of(&stops, program.params.commission_per_mille),
                Knob::Duration => index_of(&stops, stop_of_months(program.params.month_count)),
            },
            _ => 0,
        };
        let index = slider_index(position, stops.len());
        let kept = index.max(floor);
        let Some(stop) = stops.get(kept).copied() else {
            return;
        };
        match knob {
            Knob::Commission => setup.params.commission_per_mille = stop,
            Knob::Duration => setup.params.month_count = months_of(stop),
        }
        if kept != index {
            // The event is delivered after the slider's own update, so the
            // slider is free to take the floor back.
            slider.update(cx, |slider, cx| slider.set_value(kept as f32, window, cx));
        }
        cx.notify();
    }

    /// A title and its about line with an icon (tdesktop `infoRow`).
    fn setup_info_row(icon: IconName, title: &'static str, text: &'static str, cx: &App) -> Div {
        info_point(icon, title, text, cx)
    }

    /// The setup page.
    pub(super) fn setup_page(&self, bot_user_id: i64, cx: &mut Context<Self>) -> AnyElement {
        let Some(session) = self.session() else {
            return div().into_any_element();
        };
        let earn = &session.payments.earn;
        let Some(setup) = earn.setup.as_ref().filter(|s| s.bot_user_id == bot_user_id) else {
            return about(tr(keys::Q_MONEY_LOADING)).into_any_element();
        };
        let exists = setup.exists();
        let primary = cx.theme().primary;
        let mut page = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .pt_2()
                    .child(
                        div()
                            .size(px(64.))
                            .rounded_full()
                            .bg(primary)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                Icon::new(IconName::Handshake)
                                    .size(px(30.))
                                    .text_color(cx.theme().primary_foreground),
                            ),
                    )
                    .child(
                        div()
                            .text_lg()
                            .font_semibold()
                            .child(tr(keys::Q_MONEY_AFFILIATE_PROGRAM)),
                    )
                    .child(about(tr(keys::STAR_REF_ABOUT)).text_center()),
            )
            .child(Self::setup_info_row(
                IconName::Percent,
                tr(keys::STAR_REF_SHARE_TITLE),
                tr(keys::STAR_REF_SHARE_ABOUT),
                cx,
            ))
            .child(Self::setup_info_row(
                IconName::Megaphone,
                tr(keys::STAR_REF_LAUNCH_TITLE),
                tr(keys::STAR_REF_LAUNCH_ABOUT),
                cx,
            ))
            .child(Self::setup_info_row(
                IconName::Link,
                tr(keys::STAR_REF_LET_TITLE),
                tr(keys::STAR_REF_LET_ABOUT),
                cx,
            ))
            .child(header(tr(keys::Q_MONEY_COMMISSION), cx));
        if let Some((slider, stops)) = self.earn.commission.as_ref() {
            page = page.child(self.commission_slider(slider, stops, setup, cx));
        }
        page = page
            .child(about(tr(keys::STAR_REF_COMMISSION_ABOUT)))
            .child(header(tr(keys::Q_MONEY_DURATION), cx));
        if let Some((slider, stops)) = self.earn.duration.as_ref() {
            page = page.child(self.duration_slider(slider, stops, setup, cx));
        }
        page = page
            .child(about(tr(keys::STAR_REF_DURATION_ABOUT)))
            .child(
                div()
                    .id("earn-setup-existing")
                    .flex()
                    .items_center()
                    .gap_3()
                    .p_2()
                    .rounded_md()
                    .cursor_pointer()
                    .role(gpui_kit::Role::Button)
                    .aria_label(tr(keys::Q_MONEY_VIEW_EXISTING_PROGRAMS))
                    .tab_index(0)
                    .hover(|s| s.bg(cx.theme().secondary))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.earn_call(tr(keys::Q_MONEY_COULDNT_LOAD_THE_PROGRAMS), cx, |driver| {
                            driver.open_starref_programs_box()
                        });
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_sm()
                                    .font_semibold()
                                    .child(tr(keys::Q_MONEY_VIEW_EXISTING_PROGRAMS)),
                            )
                            .child(about(tr(keys::STAR_REF_EXISTING_ABOUT))),
                    )
                    .child(
                        Icon::new(IconName::ChevronRight)
                            .size(px(16.))
                            .text_color(text_muted()),
                    ),
            )
            .child(divider(cx));
        if exists {
            page = page
                .child(
                    div()
                        .id("earn-setup-end")
                        .p_2()
                        .rounded_md()
                        .cursor_pointer()
                        .role(gpui_kit::Role::Button)
                        .aria_label(tr(keys::Q_MONEY_END_AFFILIATE_PROGRAM))
                        .tab_index(0)
                        .hover(|s| s.bg(cx.theme().secondary))
                        .text_sm()
                        .text_color(danger())
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(setup) = this.earn_mut().and_then(|e| e.setup.as_mut()) {
                                setup.confirm = Some(SetupConfirm::End);
                            }
                            cx.notify();
                        }))
                        .child(tr(keys::Q_MONEY_END_AFFILIATE_PROGRAM)),
                )
                .child(divider(cx));
        }
        page.into_any_element()
    }

    /// The setup page's pinned bottom, under its scrolling part.
    pub(super) fn setup_footer(
        &self,
        bot_user_id: i64,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let setup = self
            .session()?
            .payments
            .earn
            .setup
            .as_ref()
            .filter(|s| s.bot_user_id == bot_user_id)?;
        Some(
            div()
                .id("earn-setup-bottom")
                .flex_none()
                .border_t_1()
                .border_color(cx.theme().border)
                .px_4()
                .pt_3()
                .pb_4()
                .child(self.setup_bottom(setup, cx))
                .into_any_element(),
        )
    }

    /// tdesktop `MakeSliderWithTopTag`: the lowest and highest commission
    /// at the ends, the current one above the knob.
    fn commission_slider(
        &self,
        slider: &Entity<SliderState>,
        stops: &[i32],
        setup: &SetupPage,
        cx: &App,
    ) -> Div {
        let current = setup.params.commission_per_mille;
        let first = stops.first().copied().unwrap_or(current);
        let last = stops.last().copied().unwrap_or(current);
        let at = index_of(stops, current);
        let tag = |value: i32, active: bool| {
            div()
                .text_xs()
                .when(active, |d| d.text_color(cx.theme().primary).font_semibold())
                .when(!active, |d| d.text_color(text_muted()))
                .child(format_commission(value))
        };
        let middle = at > 0 && at + 1 < stops.len();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .relative()
                    .h(px(16.))
                    .child(div().absolute().left_0().top_0().child(tag(first, at == 0)))
                    .child(
                        div()
                            .absolute()
                            .right_0()
                            .top_0()
                            .child(tag(last, at + 1 == stops.len())),
                    )
                    .when(middle, |d| {
                        // Above the knob, kept clear of the end labels.
                        let span = stops.len().saturating_sub(1).max(1) as f32;
                        let ratio = (at as f32 / span).clamp(0.12, 0.88);
                        d.child(
                            div()
                                .absolute()
                                .top_0()
                                .left(relative(ratio))
                                .ml(px(-12.))
                                .child(tag(current, true)),
                        )
                    }),
            )
            .child(Slider::new(slider).bg(cx.theme().primary))
    }

    /// tdesktop `MakeSliderWithTopLabels`: a label over every stop.
    fn duration_slider(
        &self,
        slider: &Entity<SliderState>,
        stops: &[i32],
        setup: &SetupPage,
        cx: &App,
    ) -> Div {
        let at = index_of(stops, stop_of_months(setup.params.month_count));
        let mut labels = div().flex().justify_between();
        for (index, stop) in stops.iter().enumerate() {
            labels = labels.child(
                div()
                    .text_xs()
                    .when(index == at, |d| {
                        d.text_color(cx.theme().primary).font_semibold()
                    })
                    .when(index != at, |d| d.text_color(text_muted()))
                    .child(duration_tick(*stop)),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(labels)
            .child(Slider::new(slider).bg(cx.theme().primary))
    }

    /// The Start / Update button and the terms (tdesktop `setupBottom`
    /// with `MakeStartButton`'s countdown after an ended program).
    fn setup_bottom(&self, setup: &SetupPage, cx: &mut Context<Self>) -> Div {
        let exists = setup.exists();
        let end_date = setup.program.map_or(0, |p| i64::from(p.end_date));
        let now = quill::local_time::now_unix();
        let left = (end_date - now).max(0);
        let locked = left > 0;
        if locked {
            // tdesktop redraws the countdown every 100 ms.
            self.request_animation_tick(10, cx);
        }
        let label = if exists && !locked {
            tr(keys::Q_MONEY_UPDATE_AFFILIATE_PROGRAM)
        } else {
            tr(keys::Q_MONEY_START_AFFILIATE_PROGRAM)
        };
        let mut button = div().flex().flex_col().items_center().w_full().child(
            Button::new("earn-setup-save")
                .debug_selector(|| "earn-setup-save".into())
                .label(label)
                .primary()
                .w_full()
                .disabled(locked || setup.saving)
                .loading(setup.saving)
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(setup) = this.earn_mut().and_then(|e| e.setup.as_mut()) {
                        setup.confirm = Some(SetupConfirm::Save);
                    }
                    cx.notify();
                })),
        );
        if locked {
            button = button.child(
                about(tr_with(
                    keys::Q_MONEY_AVAILABLE_IN,
                    &[("format_time_left", &(format_time_left(left)).to_string())],
                ))
                .pt_1(),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(button)
            .child(about_link(
                "earn-setup-terms",
                if exists {
                    tr(keys::Q_MONEY_BY_UPDATING_AN_AFFILIATE_PROGRAM)
                } else {
                    tr(keys::Q_MONEY_BY_CREATING_AN_AFFILIATE_PROGRAM)
                },
                tr(keys::Q_MONEY_TERMS_AND_CONDITIONS),
                tr(keys::Q_MONEY_OF_AFFILIATE_PROGRAMS),
                {
                    let app = cx.weak_entity();
                    move |_, cx| open_link_in(&app, STARREF_TOS_URL, cx)
                },
                cx,
            ))
    }
}

#[cfg(test)]
mod unit {
    use super::{index_of, months_of, slider_index, stop_of_months};

    #[test]
    fn stops_map_both_ways() {
        let stops = [1, 3, 6, 12, 24, 36, 999];
        assert_eq!(index_of(&stops, 6), 2);
        assert_eq!(index_of(&stops, 7), 2);
        assert_eq!(index_of(&stops, 0), 0);
        assert_eq!(slider_index(3.4, stops.len()), 3);
        assert_eq!(slider_index(42.0, stops.len()), 6);
        assert_eq!(slider_index(f32::NAN, stops.len()), 0);
        assert_eq!(slider_index(-1.0, 0), 0);
        assert_eq!(months_of(999), 0);
        assert_eq!(stop_of_months(0), 999);
        assert_eq!(months_of(12), 12);
    }
}
