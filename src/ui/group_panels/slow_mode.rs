//! The slow-mode control of the group info panel: tdesktop's discrete
//! slider (`AddSlowmodeSlider`, `edit_peer_permissions_box.cpp`) with a
//! label under every stop and the interval sentence below it. The kit
//! `Slider` carries the stop index; the new delay is sent on release.
#![deny(clippy::indexing_slicing)]

use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with, trn};
use std::rc::Rc;

/// `SlowmodeDelayByIndex`: the delays TDLib accepts, in slider order.
pub(super) const SLOW_MODE_STOPS: [i32; 8] = [0, 5, 10, 30, 60, 300, 900, 3600];

/// The stop for a delay; a delay between stops snaps to the next lower one.
pub(super) fn stop_index(delay: i32) -> usize {
    SLOW_MODE_STOPS
        .iter()
        .rposition(|stop| *stop <= delay)
        .unwrap_or(0)
}

/// The delay of a slider value (clamped to the stops).
pub(super) fn delay_for_value(value: f32) -> i32 {
    let index = value.round().max(0.0) as usize;
    SLOW_MODE_STOPS
        .get(index)
        .or(SLOW_MODE_STOPS.last())
        .copied()
        .unwrap_or(0)
}

/// The label under a stop: "Off", "5s", "1m", "1h" (`lng_*_tiny`).
pub(super) fn stop_label(seconds: i32) -> String {
    if seconds <= 0 {
        tr(keys::Q_PEOPLE_OFF).to_string()
    } else if seconds < 60 {
        trn(keys::SECONDS_TINY, seconds)
    } else if seconds < 3600 {
        trn(keys::MINUTES_TINY, seconds / 60)
    } else {
        trn(keys::HOURS_TINY, seconds / 3600)
    }
}

/// The sentence under the slider: the plain explanation when off, else
/// "Members will be able to send only one message every 30 seconds."
pub(super) fn about_text(seconds: i32) -> String {
    if seconds <= 0 {
        return tr(keys::RIGHTS_SLOWMODE_ABOUT).to_string();
    }
    let interval = if seconds < 60 {
        trn(keys::RIGHTS_SLOWMODE_INTERVAL_SECONDS, seconds)
    } else {
        trn(keys::RIGHTS_SLOWMODE_INTERVAL_MINUTES, seconds / 60)
    };
    tr_with(
        keys::RIGHTS_SLOWMODE_ABOUT_INTERVAL,
        &[("interval", &interval)],
    )
}

/// The slider with its stop labels. `delay` is the delay the server
/// reports; the thumb follows it whenever it changes and `on_change` gets
/// the chosen delay when the user lets go of the thumb.
#[derive(IntoElement)]
pub(super) struct SlowModeSlider {
    chat: u64,
    delay: i32,
    on_change: Rc<dyn Fn(i32, &mut App)>,
    /// Tests: receives the slider entity (keyed state only a render reaches).
    #[cfg(test)]
    probe: Option<Rc<std::cell::RefCell<Option<Entity<SliderState>>>>>,
}

impl SlowModeSlider {
    pub(super) fn new(chat: i64, delay: i32, on_change: impl Fn(i32, &mut App) + 'static) -> Self {
        Self {
            chat: chat as u64,
            delay,
            on_change: Rc::new(on_change),
            #[cfg(test)]
            probe: None,
        }
    }

    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn probe(
        mut self,
        probe: Rc<std::cell::RefCell<Option<Entity<SliderState>>>>,
    ) -> Self {
        self.probe = Some(probe);
        self
    }
}

/// Keyed state of one chat's slider: (the slider, the delay the thumb was
/// last synced to, the subscription that reports releases).
pub(super) fn slider_state(
    chat: u64,
    window: &mut Window,
    cx: &mut App,
) -> (Entity<SliderState>, Entity<i32>) {
    let slider = window.use_keyed_state(("slow-mode-slider", chat), cx, |_, _| {
        SliderState::new()
            .min(0.0)
            .max((SLOW_MODE_STOPS.len() - 1) as f32)
            .step(1.0)
    });
    let synced = window.use_keyed_state(("slow-mode-synced", chat), cx, |_, _| i32::MIN);
    (slider, synced)
}

impl RenderOnce for SlowModeSlider {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let (slider, synced) = slider_state(self.chat, window, cx);
        #[cfg(test)]
        if let Some(probe) = &self.probe {
            *probe.borrow_mut() = Some(slider.clone());
        }
        // Report a release once per change, and only when it differs from
        // what the server has.
        let on_change = self.on_change.clone();
        let release_synced = synced.clone();
        let _subscription =
            window.use_keyed_state(("slow-mode-release", self.chat), cx, |_, cx| {
                cx.subscribe(&slider, move |_, _, event: &SliderEvent, cx| {
                    if let SliderEvent::Release(value) = event {
                        let delay = delay_for_value(value.end());
                        if delay != *release_synced.read(cx) {
                            on_change(delay, cx);
                        }
                    }
                })
            });
        // Follow the server value, but never while the user is dragging
        // (the value only changes when the server answers).
        if *synced.read(cx) != self.delay {
            synced.update(cx, |value, _| *value = self.delay);
            let index = stop_index(self.delay) as f32;
            slider.update(cx, |state, cx| state.set_value(index, window, cx));
        }
        let selected = delay_for_value(slider.read(cx).value().end());
        let muted = cx.theme().muted_foreground;
        let foreground = cx.theme().foreground;
        div()
            .flex()
            .flex_col()
            .w_full()
            .gap_1()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .w_full()
                    .children(SLOW_MODE_STOPS.iter().map(|stop| {
                        div()
                            .text_xs()
                            .text_color(if *stop == selected { foreground } else { muted })
                            .when(*stop == selected, |this| this.font_semibold())
                            .child(stop_label(*stop))
                    })),
            )
            .child(div().px_1().child(Slider::new(&slider)))
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(about_text(selected)),
            )
    }
}
