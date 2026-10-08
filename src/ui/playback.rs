use super::app::QuillApp;
use super::*;
use gpui_kit::component::slider::SliderState;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
/// Which kind of track the shared in-process player is playing (Phase 4.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlaybackKind {
    Voice,
    Audio,
}
/// Seek-bar view model for one audio/voice history row (Phase 4.6).
#[derive(Clone)]
pub(crate) struct SeekBarView {
    /// Interactive slider entity — `Some` only on the active (playing or
    /// paused) row. Inactive rows render a static bar instead.
    pub(crate) slider: Option<Entity<SliderState>>,
    /// Seconds shown in the time label and as bar fill: live elapsed, scrub
    /// preview, paused offset, or the remembered position for inactive rows.
    pub(crate) display_secs: f64,
    /// Total track length in seconds (TDLib `duration`).
    pub(crate) duration_secs: f64,
    /// True while the active row's player is actually running (vs paused).
    pub(crate) is_playing: bool,
    /// MED1: current playback speed (for the speed button on active rows).
    pub(crate) speed: f64,
    /// MED1: true when the shared playback volume is muted.
    pub(crate) muted: bool,
    /// MED1: honest playback error for the active row, if any.
    pub(crate) error: Option<String>,
}
impl SeekBarView {
    pub(crate) fn fraction(&self) -> f64 {
        if self.duration_secs <= 0.0 {
            return 0.0;
        }
        (self.display_secs / self.duration_secs).clamp(0.0, 1.0)
    }
}
/// MED1: speed + mute buttons and the honest playback error line for an
/// active voice/audio row. `kind` disambiguates the button ids.
pub(crate) fn row_playback_controls(
    row_key: u64,
    kind: &str,
    seek: &SeekBarView,
    accent: Hsla,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap_3()
        .child(
            super::message_media::inline_link(
                SharedString::from(format!("{kind}-speed-{row_key}")),
                QuillApp::speed_label(seek.speed),
                accent,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.cycle_playback_speed(cx);
            })),
        )
        .child(
            super::message_media::inline_link(
                SharedString::from(format!("{kind}-mute-{row_key}")),
                if seek.muted { "Unmute" } else { "Mute" },
                accent,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.toggle_playback_mute(cx);
            })),
        )
        .when_some(seek.error.clone(), |this, err| {
            this.child(div().text_xs().text_color(danger_bright()).child(err))
        })
        .into_any_element()
}
