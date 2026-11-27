//! The chrome helpers' pure parts.

#[test]
fn luminance_spans_black_to_white() {
    use gpui_kit::{black, white};
    assert!(super::luminance(black()) < 1e-4);
    assert!((super::luminance(white()) - 1.).abs() < 1e-4);
}

#[test]
fn a_tint_over_a_clear_row_shows() {
    use gpui_kit::{black, transparent_black, white};
    let tint = black().opacity(0.06);
    assert_eq!(super::over_fill(transparent_black(), tint), tint);
    let solid = super::over_fill(white(), tint);
    assert_eq!(solid.a, 1.);
    assert!(solid.l < 1.);
}
