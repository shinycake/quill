//! The chrome helpers' pure parts.

#[test]
fn luminance_spans_black_to_white() {
    use gpui_kit::{black, white};
    assert!(super::luminance(black()) < 1e-4);
    assert!((super::luminance(white()) - 1.).abs() < 1e-4);
}
