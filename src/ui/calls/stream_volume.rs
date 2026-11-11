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

/// What holds the pill open: independent inputs, each set by its own
/// event.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Holds {
    toggle: bool,
    pill: bool,
    dragging: bool,
    keyboard: bool,
}

impl Holds {
    fn any(self) -> bool {
        self.toggle || self.pill || self.dragging || self.keyboard
    }
}

/// The live story header's volume pill: open while the pointer is on the
/// sound toggle or the pill, while the slider is dragged (tdesktop's
/// slider grabs the mouse, so a drag may leave the pill), and while the
/// toggle has keyboard focus. Leaving closes it after tdesktop's delays,
/// so a diagonal move from the toggle into the pill keeps it open. The
/// close timer is owned by the stage that waits for it: coming back (or
/// `release`) drops it.
#[derive(Default)]
pub(crate) enum VolumePill {
    #[default]
    Closed,
    Held(Holds),
    /// Nothing holds it: it closes at `at`, when the timer redraws.
    Closing {
        at: Instant,
        _timer: Task<()>,
    },
}

impl VolumePill {
    pub(crate) fn open(&self, now: Instant) -> bool {
        match self {
            Self::Held(_) => true,
            Self::Closing { at, .. } => now < *at,
            Self::Closed => false,
        }
    }

    fn holds(&self) -> Holds {
        match self {
            Self::Held(holds) => *holds,
            Self::Closing { .. } | Self::Closed => Holds::default(),
        }
    }

    /// An input changed. Any hold keeps it open (dropping a close timer);
    /// with none, an open pill closes at once and a closing one goes on.
    fn set_hold(&mut self, change: impl FnOnce(&mut Holds)) {
        let mut holds = self.holds();
        change(&mut holds);
        if holds.any() {
            *self = Self::Held(holds);
        } else if matches!(self, Self::Held(_)) {
            *self = Self::Closed;
        }
    }

    /// The pointer entered or left `part`. A leave that leaves nothing
    /// holding it starts the close: `start_timer` gives the task that
    /// redraws after the returned delay, which this stage then owns.
    pub(crate) fn hover(
        &mut self,
        part: VolumePart,
        inside: bool,
        now: Instant,
        start_timer: impl FnOnce(Duration) -> Task<()>,
    ) -> Option<Duration> {
        let mut holds = self.holds();
        match part {
            VolumePart::Toggle => holds.toggle = inside,
            VolumePart::Pill => holds.pill = inside,
        }
        if inside || holds.any() {
            *self = Self::Held(holds);
            return None;
        }
        let delay = match part {
            VolumePart::Toggle => LEAVE_TOGGLE,
            VolumePart::Pill => LEAVE_PILL,
        };
        *self = Self::Closing {
            at: now + delay,
            _timer: start_timer(delay),
        };
        Some(delay)
    }

    pub(crate) fn focus(&mut self, keyboard: bool) {
        self.set_hold(|holds| holds.keyboard = keyboard);
    }

    fn drag(&mut self, dragging: bool) {
        self.set_hold(|holds| holds.dragging = dragging);
    }

    /// The slider or the viewer it belonged to is gone: no leave, release
    /// or blur will come for what was held.
    pub(crate) fn release(&mut self) {
        *self = Self::Closed;
    }
}

/// Up and Down while a live story plays through Quill: its volume, 5 %
/// at a time, as in the stream window. Intercepted, because Up and Down
/// are already bound (message selection) and bindings match before
/// `on_key_down`. Only the window that holds the story viewer.
pub(crate) fn intercept_live_story_volume_keys(cx: &mut Context<QuillApp>) {
    let app = cx.weak_entity();
    crate::ui::passcode::intercept_keys_unlocked(cx, move |event, window, cx| {
        let keystroke = &event.keystroke;
        // Read at each key: a reset gives the main window a new id.
        let main = crate::ui::link_inbox::main_window_id(cx);
        if main != Some(window.window_handle().window_id()) || keystroke.modifiers.modified() {
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
        (self.players.volume * 100.0).round()
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
            if host == VolumeHost::LiveStory {
                self.stories.live.volume_pill.release();
            }
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
        // The slider belongs to the window that built it: its events act
        // on that window's viewer player.
        let scope = self.windows.scope;
        cx.subscribe(&slider, move |this, _, event: &SliderEvent, cx| {
            let (value, dragging) = match event {
                SliderEvent::Change(value) => (value.end(), true),
                SliderEvent::Release(value) => (value.end(), false),
            };
            this.in_window_scope(scope, cx, |this, cx| {
                if host == VolumeHost::LiveStory {
                    this.stories.live.volume_pill.drag(dragging);
                }
                this.set_playback_volume(value / 100.0, cx);
            });
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
        let next = ((self.players.volume + step) / KEY_STEP).round() * KEY_STEP;
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
            || self.live_story_shown().is_none()
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
    use gpui_kit::Task;
    use std::time::{Duration, Instant};

    fn timer(_: Duration) -> Task<()> {
        Task::ready(())
    }

    #[test]
    fn the_pill_stays_open_across_toggle_pill_and_drag() {
        let now = Instant::now();
        let mut pill = VolumePill::default();
        pill.hover(VolumePart::Toggle, true, now, timer);
        assert!(pill.open(now), "opens on the toggle");
        pill.hover(VolumePart::Pill, true, now, timer);
        pill.hover(VolumePart::Toggle, false, now, timer);
        assert!(pill.open(now), "into the pill");
        pill.drag(true);
        assert_eq!(pill.hover(VolumePart::Pill, false, now, timer), None);
        assert!(pill.open(now), "a drag holds it");
        pill.drag(false);
        assert!(!pill.open(now));
    }

    #[test]
    fn leaving_closes_after_tdesktops_delays() {
        let now = Instant::now();
        let mut pill = VolumePill::default();
        pill.hover(VolumePart::Toggle, true, now, timer);
        assert_eq!(
            pill.hover(VolumePart::Toggle, false, now, timer),
            Some(LEAVE_TOGGLE)
        );
        assert!(pill.open(now), "a diagonal move does not flicker it");
        assert!(!pill.open(now + LEAVE_TOGGLE));
        pill.hover(VolumePart::Pill, true, now, timer);
        assert_eq!(
            pill.hover(VolumePart::Pill, false, now, timer),
            Some(LEAVE_PILL)
        );
        assert!(pill.open(now + Duration::from_millis(199)));
        pill.hover(
            VolumePart::Toggle,
            true,
            now + Duration::from_millis(100),
            timer,
        );
        assert!(pill.open(now + LEAVE_PILL), "coming back keeps it");
    }

    #[test]
    fn a_released_pill_forgets_what_it_held() {
        let now = Instant::now();
        let mut pill = VolumePill::default();
        pill.hover(VolumePart::Pill, true, now, timer);
        pill.drag(true);
        pill.focus(true);
        pill.release();
        assert!(!pill.open(now));
    }

    #[test]
    fn the_close_timer_belongs_to_the_closing_stage() {
        let now = Instant::now();
        let mut pill = VolumePill::default();
        pill.hover(VolumePart::Toggle, true, now, timer);
        pill.hover(VolumePart::Toggle, false, now, timer);
        assert!(matches!(pill, VolumePill::Closing { .. }));
        // Coming back ends the closing and its timer.
        pill.hover(VolumePart::Pill, true, now, timer);
        assert!(matches!(pill, VolumePill::Held(_)));
        pill.hover(VolumePart::Pill, false, now, timer);
        assert!(matches!(pill, VolumePill::Closing { .. }));
        // A blur while it closes leaves it closing; a release ends it.
        pill.focus(false);
        assert!(matches!(pill, VolumePill::Closing { .. }));
        pill.release();
        assert!(matches!(pill, VolumePill::Closed));
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
