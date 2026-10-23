//! Push-to-talk release delay slider in Settings > Calls (tdesktop
//! `calls_group_settings.cpp`: a media slider with 201 positions, 0 to 2 s in
//! 10 ms steps, and a "Release delay: N ms" label). The positions and the
//! label text live in `quill::calls::ptt`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::*;
use quill::calls::ptt::{DELAY_STEPS, delay_by_index, delay_label, index_for_delay};
use quill::lang::{keys, tr};

impl QuillApp {
    /// Create the slider on first use, seeded from the saved delay.
    pub(super) fn ensure_ptt_delay_slider(&mut self, cx: &mut Context<Self>) {
        if self.group_call.ptt_delay_slider.is_some() {
            return;
        }
        let saved = self
            .session()
            .map(|s| s.calls.prefs.push_to_talk.sanitized().release_delay_ms)
            .unwrap_or_default();
        let slider = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max((DELAY_STEPS - 1) as f32)
                .step(1.0)
                .default_value(index_for_delay(saved) as f32)
        });
        cx.subscribe(&slider, |this, _, event: &SliderEvent, cx| match event {
            // The label follows the drag; the file is written on release.
            SliderEvent::Change(_) => cx.notify(),
            SliderEvent::Release(value) => {
                let ms = delay_by_index(value.end().round().max(0.0) as u32);
                this.set_call_pref(|prefs| prefs.push_to_talk.release_delay_ms = ms, cx);
            }
        })
        .detach();
        self.group_call.ptt_delay_slider = Some(slider);
    }

    /// Label and slider for the release delay.
    pub(super) fn ptt_delay_row(&self, saved_ms: u32, cx: &mut Context<Self>) -> AnyElement {
        let shown = self
            .group_call
            .ptt_delay_slider
            .as_ref()
            .map(|slider| delay_by_index(slider.read(cx).value().end().round().max(0.0) as u32))
            .unwrap_or(saved_ms);
        let mut row = div()
            .id("call-pref-ptt-delay")
            .flex()
            .flex_col()
            .gap_1()
            .px_2()
            .pt_1()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .child(delay_label(shown)),
            );
        if let Some(slider) = self.group_call.ptt_delay_slider.as_ref() {
            row = row.child(
                div()
                    .id("call-pref-ptt-delay-slider")
                    .role(Role::Group)
                    .aria_label(tr(keys::Q_LEFTOVERS_PUSH_TO_TALK_RELEASE_DELAY))
                    .child(Slider::new(slider).bg(cx.theme().primary)),
            );
        }
        row.into_any_element()
    }
}
