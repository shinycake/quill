//! Classic's motion is today's: the layout's numbers equal the constants the
//! slides were written with.

use crate::ui::sidebar_reveal::SLIDE_DURATION;
use quill::layout_style::CLASSIC;

#[test]
fn classic_slides_keep_todays_timing() {
    let ms = u16::try_from(SLIDE_DURATION.as_millis()).unwrap_or(u16::MAX);
    assert_eq!(CLASSIC.motion.sidebar_ms, ms);
    assert_eq!(CLASSIC.motion.panel_in_ms, ms);
    assert_eq!(CLASSIC.motion.panel_out_ms, ms);
}

#[test]
fn luminance_spans_black_to_white() {
    use gpui_kit::{black, white};
    assert!(super::luminance(black()) < 1e-4);
    assert!((super::luminance(white()) - 1.).abs() < 1e-4);
}
