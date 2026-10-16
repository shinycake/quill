//! The stream volume slider: beside mute in the stream window's bar, and
//! in the pill under the live story header's sound toggle (tdesktop
//! `VideoStream::setVolume`, which starts at the media viewer's
//! `videoVolume` and writes back to it).
//! It is 0 to 100 % and shows only while Quill plays the stream itself
//! (`calls::stream_audio`); on ntgcalls' device playback there is no gain
//! and mute stays the only control. Mute keeps its own meaning: the slider
//! value survives it.
//!
//! The one source of truth is the media viewer's volume
//! (`playback.volume`): the slider is rebuilt from it whenever they
//! differ, and every change goes through `set_playback_volume`, which
//! also hands it to the driver. Up and Down in the stream window move it
//! by 5 %.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::video_player::controls::VolumePart;
use gpui_kit::component::slider::{SliderEvent, SliderState};
use gpui_kit::*;
use quill::lang::{keys, tr};

/// Up / Down step.
const KEY_STEP: f32 = 0.05;

/// Where a stream volume slider sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VolumeHost {
    StreamWindow,
    LiveStory,
}

/// The live story header's volume pill: open while the pointer is on the
/// sound toggle or the pill, and while the slider is dragged (tdesktop's
/// slider grabs the mouse, so a drag may leave the pill).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct VolumePill {
    toggle: bool,
    pill: bool,
    dragging: bool,
}

impl VolumePill {
    pub(crate) fn open(self) -> bool {
        self.toggle || self.pill || self.dragging
    }

    /// Returns whether the pill opened or closed.
    pub(crate) fn hover(&mut self, part: VolumePart, inside: bool) -> bool {
        let was = self.open();
        match part {
            VolumePart::Toggle => self.toggle = inside,
            VolumePart::Pill => self.pill = inside,
        }
        was != self.open()
    }

    fn drag(&mut self, dragging: bool) {
        self.dragging = dragging;
    }
}

impl QuillApp {
    /// The volume as the slider shows it, 0 to 100.
    fn stream_volume_percent(&self) -> f32 {
        (self.playback.volume * 100.0).round()
    }

    /// Whether group call `call_id`'s stream has a volume: Quill plays its
    /// sound. The screenshot fixtures have no engine and always show it.
    pub(crate) fn stream_volume_shown(&self, call_id: Option<i32>) -> bool {
        match self.live.as_ref() {
            Some(live) => call_id.is_some_and(|id| live.driver.stream_has_volume(id)),
            None => true,
        }
    }

    fn volume_slot(&mut self, host: VolumeHost) -> &mut Option<Entity<SliderState>> {
        match host {
            VolumeHost::StreamWindow => &mut self.group_call.stream.volume,
            VolumeHost::LiveStory => &mut self.stories.live.volume,
        }
    }

    /// The host's slider for group call `call_id`, at the current volume,
    /// or `None` when the stream has no volume. Also brings the driver's
    /// copy of the volume in line (a new account's driver starts at 100 %).
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
        let volume = self.playback.volume;
        if let Some(live) = self.live.as_mut() {
            live.driver.set_stream_volume(volume);
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
        // Every move applies at once: a stream's gain changes without a
        // restart (tdesktop sets it on each change).
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

    /// Up / Down: the volume 5 % up or down, while the stream has one.
    pub(crate) fn step_stream_volume(
        &mut self,
        up: bool,
        call_id: Option<i32>,
        cx: &mut Context<Self>,
    ) {
        if !self.stream_volume_shown(call_id) {
            return;
        }
        let step = if up { KEY_STEP } else { -KEY_STEP };
        let next = ((self.playback.volume + step) / KEY_STEP).round() * KEY_STEP;
        self.set_playback_volume(next.clamp(0.0, 1.0), cx);
    }
}

#[cfg(test)]
mod tests {
    use super::VolumePill;
    use crate::ui::video_player::controls::VolumePart;

    #[test]
    fn the_pill_stays_open_across_toggle_pill_and_drag() {
        let mut pill = VolumePill::default();
        assert!(pill.hover(VolumePart::Toggle, true), "opens on the toggle");
        assert!(!pill.hover(VolumePart::Pill, true));
        assert!(!pill.hover(VolumePart::Toggle, false), "into the pill");
        pill.drag(true);
        assert!(!pill.hover(VolumePart::Pill, false), "a drag holds it");
        assert!(pill.open());
        pill.drag(false);
        assert!(!pill.open());
    }
}
