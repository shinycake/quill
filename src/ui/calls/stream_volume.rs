//! The stream volume slider: beside mute in the stream window's bar, and
//! in the pill under the live story header's sound toggle (tdesktop
//! `VideoStream::setVolume`, which starts at the media viewer's
//! `videoVolume` and writes back to it).
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::video_player::controls::VolumePart;
use gpui_kit::component::slider::{SliderEvent, SliderState};
use gpui_kit::*;
use quill::lang::{keys, tr};
use std::time::{Duration, Instant};

pub(crate) const KEY_STEP: f32 = 0.05;
/// tdesktop `kVolumeHideTimeoutShort` / `kVolumeHideTimeoutLong`.
const LEAVE_TOGGLE: Duration = Duration::from_millis(20);
const LEAVE_PILL: Duration = Duration::from_millis(200);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VolumeHost {
    StreamWindow,
    LiveStory,
}

/// The live story header's volume pill: open while the pointer is on the
/// sound toggle or the pill, while the slider is dragged (tdesktop's
/// slider grabs the mouse, so a drag may leave the pill), and while the
/// toggle has keyboard focus. Leaving closes it after tdesktop's delays,
/// so a diagonal move from the toggle into the pill keeps it open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct VolumePill {
    toggle: bool,
    pill: bool,
    dragging: bool,
    keyboard: bool,
    closes_at: Option<Instant>,
}

impl VolumePill {
    pub(crate) fn open(self, now: Instant) -> bool {
        self.held() || self.closes_at.is_some_and(|at| now < at)
    }

    fn held(self) -> bool {
        self.toggle || self.pill || self.dragging || self.keyboard
    }

    /// Returns the delay after which the pill closes, when this leave
    /// starts one.
    pub(crate) fn hover(
        &mut self,
        part: VolumePart,
        inside: bool,
        now: Instant,
    ) -> Option<Duration> {
        match part {
            VolumePart::Toggle => self.toggle = inside,
            VolumePart::Pill => self.pill = inside,
        }
        if inside || self.held() {
            self.closes_at = None;
            return None;
        }
        let delay = match part {
            VolumePart::Toggle => LEAVE_TOGGLE,
            VolumePart::Pill => LEAVE_PILL,
        };
        self.closes_at = Some(now + delay);
        Some(delay)
    }

    pub(crate) fn focus(&mut self, keyboard: bool) {
        self.keyboard = keyboard;
    }

    fn drag(&mut self, dragging: bool) {
        self.dragging = dragging;
    }
}

/// Up and Down while a live story plays through Quill: its volume, 5 %
/// at a time, as in the stream window. Intercepted, because Up and Down
/// are already bound (message selection) and bindings match before
/// `on_key_down`. Only the window that holds the story viewer.
pub(crate) fn intercept_live_story_volume_keys(window: &Window, cx: &mut Context<QuillApp>) {
    let main = window.window_handle();
    let app = cx.weak_entity();
    cx.intercept_keystrokes(move |event, window, cx| {
        let keystroke = &event.keystroke;
        if window.window_handle() != main || keystroke.modifiers.modified() {
            return;
        }
        let step = match keystroke.key.as_str() {
            "up" => KEY_STEP,
            "down" => -KEY_STEP,
            _ => return,
        };
        let handled = app
            .update(cx, |this, cx| this.step_live_story_volume(step, window, cx))
            .unwrap_or(false);
        if handled {
            cx.stop_propagation();
        }
    })
    .detach();
}

impl QuillApp {
    fn stream_volume_percent(&self) -> f32 {
        (self.playback.volume * 100.0).round()
    }

    pub(crate) fn stream_volume_shown(&self, call_id: Option<i32>) -> bool {
        match self.live.as_ref() {
            Some(live) => call_id.is_some_and(|id| live.driver.stream_has_volume(id)),
            None => self.demo_session.is_some(),
        }
    }

    fn volume_slot(&mut self, host: VolumeHost) -> &mut Option<Entity<SliderState>> {
        match host {
            VolumeHost::StreamWindow => &mut self.group_call.stream.volume,
            VolumeHost::LiveStory => &mut self.stories.live.volume,
        }
    }

    pub(crate) fn stream_volume_slider(
        &mut self,
        host: VolumeHost,
        call_id: Option<i32>,
        cx: &mut Context<Self>,
    ) -> Option<Entity<SliderState>> {
        if !self.stream_volume_shown(call_id) {
            *self.volume_slot(host) = None;
            return None;
        }
        let percent = self.stream_volume_percent();
        if let Some(slider) = self.volume_slot(host).as_ref()
            && (slider.read(cx).value().end() - percent).abs() < 0.5
        {
            return Some(slider.clone());
        }
        let slider = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(100.0)
                .step(1.0)
                .default_value(percent)
                .label(tr(keys::Q_AV_VOLUME_2))
        });
        cx.subscribe(&slider, move |this, _, event: &SliderEvent, cx| {
            let (value, dragging) = match event {
                SliderEvent::Change(value) => (value.end(), true),
                SliderEvent::Release(value) => (value.end(), false),
            };
            if host == VolumeHost::LiveStory {
                this.stories.live.volume_pill.drag(dragging);
            }
            this.set_playback_volume(value / 100.0, cx);
        })
        .detach();
        *self.volume_slot(host) = Some(slider.clone());
        Some(slider)
    }

    /// Returns whether the stream has a volume to step.
    pub(crate) fn step_stream_volume(
        &mut self,
        step: f32,
        call_id: Option<i32>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.stream_volume_shown(call_id) {
            return false;
        }
        let next = ((self.playback.volume + step) / KEY_STEP).round() * KEY_STEP;
        self.set_playback_volume(next.clamp(0.0, 1.0), cx);
        true
    }

    fn step_live_story_volume(
        &mut self,
        step: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.stories.viewer.is_open()
            || self.stories.live.watching.is_none()
            || self.story_text_input_open(window, cx)
        {
            return false;
        }
        let call_id = self
            .session()
            .and_then(|s| s.calls.story_stream.as_ref())
            .map(|call| call.id);
        self.step_stream_volume(step, call_id, cx)
    }
}

#[cfg(test)]
mod tests {
    use super::{LEAVE_PILL, LEAVE_TOGGLE, VolumePill};
    use crate::ui::video_player::controls::VolumePart;
    use std::time::{Duration, Instant};

    #[test]
    fn the_pill_stays_open_across_toggle_pill_and_drag() {
        let now = Instant::now();
        let mut pill = VolumePill::default();
        pill.hover(VolumePart::Toggle, true, now);
        assert!(pill.open(now), "opens on the toggle");
        pill.hover(VolumePart::Pill, true, now);
        pill.hover(VolumePart::Toggle, false, now);
        assert!(pill.open(now), "into the pill");
        pill.drag(true);
        assert_eq!(pill.hover(VolumePart::Pill, false, now), None);
        assert!(pill.open(now), "a drag holds it");
        pill.drag(false);
        assert!(!pill.open(now));
    }

    #[test]
    fn leaving_closes_after_tdesktops_delays() {
        let now = Instant::now();
        let mut pill = VolumePill::default();
        pill.hover(VolumePart::Toggle, true, now);
        assert_eq!(
            pill.hover(VolumePart::Toggle, false, now),
            Some(LEAVE_TOGGLE)
        );
        assert!(pill.open(now), "a diagonal move does not flicker it");
        assert!(!pill.open(now + LEAVE_TOGGLE));
        pill.hover(VolumePart::Pill, true, now);
        assert_eq!(pill.hover(VolumePart::Pill, false, now), Some(LEAVE_PILL));
        assert!(pill.open(now + Duration::from_millis(199)));
        pill.hover(VolumePart::Toggle, true, now + Duration::from_millis(100));
        assert!(pill.open(now + LEAVE_PILL), "coming back keeps it");
    }

    #[test]
    fn keyboard_focus_on_the_toggle_holds_it_open() {
        let now = Instant::now();
        let mut pill = VolumePill::default();
        pill.focus(true);
        assert!(pill.open(now));
        pill.focus(false);
        assert!(!pill.open(now));
    }
}
