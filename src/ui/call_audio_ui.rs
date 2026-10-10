//! Microphone levels in the UI: the "Test microphone" meter in Settings >
//! Calls (tdesktop `settings_calls.cpp` + `Ui::LevelMeter`) and your own
//! level for the voice chat window (read from the driver's level tap,
//! `connect::call_audio`). The rules and numbers are in
//! `quill::calls::audio_level`.

use super::app::QuillApp;
use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme, Sizable};
use gpui_kit::*;
use quill::calls::audio_level::{
    LevelAnimation, METER_HEIGHT, METER_LINE_COUNT, METER_LINE_SPACING, METER_LINE_WIDTH,
    MIC_TEST_ANIMATION_MS, MIC_TEST_UPDATE_MS, VOICE_LEVEL, meter_lit_lines, peak_from_level,
};
use quill::calls::level_tap::{FixedLevel, LevelSource, LevelTap};
use std::cell::Cell;
use std::time::{Duration, Instant};

/// A running microphone test: the open device and the meter's animation.
pub(crate) struct MicTest {
    source: Box<dyn LevelSource>,
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
    fn new(source: Box<dyn LevelSource>) -> Self {
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
        let test = self.calls.mic_test.as_ref();
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
                            .label(if test.is_some() {
                                "Stop test"
                            } else {
                                "Test microphone"
                            })
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_mic_test(cx))),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Speak to see your microphone's level"),
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

    /// Start or stop the microphone test.
    pub(super) fn toggle_mic_test(&mut self, cx: &mut Context<Self>) {
        if self.calls.mic_test.is_some() {
            self.calls.mic_test = None;
        } else {
            match LevelTap::open_default() {
                Ok(tap) => self.start_mic_test(Box::new(tap), cx),
                Err(err) => self.connection.status_note = err,
            }
        }
        cx.notify();
    }

    fn start_mic_test(&mut self, source: Box<dyn LevelSource>, cx: &mut Context<Self>) {
        self.calls.mic_test = Some(MicTest::new(source));
        // Stop the test, and close the microphone, once its meter stops
        // rendering: the Calls section is gone, so nobody is watching.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                let keep = this
                    .update(cx, |this, cx| match this.calls.mic_test.as_ref() {
                        Some(test) if test.last_render.get().elapsed() < UNSEEN_STOP => true,
                        Some(_) => {
                            this.calls.mic_test = None;
                            cx.notify();
                            false
                        }
                        None => false,
                    })
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
        })
        .detach();
    }

    /// Screenshot demos: the test running with a fixed peak (`0..=1` of
    /// full scale), no device opened and no watchdog.
    pub(super) fn start_mic_test_demo(&mut self, peak: f32) {
        let level = peak * 32768.0 / quill::calls::audio_level::LEVEL_PEAK_DIVISOR;
        let test = MicTest::new(Box::new(FixedLevel(std::sync::Mutex::new(level))));
        test.anim.set(LevelAnimation::new(
            peak.clamp(0.0, 1.0),
            MIC_TEST_ANIMATION_MS,
        ));
        self.calls.mic_test = Some(test);
    }

    /// Your own level and speaking state in the voice chat: the driver's
    /// tap, or the demo level.
    pub(super) fn group_call_self_level(&self) -> (f32, bool) {
        if let Some(live) = self.live.as_ref() {
            return live.driver.group_call_self_level();
        }
        self.group_call
            .demo_level
            .map(|level| (level, level >= VOICE_LEVEL))
            .unwrap_or((0.0, false))
    }

    /// Milliseconds on the voice chat window's clock (the push-to-talk
    /// clock), for the level animation (`LEVEL_ANIMATION_MS`).
    pub(super) fn group_call_now_ms(&self) -> u64 {
        u64::try_from(self.group_call.ptt_clock.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}
