//! What the 1:1 call window says about the peer's side of the call (kept
//! out of `call_panel.rs` for its size): the microphone-off line and
//! tdesktop's low-battery notice (`Calls::Panel::createRemoteLowBattery`).
#![deny(clippy::indexing_slicing)]

use gpui_kit::*;
use quill::lang::{keys, tr_with};

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
    use super::{battery_low_line, battery_notice_shown, microphone_off_line};

    #[test]
    fn peer_microphone_off_line_names_the_peer() {
        assert_eq!(microphone_off_line("Zed"), "Zed's microphone is off");
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
