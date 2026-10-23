//! The discrete sliders of the permissions box: tdesktop's slow mode
//! slider (`AddSlowmodeSlider`) and the boosters slider
//! (`AddBoostsUnrestrictSlider`), each with a label under every stop. The
//! kit `Slider` carries the stop index; the chosen value is reported on
//! release and the thumb follows the value the caller passes in.
#![deny(clippy::indexing_slicing)]

use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with, trn};
use std::rc::Rc;

/// The stop for a value; a value between stops snaps to the next lower one.
pub(super) fn stop_index(stops: &[i32], value: i32) -> usize {
    stops.iter().rposition(|stop| *stop <= value).unwrap_or(0)
}

/// The value of a slider position (clamped to the stops).
pub(super) fn value_for(stops: &[i32], position: f32) -> i32 {
    let index = position.round().max(0.0) as usize;
    stops.get(index).or(stops.last()).copied().unwrap_or(0)
}

/// The label under a slow mode stop: "Off", "5s", "1m", "1h" (`lng_*_tiny`).
pub(super) fn slow_mode_label(seconds: i32) -> String {
    if seconds <= 0 {
        tr(keys::RIGHTS_SLOWMODE_OFF).to_string()
    } else if seconds < 60 {
        trn(keys::SECONDS_TINY, seconds)
    } else if seconds < 3600 {
        trn(keys::MINUTES_TINY, seconds / 60)
    } else {
        trn(keys::HOURS_TINY, seconds / 3600)
    }
}

/// The sentence under the slow mode slider: the plain explanation when
/// off, else "Members will be able to send only one message every 30
/// seconds."
pub(super) fn slow_mode_about(seconds: i32) -> String {
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

/// The keyed-state names of one slider kind.
#[derive(Clone, Copy)]
pub(super) struct SliderKeys {
    slider: &'static str,
    synced: &'static str,
    release: &'static str,
}

pub(super) const SLOW_MODE_KEYS: SliderKeys = SliderKeys {
    slider: "slow-mode-slider",
    synced: "slow-mode-synced",
    release: "slow-mode-release",
};

pub(super) const BOOSTS_KEYS: SliderKeys = SliderKeys {
    slider: "boosts-slider",
    synced: "boosts-synced",
    release: "boosts-release",
};

/// A slider over fixed stops with their labels.
#[derive(IntoElement)]
pub(super) struct StopsSlider {
    keys: SliderKeys,
    chat: u64,
    stops: &'static [i32],
    value: i32,
    label: fn(i32) -> String,
    accessibility_label: SharedString,
    on_change: Rc<dyn Fn(i32, &mut App)>,
    /// Tests: receives the slider entity (keyed state only a render reaches).
    #[cfg(test)]
    probe: Option<Rc<std::cell::RefCell<Option<Entity<SliderState>>>>>,
}

impl StopsSlider {
    pub(super) fn new(
        keys: SliderKeys,
        chat: i64,
        stops: &'static [i32],
        value: i32,
        label: fn(i32) -> String,
        on_change: impl Fn(i32, &mut App) + 'static,
    ) -> Self {
        Self {
            keys,
            chat: chat as u64,
            stops,
            value,
            label,
            accessibility_label: SharedString::default(),
            on_change: Rc::new(on_change),
            #[cfg(test)]
            probe: None,
        }
    }

    pub(super) fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.accessibility_label = label.into();
        self
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

impl RenderOnce for StopsSlider {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let last = self.stops.len().saturating_sub(1) as f32;
        let slider = window.use_keyed_state((self.keys.slider, self.chat), cx, |_, _| {
            SliderState::new().min(0.0).max(last).step(1.0)
        });
        let synced = window.use_keyed_state((self.keys.synced, self.chat), cx, |_, _| i32::MIN);
        #[cfg(test)]
        if let Some(probe) = &self.probe {
            *probe.borrow_mut() = Some(slider.clone());
        }
        // Report a release once per change, and only when it differs from
        // the value passed in.
        let on_change = self.on_change.clone();
        let release_synced = synced.clone();
        let stops = self.stops;
        let _subscription = window.use_keyed_state((self.keys.release, self.chat), cx, |_, cx| {
            cx.subscribe(&slider, move |_, _, event: &SliderEvent, cx| {
                if let SliderEvent::Release(position) = event {
                    let value = value_for(stops, position.end());
                    if value != *release_synced.read(cx) {
                        on_change(value, cx);
                    }
                }
            })
        });
        // Follow the value passed in; it only changes once a release was
        // reported, so a drag is never interrupted.
        if *synced.read(cx) != self.value {
            synced.update(cx, |synced, _| *synced = self.value);
            let index = stop_index(self.stops, self.value) as f32;
            slider.update(cx, |state, cx| state.set_value(index, window, cx));
        }
        let selected = value_for(self.stops, slider.read(cx).value().end());
        let muted = cx.theme().muted_foreground;
        let foreground = cx.theme().foreground;
        let label = self.label;
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
                    .children(self.stops.iter().map(|stop| {
                        div()
                            .text_xs()
                            .text_color(if *stop == selected { foreground } else { muted })
                            .when(*stop == selected, |this| this.font_semibold())
                            .child(label(*stop))
                    })),
            )
            .child(
                div()
                    .px_1()
                    .child(Slider::new(&slider).accessibility_label(self.accessibility_label)),
            )
    }
}
