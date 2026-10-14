//! The TON and Star glyphs of the revenue screens (tdesktop
//! `Ui::Earn::IconCurrencyColored` and the credits star), drawn as paths
//! so they need no image asset and follow the text color.

use gpui_kit::*;

/// tdesktop's Stars gold (`creditsBg1`).
pub(in crate::ui) const STAR_GOLD: u32 = 0xF7A21F;

/// The TON mark: a rounded downward triangle split by its axis.
fn paint_ton(bounds: Bounds<Pixels>, color: Hsla, window: &mut Window) {
    let side = f32::from(bounds.size.width.min(bounds.size.height));
    let at = |x: f32, y: f32| {
        point(
            bounds.origin.x + px(x * side),
            bounds.origin.y + px(y * side),
        )
    };
    let width = (side * 0.11).max(1.0);
    let mut outline = PathBuilder::stroke(px(width));
    outline.add_polygon(
        &[at(0.14, 0.2), at(0.86, 0.2), at(0.5, 0.88), at(0.14, 0.2)],
        true,
    );
    if let Ok(path) = outline.build() {
        window.paint_path(path, color);
    }
    let mut axis = PathBuilder::stroke(px(width));
    axis.add_polygon(&[at(0.5, 0.2), at(0.5, 0.88)], false);
    if let Ok(path) = axis.build() {
        window.paint_path(path, color);
    }
}

/// A filled five-point star.
fn paint_star(bounds: Bounds<Pixels>, color: Hsla, window: &mut Window) {
    let side = f32::from(bounds.size.width.min(bounds.size.height));
    let center = (side / 2.0, side * 0.53);
    let (outer, inner) = (side * 0.48, side * 0.21);
    let points: Vec<Point<Pixels>> = (0..10)
        .map(|i| {
            let radius = if i % 2 == 0 { outer } else { inner };
            let angle = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
            point(
                bounds.origin.x + px(center.0 + radius * angle.cos()),
                bounds.origin.y + px(center.1 + radius * angle.sin()),
            )
        })
        .collect();
    let mut path = PathBuilder::fill();
    path.add_polygon(&points, true);
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}

/// An inline TON glyph `size` pixels tall in `color`.
pub(in crate::ui) fn ton_icon(size: f32, color: Hsla) -> AnyElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| paint_ton(bounds, color, window),
    )
    .size(px(size))
    .flex_none()
    .into_any_element()
}

/// An inline gold Star `size` pixels tall.
pub(in crate::ui) fn star_icon(size: f32) -> AnyElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| paint_star(bounds, rgb(STAR_GOLD).into(), window),
    )
    .size(px(size))
    .flex_none()
    .into_any_element()
}
