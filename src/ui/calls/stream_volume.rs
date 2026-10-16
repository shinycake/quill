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

pub(crate) const KEY_STEP: f32 = 0.05;

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

    pub(crate) fn hover(&mut self, part: VolumePart, inside: bool) {
        match part {
            VolumePart::Toggle => self.toggle = inside,
            VolumePart::Pill => self.pill = inside,
        }
    }

    fn drag(&mut self, dragging: bool) {
        self.dragging = dragging;
    }
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

    pub(crate) fn step_stream_volume(
        &mut self,
        step: f32,
        call_id: Option<i32>,
        cx: &mut Context<Self>,
    ) {
        if !self.stream_volume_shown(call_id) {
            return;
        }
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
        pill.hover(VolumePart::Toggle, true);
        assert!(pill.open(), "opens on the toggle");
        pill.hover(VolumePart::Pill, true);
        pill.hover(VolumePart::Toggle, false);
        assert!(pill.open(), "into the pill");
        pill.drag(true);
        pill.hover(VolumePart::Pill, false);
        assert!(pill.open(), "a drag holds it");
        pill.drag(false);
        assert!(!pill.open());
    }
}
