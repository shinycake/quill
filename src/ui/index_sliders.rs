//! Step sliders of the Data & Storage settings and the auto-delete picker.
//!
//! tdesktop picks one of a fixed list of values with a slider
//! (`Ui::MediaSlider::setPseudoDiscrete`): the auto-download size caps
//! (`boxes/auto_download_box.cpp`, `AddSizeLimitSlider`) and the local storage
//! limits (`settings/sections/settings_local_storage.cpp`,
//! `createLimitsSlider`). Here each is a kit `Slider` over the step index,
//! `0..steps`, with a step of 1. The slider only chooses the position; the
//! value shown next to it comes from the step list (`quill::limit_steps`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::data_settings::{CapKind, NO_LIMIT_SIZE_CAP};
use quill::limit_steps::{CAP_STEPS, KEEP_STEPS, SIZE_STEPS, cap_at, cap_index};
use quill::limit_steps::{keep_limit_at, size_limit_at};
use std::collections::HashMap;

/// Which step slider an event or a row belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum StepSlider {
    /// The auto-download size cap of one media type (network editor).
    DataCap(CapKind),
    /// Local storage "Total size limit".
    StorageSize,
    /// Local storage "Clear files older than".
    StorageKeep,
    /// The auto-delete timer of the chat the picker is open for.
    AutoDelete,
}

impl StepSlider {
    /// How many positions the slider has.
    pub(super) fn steps(self) -> usize {
        match self {
            Self::DataCap(_) => CAP_STEPS,
            Self::StorageSize => SIZE_STEPS,
            Self::StorageKeep => KEEP_STEPS,
            // Step 0 is Off, then the fixed periods.
            Self::AutoDelete => quill::auto_delete::TTL_CHOICES.len() + 1,
        }
    }
}

/// The live slider entities, by role.
#[derive(Default)]
pub(crate) struct IndexSliders {
    map: HashMap<StepSlider, Entity<SliderState>>,
}

impl IndexSliders {
    pub(super) fn get(&self, key: StepSlider) -> Option<&Entity<SliderState>> {
        self.map.get(&key)
    }

    pub(super) fn contains(&self, key: StepSlider) -> bool {
        self.map.contains_key(&key)
    }

    pub(super) fn remove(&mut self, key: StepSlider) {
        self.map.remove(&key);
    }

    /// The position the slider is on, `None` while it does not exist.
    pub(super) fn index(&self, key: StepSlider, cx: &App) -> Option<usize> {
        self.get(key)
            .map(|slider| position(slider.read(cx).value().end(), key.steps()))
    }
}

/// A slider value as a step index.
fn position(value: f32, steps: usize) -> usize {
    let top = steps.saturating_sub(1);
    // The clamp keeps the cast in range; NaN rounds to 0.
    value.round().clamp(0.0, top as f32) as usize
}

impl QuillApp {
    /// Create (or recreate) the slider for `key` on step `start`.
    pub(super) fn set_step_slider(
        &mut self,
        key: StepSlider,
        start: usize,
        cx: &mut Context<Self>,
    ) {
        let top = key.steps().saturating_sub(1);
        let slider = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(top as f32)
                .step(1.0)
                .default_value(start.min(top) as f32)
        });
        cx.subscribe(&slider, move |this, _, event: &SliderEvent, cx| {
            this.on_step_slider(key, event, cx);
        })
        .detach();
        self.settings.index_sliders.map.insert(key, slider);
    }

    /// A slider moved (`Change`, while dragging) or was let go (`Release`).
    fn on_step_slider(&mut self, key: StepSlider, event: &SliderEvent, cx: &mut Context<Self>) {
        let (SliderEvent::Change(value) | SliderEvent::Release(value)) = event;
        let index = position(value.end(), key.steps());
        match key {
            StepSlider::DataCap(kind) => {
                if let Some((_, draft)) = self.settings.data_storage_editor.as_mut() {
                    draft.set_cap(kind, cap_at(index));
                }
            }
            StepSlider::StorageSize | StepSlider::StorageKeep => {
                // tdesktop applies on every move; one `setOption` round per
                // drag step would flood TDLib, so the release applies.
                if matches!(event, SliderEvent::Release(_)) {
                    self.apply_storage_sliders(cx);
                }
            }
            StepSlider::AutoDelete => {}
        }
        cx.notify();
    }

    /// The limits the two storage sliders are on.
    pub(super) fn storage_slider_limits(&self, cx: &App) -> (Option<i64>, Option<i64>) {
        let sliders = &self.settings.index_sliders;
        let size = sliders
            .index(StepSlider::StorageSize, cx)
            .and_then(size_limit_at);
        let keep = sliders
            .index(StepSlider::StorageKeep, cx)
            .and_then(keep_limit_at);
        (size, keep)
    }

    /// The label above a slider: the title on the left, the value on the
    /// right (tdesktop `createLimitsSlider`), then the slider.
    pub(super) fn step_slider_row(
        &self,
        key: StepSlider,
        id: &'static str,
        title: String,
        value: String,
        cx: &App,
    ) -> Option<AnyElement> {
        let slider = self.settings.index_sliders.get(key)?;
        Some(
            div()
                .id(id)
                .test_support()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(div().text_sm().child(title.clone()))
                        .child(
                            div()
                                .text_sm()
                                .font_semibold()
                                .text_color(cx.theme().primary)
                                .child(value),
                        ),
                )
                .child(Slider::new(slider).accessibility_label(title))
                .into_any_element(),
        )
    }
}

/// The step the cap slider starts on: "No limit" sits on the last step.
pub(super) fn cap_slider_start(bytes: i64) -> usize {
    if bytes >= NO_LIMIT_SIZE_CAP {
        CAP_STEPS - 1
    } else {
        cap_index(bytes)
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "index_sliders_tests.rs"]
mod live_tests;

#[cfg(test)]
mod tests {
    use super::{cap_slider_start, position};
    use quill::data_settings::NO_LIMIT_SIZE_CAP;
    use quill::limit_steps::CAP_STEPS;

    #[test]
    fn positions_round_and_clamp() {
        assert_eq!(position(0.0, 100), 0);
        assert_eq!(position(3.4, 100), 3);
        assert_eq!(position(3.6, 100), 4);
        assert_eq!(position(250.0, 100), 99);
        assert_eq!(position(-4.0, 100), 0);
        assert_eq!(position(f32::NAN, 100), 0);
    }

    #[test]
    fn no_limit_starts_on_the_last_step() {
        assert_eq!(cap_slider_start(NO_LIMIT_SIZE_CAP), CAP_STEPS - 1);
        assert_eq!(cap_slider_start(1024 * 1024), 0);
    }
}
