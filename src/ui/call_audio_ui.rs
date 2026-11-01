//! Microphone levels in the UI: the "Test microphone" meter in Settings >
//! Calls (tdesktop `settings_calls.cpp` + `Ui::LevelMeter`) and your own
//! level for the voice chat window (read from the driver's level tap,
//! `connect::call_audio`). The rules and numbers are in
//! `quill::calls::audio_level`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme, Sizable};
use gpui_kit::*;
use quill::calls::audio_level::{
    LevelAnimation, METER_HEIGHT, METER_LINE_COUNT, METER_LINE_SPACING, METER_LINE_WIDTH,
    MIC_TEST_ANIMATION_MS, MIC_TEST_UPDATE_MS, meter_lit_lines, peak_from_level,
};
use quill::calls::level_tap::{LevelSource, LevelTap};
use quill::lang::{keys, tr};
use std::cell::Cell;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Settings > Calls' microphone test. While the device opens, a click
/// cancels: the test shows as running from the click on, and an answer for
/// a start that is no longer current is dropped, closing the device.
#[derive(Default)]
pub(crate) enum MicTestRun {
    #[default]
    Off,
    Opening(u64),
    On(MicTest),
}

/// Opens the microphone for the test, off the UI thread.
pub(crate) type OpenMicTest =
    Arc<dyn Fn() -> Result<Box<dyn LevelSource + Send>, String> + Send + Sync>;

pub(super) fn open_default_mic_test() -> OpenMicTest {
    Arc::new(|| LevelTap::open_default().map(|tap| Box::new(tap) as Box<dyn LevelSource + Send>))
}

/// A running microphone test: the open device and the meter's animation.
pub(crate) struct MicTest {
    source: Box<dyn LevelSource + Send>,
    clock: Instant,
    /// The meter value, animated between samples (tdesktop
    /// `kMicTestAnimationDuration`).
    anim: Cell<LevelAnimation>,
    last_sample: Cell<Instant>,
    /// When the meter last rendered; the watchdog stops a test whose
    /// section went away (another tab, another settings page).
    last_render: Cell<Instant>,
}

/// Without a render for this long, the test stops.
const UNSEEN_STOP: Duration = Duration::from_secs(1);

impl MicTest {
    fn new(source: Box<dyn LevelSource + Send>) -> Self {
        let now = Instant::now();
        Self {
            source,
            clock: now,
            anim: Cell::new(LevelAnimation::new(0.0, MIC_TEST_ANIMATION_MS)),
            last_sample: Cell::new(now),
            last_render: Cell::new(now),
        }
    }

    fn now_ms(&self) -> u64 {
        u64::try_from(self.clock.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// The meter value for this frame: every `kMicTestUpdateInterval`
    /// the loudest level since the last sample becomes the new target.
    fn meter_value(&self) -> f32 {
        let now = self.now_ms();
        if self.last_sample.get().elapsed() >= Duration::from_millis(MIC_TEST_UPDATE_MS) {
            self.last_sample.set(Instant::now());
            let mut anim = self.anim.get();
            let target = self
                .source
                .take_level()
                .map(peak_from_level)
                .unwrap_or_else(|| anim.target());
            anim.retarget(target, now);
            self.anim.set(anim);
        }
        self.anim.get().value(now)
    }
}

impl QuillApp {
    /// The "Test microphone" row for Settings > Calls: a button, and the
    /// meter while the test runs.
    pub(super) fn mic_test_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let testing = !matches!(self.calls.mic_test, MicTestRun::Off);
        let test = match &self.calls.mic_test {
            MicTestRun::On(test) => Some(test),
            _ => None,
        };
        let error = test.and_then(|test| test.source.error());
        let row = div()
            .id("call-mic-test")
            .flex()
            .flex_col()
            .gap_1()
            .px_2()
            .py_1()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("call-mic-test-toggle")
                            .label(if testing {
                                tr(keys::Q_OPS_STOP_TEST)
                            } else {
                                tr(keys::Q_OPS_TEST_MICROPHONE)
                            })
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_mic_test(cx))),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(keys::Q_OPS_SPEAK_TO_SEE_YOUR_MICROPHONES)),
                    ),
            );
        let Some(test) = test else {
            return row.into_any_element();
        };
        test.last_render.set(Instant::now());
        self.request_animation_tick(30, cx);
        if let Some(error) = error {
            return row
                .child(div().text_xs().text_color(cx.theme().danger).child(error))
                .into_any_element();
        }
        let value = test.meter_value();
        let lit = meter_lit_lines(value, METER_LINE_COUNT);
        let (active, inactive) = (cx.theme().primary, cx.theme().border);
        row.child(
            div()
                .id("call-mic-test-meter")
                .h(px(METER_HEIGHT))
                .flex()
                .items_center()
                .gap(px(METER_LINE_SPACING))
                .overflow_hidden()
                .children((0..METER_LINE_COUNT).map(|i| {
                    div()
                        .flex_none()
                        .w(px(METER_LINE_WIDTH))
                        .h_full()
                        .rounded(px(METER_LINE_WIDTH / 2.))
                        .bg(if i < lit { active } else { inactive })
                })),
        )
        .into_any_element()
    }

    /// Start or stop the microphone test; a click while it opens stops it.
    pub(super) fn toggle_mic_test(&mut self, cx: &mut Context<Self>) {
        if matches!(self.calls.mic_test, MicTestRun::Off) {
            self.open_mic_test(cx);
        } else {
            self.calls.mic_test = MicTestRun::Off;
        }
        cx.notify();
    }

    fn open_mic_test(&mut self, cx: &mut Context<Self>) {
        self.calls.mic_test_starts = self.calls.mic_test_starts.wrapping_add(1);
        let start = self.calls.mic_test_starts;
        self.calls.mic_test = MicTestRun::Opening(start);
        let open = self.calls.open_mic_test.clone();
        let opening = cx.background_spawn(async move { open() });
        Self::spawn_main(cx, async move |this, cx| {
            let opened = opening.await;
            let _ = this.update(cx, |this, cx| {
                if !matches!(this.calls.mic_test, MicTestRun::Opening(s) if s == start) {
                    if let Ok(stale) = opened {
                        cx.background_spawn(async move { drop(stale) }).detach();
                    }
                    return;
                }
                match opened {
                    Ok(tap) => this.start_mic_test(tap, cx),
                    Err(err) => {
                        this.calls.mic_test = MicTestRun::Off;
                        this.connection.set_note(StatusNote::error(err));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn start_mic_test(&mut self, source: Box<dyn LevelSource + Send>, cx: &mut Context<Self>) {
        self.calls.mic_test = MicTestRun::On(MicTest::new(source));
        // Stop the test, and close the microphone, once its meter stops
        // rendering: the Calls section is gone, so nobody is watching.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                let keep = this
                    .update(cx, |this, cx| match &this.calls.mic_test {
                        MicTestRun::On(test) if test.last_render.get().elapsed() < UNSEEN_STOP => {
                            true
                        }
                        MicTestRun::On(_) => {
                            this.calls.mic_test = MicTestRun::Off;
                            cx.notify();
                            false
                        }
                        _ => false,
                    })
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
        })
        .detach();
    }

    /// Your own level and speaking state in the voice chat, from the
    /// driver's tap.
    pub(super) fn group_call_self_level(&self) -> (f32, bool) {
        if let Some(live) = self.live.as_ref() {
            return live.driver.group_call_self_level();
        }
        (0.0, false)
    }

    /// Milliseconds on the voice chat window's clock (the push-to-talk
    /// clock), for the level animation (`LEVEL_ANIMATION_MS`).
    pub(super) fn group_call_now_ms(&self) -> u64 {
        u64::try_from(self.group_call.ptt_clock.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "call_audio_ui_tests.rs"]
mod tests;
