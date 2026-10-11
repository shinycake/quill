//! The call window backdrop (moved out of `call_panel.rs` to keep it under
//! the file size limit).

use gpui_kit::*;
use std::sync::Arc;

/// A radial backdrop from the peer's profile colors (tdesktop
/// `PanelBackground`): the first color around the photo, the last at the
/// edges. Cached per palette.
pub(super) fn backdrop(colors: &[u32]) -> Option<Arc<RenderImage>> {
    use super::lru::Lru;
    use std::cell::RefCell;
    thread_local! {
        static CACHE: RefCell<Lru<Vec<u32>, Arc<RenderImage>>> = RefCell::new(Lru::new(8));
    }
    if colors.len() < 2 {
        return None;
    }
    let key = colors.to_vec();
    if let Some(hit) = CACHE.with(|cache| cache.borrow_mut().get(&key)) {
        return Some(hit);
    }
    let (w, h) = (360u32, 270u32);
    // The photo's center: tdesktop puts it a third of the way down.
    let (cx, cy) = (w as f32 / 2., h as f32 * 0.36);
    let radius = (cx.hypot(cy)).max((w as f32 - cx).hypot(h as f32 - cy));
    let channel = |color: u32, shift: u32| ((color >> shift) & 0xff) as f32;
    let mut image = image::RgbaImage::new(w, h);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let t = ((x as f32 - cx).hypot(y as f32 - cy) / radius).clamp(0., 1.);
        let scaled = t * (colors.len() - 1) as f32;
        let index = (scaled.floor() as usize).min(colors.len() - 2);
        let local = scaled - index as f32;
        let (a, b) = (colors[index], colors[index + 1]);
        let mix = |shift| channel(a, shift) + (channel(b, shift) - channel(a, shift)) * local;
        // BGRA for GPUI.
        *pixel = image::Rgba([mix(0) as u8, mix(8) as u8, mix(16) as u8, 255]);
    }
    let render = Arc::new(RenderImage::new(smallvec::SmallVec::from_buf([
        image::Frame::new(image),
    ])));
    CACHE.with(|cache| {
        // One per peer palette; a few calls' worth is plenty.
        if let Some(old) = cache.borrow_mut().insert(key, render.clone()) {
            super::image_budget::retire_all([old]);
        }
    });
    Some(render)
}

/// A quarter-ish arc turning around the photo, `turn` of the way round.
pub(super) fn connecting_arc(turn: f32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let center = bounds.center();
            let radius = bounds.size.width / px(1.) / 2. - 2.;
            let start = turn * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
            let sweep = std::f32::consts::FRAC_PI_2 * 1.2;
            let points: Vec<_> = (0..=40)
                .map(|step| {
                    let angle = start + sweep * step as f32 / 40.;
                    point(
                        center.x + px(radius * angle.cos()),
                        center.y + px(radius * angle.sin()),
                    )
                })
                .collect();
            let mut path = PathBuilder::stroke(px(4.));
            path.add_polygon(&points, false);
            if let Ok(path) = path.build() {
                window.paint_path(path, hsla(0., 0., 1., 0.85));
            }
        },
    )
    .size_full()
}
