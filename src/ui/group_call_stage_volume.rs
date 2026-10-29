//! The full-screen stage's volume for the pinned participant: a kit
//! slider from 0 to 200 % (tdesktop's participant menu volume,
//! `Calls::Group::MenuVolumeItem`), sent on release as
//! `setGroupCallParticipantVolumeLevel`.
//!
//! TDLib applies the level for everyone only when you manage the call;
//! otherwise the server keeps it for you. Quill's own playback does not
//! follow it: the call library mixes the call's audio itself and has no gain per
//! source (see `docs/decisions/video-player-live-ui.md`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::assets::IconName;
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::*;
use quill::lang::{keys, tr_with};
use quill::state::ActiveGroupCall;
use quill::telegram::envelope::MessageSender;

/// TDLib's volume levels: 1 to 20000, 10000 being 100 %.
const LEVEL_MIN: i32 = 1;
const LEVEL_MAX: i32 = 20000;
const LEVEL_PER_PERCENT: i32 = 100;
/// The slider's width on the stage.
const SLIDER_WIDTH: f32 = 120.;

/// The slider of the participant on the stage.
pub(super) struct StageVolume {
    sender: MessageSender,
    /// The level the slider was made from; a new level from the server
    /// remakes it, unless a drag is under way.
    level: i32,
    /// A drag is under way (a change seen, no release yet).
    dragging: bool,
    slider: Entity<SliderState>,
    _events: Subscription,
}

/// The slider's percent for a TDLib level.
fn percent_for_level(level: i32) -> f32 {
    (level.clamp(LEVEL_MIN, LEVEL_MAX) / LEVEL_PER_PERCENT) as f32
}

/// The TDLib level for the slider's percent.
fn level_for_percent(percent: f32) -> i32 {
    let percent = percent
        .round()
        .clamp(0., (LEVEL_MAX / LEVEL_PER_PERCENT) as f32) as i32;
    percent
        .saturating_mul(LEVEL_PER_PERCENT)
        .clamp(LEVEL_MIN, LEVEL_MAX)
}

impl QuillApp {
    /// The stage's volume slider for `sender`, `None` when the
    /// participant has left.
    pub(super) fn stage_volume_slider(
        &mut self,
        call: &ActiveGroupCall,
        sender: MessageSender,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let level = call
            .participants
            .iter()
            .find(|participant| participant.participant_id == sender)?
            .volume_level;
        let current = self
            .group_call
            .stage_volume
            .as_ref()
            .filter(|volume| volume.sender == sender && (volume.level == level || volume.dragging))
            .map(|volume| volume.slider.clone());
        let slider = match current {
            Some(slider) => slider,
            None => {
                let name = self.group_call_participant_name(&sender);
                let slider = cx.new(|_| {
                    SliderState::new()
                        .min(0.)
                        .max(percent_for_level(LEVEL_MAX))
                        .step(1.)
                        .default_value(percent_for_level(level))
                        .label(tr_with(
                            keys::Q_AV_PARTICIPANT_VOLUME,
                            &[("name", &name.to_string())],
                        ))
                });
                let events = cx.subscribe(&slider, move |this, _, event: &SliderEvent, cx| {
                    let release = matches!(event, SliderEvent::Release(_));
                    if let Some(volume) = this.group_call.stage_volume.as_mut() {
                        volume.dragging = !release;
                    }
                    if let SliderEvent::Release(value) = event {
                        this.set_group_call_participant_volume(
                            sender,
                            level_for_percent(value.end()),
                            cx,
                        );
                    }
                });
                self.group_call.stage_volume = Some(StageVolume {
                    sender,
                    level,
                    dragging: false,
                    slider: slider.clone(),
                    _events: events,
                });
                slider
            }
        };
        Some(
            div()
                .id("group-stage-volume")
                .debug_selector(|| "group-stage-volume".into())
                .flex()
                .items_center()
                .gap(px(6.))
                .h(px(34.))
                .px(px(10.))
                .rounded_full()
                .bg(rgba(0x0000_0099))
                .text_color(white())
                .child(Icon::new(IconName::Volume2).with_size(px(16.)))
                .child(div().w(px(SLIDER_WIDTH)).child(Slider::new(&slider)))
                .into_any_element(),
        )
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod tests {
    use super::{LEVEL_MAX, LEVEL_MIN, level_for_percent, percent_for_level};

    #[test]
    fn levels_map_to_percents_and_back() {
        assert_eq!(percent_for_level(10000), 100.);
        assert_eq!(percent_for_level(20000), 200.);
        assert_eq!(percent_for_level(0), 0.);
        assert_eq!(level_for_percent(100.), 10000);
        assert_eq!(level_for_percent(0.), LEVEL_MIN);
        assert_eq!(level_for_percent(250.), LEVEL_MAX);
        assert_eq!(level_for_percent(f32::NAN), LEVEL_MIN);
    }
}
