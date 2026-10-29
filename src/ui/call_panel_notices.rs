//! What the 1:1 call window says about the peer's side of the call (kept
//! out of `call_panel.rs` for its size): the microphone-off line,
//! tdesktop's low-battery notice (`Calls::Panel::createRemoteLowBattery`)
//! and its signal bars (`Calls::SignalBars`).
#![deny(clippy::indexing_slicing)]

use gpui_kit::*;
use quill::calls::engine::TransportState;
use quill::lang::{keys, tr_with};
use quill::state::ActiveCall;
use quill::telegram::envelope::CallState;

/// tdesktop `Call::kSignalBarCount`.
const SIGNAL_BAR_COUNT: u8 = 4;

/// The two looks of tdesktop's signal bars.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum BarsStyle {
    /// `callPanelSignalBars`, beside the emoji in the call window.
    Panel,
    /// `callBarSignalBars`, in the main window's call bar.
    Bar,
}

/// One bar set's geometry: bar width, gap, shortest and tallest bar.
struct BarsMetrics {
    width: f32,
    skip: f32,
    min: f32,
    max: f32,
}

impl BarsStyle {
    fn metrics(self) -> BarsMetrics {
        match self {
            BarsStyle::Panel => BarsMetrics {
                width: 2.,
                skip: 2.,
                min: 4.,
                max: 10.,
            },
            BarsStyle::Bar => BarsMetrics {
                width: 3.,
                skip: 1.,
                min: 3.,
                max: 12.,
            },
        }
    }
}

/// The signal bars to show for `call`: tgcalls' count once the call is
/// established (connected, or reconnecting after it was), nothing before.
pub(super) fn shown_signal_bars(call: &ActiveCall) -> Option<u8> {
    let established = matches!(call.state, CallState::Ready)
        && matches!(
            call.transport,
            Some(TransportState::Connected | TransportState::Reconnecting)
        );
    call.signal_bars.filter(|_| established)
}

/// Bar `index`'s height (tdesktop: from `min` for the first to `max` for
/// the last, evenly).
fn bar_height(style: BarsStyle, index: u8) -> f32 {
    let m = style.metrics();
    m.min + (m.max - m.min) * f32::from(index) / f32::from(SIGNAL_BAR_COUNT - 1)
}

/// tdesktop `SignalBars::paintEvent`: four rounded bars growing left to
/// right, bottom-aligned, in white; bars past `count` at half opacity.
pub(super) fn signal_bars(count: u8, style: BarsStyle) -> impl IntoElement {
    let m = style.metrics();
    let left = match style {
        // `callFingerprintSignalBarsSkip` and `callSignalBarsPadding`'s
        // left between the emoji and the bars.
        BarsStyle::Panel => 2. + 8.,
        // Right after the mute button.
        BarsStyle::Bar => 0.,
    };
    div()
        .ml(px(left))
        .h(px(m.max))
        .flex()
        .items_end()
        .gap(px(m.skip))
        .children((0..SIGNAL_BAR_COUNT).map(move |index| {
            div()
                .w(px(m.width))
                .h(px(bar_height(style, index)))
                .rounded(px(m.width / 2.))
                .bg(white())
                .opacity(if index < count { 1. } else { 0.5 })
        }))
}

/// tdesktop `lng_call_microphone_off`.
pub(super) fn microphone_off_line(name: &str) -> String {
    tr_with(keys::CALL_MICROPHONE_OFF, &[("user", name)])
}

/// tdesktop `lng_call_battery_level_low`.
pub(super) fn battery_low_line(name: &str) -> String {
    tr_with(keys::CALL_BATTERY_LEVEL_LOW, &[("user", name)])
}

/// tdesktop shows the low-battery notice only while the peer's
/// microphone-off notice is not on screen (`showRemoteLowBattery`).
pub(super) fn battery_notice_shown(battery_low: bool, microphone_off_shown: bool) -> bool {
    battery_low && !microphone_off_shown
}

/// tdesktop `videoPlayIconBg` (#0000007f) under the notice.
fn notice_bg() -> Hsla {
    hsla(0., 0., 0., 0x7f as f32 / 255.)
}

/// tdesktop's notice: a 12 px white line in a dark pill
/// (`callRemoteAudioMute` on `videoPlayIconBg`, padding
/// `callTooltipPadding` 41/7/15/8) with the battery drawn at
/// `callTooltipMutedIconPosition`'s x (10 px), centered over the
/// buttons with `callRemoteAudioMuteSkip` (12 px) between.
pub(super) fn battery_low_notice(name: &str, buttons_top: f32) -> impl IntoElement {
    div()
        .id("call-remote-battery-low")
        .absolute()
        .left_0()
        .right_0()
        .bottom(px(buttons_top + 12.))
        .flex()
        .justify_center()
        .child(
            div()
                .relative()
                .pl(px(41.))
                .pt(px(7.))
                .pr(px(15.))
                .pb(px(8.))
                .rounded_full()
                .bg(notice_bg())
                .text_size(px(12.))
                .text_color(white())
                .child(
                    div()
                        .absolute()
                        .left(px(10.))
                        .top_0()
                        .bottom_0()
                        .flex()
                        .items_center()
                        .child(battery_icon()),
                )
                .child(battery_low_line(name)),
        )
}

/// tdesktop's `BatterySvg` (29 × 13): a 24 × 12 outline with 4 px
/// corners, the terminal on its right and one thin bar of charge.
fn battery_icon() -> impl IntoElement {
    div()
        .relative()
        .w(px(29.))
        .h(px(13.))
        .child(
            div()
                .absolute()
                .left(px(1.))
                .top_0()
                .w(px(25.))
                .h(px(13.))
                .rounded(px(4.))
                .border_1()
                .border_color(white()),
        )
        .child(
            div()
                .absolute()
                .left(px(27.))
                .top(px(4.5))
                .w(px(1.5))
                .h(px(4.))
                .rounded_r(px(1.))
                .bg(white()),
        )
        .child(
            div()
                .absolute()
                .left(px(5.))
                .top(px(3.5))
                .w(px(1.))
                .h(px(6.))
                .rounded(px(0.5))
                .bg(white()),
        )
}

#[cfg(test)]
mod tests {
    use super::{
        BarsStyle, bar_height, battery_low_line, battery_notice_shown, microphone_off_line,
        shown_signal_bars,
    };
    use quill::calls::engine::{RemoteVideoState, TransportState};
    use quill::state::ActiveCall;
    use quill::telegram::envelope::CallState;

    fn call(state: CallState, transport: Option<TransportState>, bars: Option<u8>) -> ActiveCall {
        ActiveCall {
            id: 7,
            user_id: 11,
            is_outgoing: true,
            is_video: false,
            state,
            started_at: std::time::Instant::now(),
            ready_at: None,
            ready: None,
            transport,
            transport_error: None,
            signaling_queue: Vec::new(),
            signaling_dropped: 0,
            muted: false,
            camera_on: false,
            screen_sharing: false,
            remote_video: RemoteVideoState::Inactive,
            remote_screen: RemoteVideoState::Inactive,
            remote_audio_muted: false,
            remote_battery_low: false,
            signal_bars: bars,
        }
    }

    #[test]
    fn bars_show_only_once_the_call_is_established() {
        use TransportState::*;
        let ready = |transport, bars| shown_signal_bars(&call(CallState::Ready, transport, bars));
        assert_eq!(ready(Some(Connected), Some(3)), Some(3));
        assert_eq!(ready(Some(Reconnecting), Some(0)), Some(0));
        assert_eq!(ready(Some(Connecting), Some(2)), None);
        assert_eq!(ready(Some(Failed), Some(2)), None);
        assert_eq!(ready(Some(Connected), None), None, "nothing reported yet");
        let hanging = call(CallState::HangingUp, Some(Connected), Some(4));
        assert_eq!(shown_signal_bars(&hanging), None);
    }

    #[test]
    fn peer_microphone_off_line_names_the_peer() {
        assert_eq!(microphone_off_line("Zed"), "Zed's microphone is off");
    }

    #[test]
    fn bars_grow_like_tdesktops() {
        let panel: Vec<f32> = (0..4).map(|i| bar_height(BarsStyle::Panel, i)).collect();
        assert_eq!(panel, vec![4., 6., 8., 10.]);
        let bar: Vec<f32> = (0..4).map(|i| bar_height(BarsStyle::Bar, i)).collect();
        assert_eq!(bar, vec![3., 6., 9., 12.]);
    }

    #[test]
    fn battery_line_names_the_peer() {
        assert_eq!(battery_low_line("Zed"), "Zed's battery level is low");
    }

    #[test]
    fn battery_notice_gives_way_to_the_microphone_notice() {
        assert!(battery_notice_shown(true, false));
        assert!(!battery_notice_shown(true, true));
        assert!(!battery_notice_shown(false, false));
    }
}
