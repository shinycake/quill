//! Zoom, pan, orientation and pixel transforms for the viewer visual
//! (pure, no GPUI).

/// Zoom/pan state for the viewer visual (pure, no GPUI). Zoom is a fit-scale
/// factor (`1.0` = contain); pan is the visual's top-left offset in px at the
/// current zoom, clamped so a magnified image always covers the frame and a
/// smaller one sits in its middle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewerZoom {
    pub zoom: f32,
    pub pan: (f32, f32),
    /// The `+` / `-` ladder of the picture in this frame; `None` while its
    /// natural size is unknown, and the steps then multiply by
    /// [`VIEWER_ZOOM_STEP`].
    pub levels: Option<ZoomLevels>,
}

/// Wheel-zoom step factor.
pub const VIEWER_ZOOM_STEP: f32 = 1.15;
/// Wheel and pinch zoom limits (fit-scale factors). The `+` / `-` ladder
/// may step outside them; a wheel then only moves back toward them.
pub const VIEWER_ZOOM_MIN: f32 = 1.0;
pub const VIEWER_ZOOM_MAX: f32 = 8.0;
/// tdesktop `kMaxZoomLevel`: the ladder runs from level -7 (an eighth of
/// the natural size) to level 7 (eight times it).
pub const VIEWER_MAX_ZOOM_LEVEL: i32 = 7;
/// Two scales closer than this fraction are one stop of the ladder.
const STOP_EPSILON: f32 = 1e-4;

/// Scale of ladder level `level` against the natural size: `level + 1`
/// from 0 up, `1 / (1 - level)` below (tdesktop `setZoomLevel`).
pub fn zoom_level_scale(level: i32) -> f32 {
    if level >= 0 {
        level as f32 + 1.0
    } else {
        1.0 / (1.0 - level as f32)
    }
}

fn same_stop(a: f32, b: f32) -> bool {
    (a - b).abs() <= b.abs() * STOP_EPSILON
}

/// The `+` / `-` zoom ladder of one picture in one frame (tdesktop
/// `zoomIn` / `zoomOut`): the integer levels, plus one snap on the way,
/// fit (the whole picture in the area, tdesktop `_zoomToDefault`). Scales
/// are against the natural size, one picture pixel per point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoomLevels {
    natural: (f32, f32),
    fit: f32,
}

impl ZoomLevels {
    /// The ladder of a `natural`-sized picture in an `area`; `None` unless
    /// both sizes are positive.
    pub fn new(natural: (f32, f32), area: (f32, f32)) -> Option<Self> {
        let (nw, nh) = natural;
        let (aw, ah) = area;
        if !(nw > 0.0 && nh > 0.0 && aw > 0.0 && ah > 0.0) {
            return None;
        }
        let fit = (aw / nw).min(ah / nh);
        (fit > 0.0 && fit.is_finite()).then_some(Self { natural, fit })
    }

    pub fn fit(&self) -> f32 {
        self.fit
    }

    /// Every stop, unordered: the integer levels that leave the picture at
    /// least a point on each side (tdesktop `zoomUpdate`), and fit.
    fn stops(&self) -> impl Iterator<Item = f32> + '_ {
        let side = self.natural.0.min(self.natural.1);
        (-VIEWER_MAX_ZOOM_LEVEL..=VIEWER_MAX_ZOOM_LEVEL)
            .map(zoom_level_scale)
            .filter(move |scale| side * scale >= 1.0)
            .chain([self.fit])
    }

    /// The nearest stop above (`zoom_in`) or below `scale`, or `scale`
    /// itself at the end of the ladder.
    pub fn step(&self, scale: f32, zoom_in: bool) -> f32 {
        let next = if zoom_in {
            self.stops()
                .filter(|stop| *stop > scale * (1.0 + STOP_EPSILON))
                .reduce(f32::min)
        } else {
            self.stops()
                .filter(|stop| *stop < scale * (1.0 - STOP_EPSILON))
                .reduce(f32::max)
        };
        next.unwrap_or(scale)
    }

    /// tdesktop `zoomReset`: natural size goes back to fit, any other
    /// scale to natural size.
    pub fn reset(&self, scale: f32) -> f32 {
        if same_stop(scale, 1.0) { self.fit } else { 1.0 }
    }
}
/// Scroll distance (px) that equals one `VIEWER_ZOOM_STEP`; a mouse-wheel
/// line counts as one notch.
pub const VIEWER_WHEEL_NOTCH_PX: f32 = 40.0;
/// Largest zoom change one wheel event may apply, in notches.
pub const VIEWER_WHEEL_MAX_NOTCHES: f32 = 3.0;

/// Zoom factor for one wheel event of `delta_y` pixels (positive zooms
/// in): proportional to the delta so a trackpad glides and a wheel notch
/// steps, clamped so a flick cannot jump the whole range.
pub fn wheel_zoom_factor(delta_y: f32) -> f32 {
    let notches = (delta_y / VIEWER_WHEEL_NOTCH_PX)
        .clamp(-VIEWER_WHEEL_MAX_NOTCHES, VIEWER_WHEEL_MAX_NOTCHES);
    VIEWER_ZOOM_STEP.powf(notches)
}

/// Zoom factor for one pinch event (`delta` is the scale change, `0.1`
/// is 10% larger), clamped like a wheel event.
pub fn pinch_zoom_factor(delta: f32) -> f32 {
    if !delta.is_finite() {
        return 1.0;
    }
    let cap = VIEWER_ZOOM_STEP.powf(VIEWER_WHEEL_MAX_NOTCHES);
    (1.0 + delta).clamp(1.0 / cap, cap)
}

/// Photo orientation in the viewer: quarter-turns clockwise plus mirror
/// flips (Telegram Desktop's `_flip`, media_view_overlay_widget.cpp:7431).
/// Pixels are flipped first, then rotated.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ViewerOrientation {
    pub turns: u8,
    pub flip_h: bool,
    pub flip_v: bool,
}

impl ViewerOrientation {
    pub fn is_identity(&self) -> bool {
        self.turns.is_multiple_of(4) && !self.flip_h && !self.flip_v
    }

    /// `true` when the displayed width/height are swapped.
    pub fn swaps_axes(&self) -> bool {
        self.turns % 2 == 1
    }

    /// Cache key distinguishing every orientation.
    pub fn code(&self) -> u8 {
        (self.turns % 4) | (u8::from(self.flip_h) << 2) | (u8::from(self.flip_v) << 3)
    }

    pub fn rotate_cw(&mut self) {
        self.turns = (self.turns + 1) % 4;
    }

    /// Mirror what the user sees left-to-right (`H`). After a quarter
    /// turn the stored flip axes are swapped, so the on-screen result
    /// stays what the key promises.
    pub fn flip_horizontal(&mut self) {
        if self.swaps_axes() {
            self.flip_v = !self.flip_v;
        } else {
            self.flip_h = !self.flip_h;
        }
    }

    /// Mirror what the user sees top-to-bottom (`V`).
    pub fn flip_vertical(&mut self) {
        if self.swaps_axes() {
            self.flip_h = !self.flip_h;
        } else {
            self.flip_v = !self.flip_v;
        }
    }
}

/// Mirror a packed RGBA image horizontally and/or vertically.
pub fn flip_rgba(pixels: &[u8], width: u32, height: u32, flip_h: bool, flip_v: bool) -> Vec<u8> {
    if (!flip_h && !flip_v) || pixels.len() != (width * height * 4) as usize {
        return pixels.to_vec();
    }
    let (w, h) = (width as usize, height as usize);
    let mut out = vec![0u8; pixels.len()];
    for y in 0..h {
        let dy = if flip_v { h - 1 - y } else { y };
        for x in 0..w {
            let dx = if flip_h { w - 1 - x } else { x };
            let src = (y * w + x) * 4;
            let dst = (dy * w + dx) * 4;
            if let (Some(from), Some(to)) = (pixels.get(src..src + 4), out.get_mut(dst..dst + 4)) {
                to.copy_from_slice(from);
            }
        }
    }
    out
}

/// Apply `orientation` (flip, then rotate) to a packed RGBA image.
pub fn orient_rgba(
    pixels: &[u8],
    width: u32,
    height: u32,
    orientation: ViewerOrientation,
) -> (Vec<u8>, u32, u32) {
    let flipped = flip_rgba(
        pixels,
        width,
        height,
        orientation.flip_h,
        orientation.flip_v,
    );
    rotate_rgba_quarter_turns(&flipped, width, height, orientation.turns)
}

impl Default for ViewerZoom {
    fn default() -> Self {
        Self::fitted(None)
    }
}

impl ViewerZoom {
    pub fn new() -> Self {
        Self::default()
    }

    /// At fit, stepping along `levels`.
    pub fn fitted(levels: Option<ZoomLevels>) -> Self {
        Self {
            zoom: VIEWER_ZOOM_MIN,
            pan: (0.0, 0.0),
            levels,
        }
    }

    /// Back to fit (the ladder stays).
    pub fn reset(&mut self) {
        *self = Self::fitted(self.levels);
    }

    /// The zoom as the label shows it: percent of the natural size, or of
    /// the fit size while the natural size is unknown.
    pub fn percent(&self) -> f32 {
        self.zoom * self.levels.map_or(1.0, |levels| levels.fit()) * 100.0
    }

    /// `true` when the visual is magnified (pan is meaningful).
    pub fn is_zoomed(&self) -> bool {
        self.zoom > VIEWER_ZOOM_MIN
    }

    /// Step the zoom in (`zoom_in = true`) or out to the next stop of the
    /// ladder, keeping the frame center fixed. `frame` is the
    /// `(width, height)` of the visual container.
    pub fn step(&mut self, zoom_in: bool, frame: (f32, f32)) {
        let target = match self.levels {
            Some(levels) => levels.step(self.zoom * levels.fit(), zoom_in) / levels.fit(),
            None => {
                let factor = if zoom_in {
                    VIEWER_ZOOM_STEP
                } else {
                    1.0 / VIEWER_ZOOM_STEP
                };
                (self.zoom * factor).clamp(VIEWER_ZOOM_MIN, VIEWER_ZOOM_MAX)
            }
        };
        self.set_zoom(target, (frame.0 / 2.0, frame.1 / 2.0), frame);
    }

    /// Multiply the zoom by `factor`, keeping the point `anchor` (relative
    /// to the visual container's top-left, e.g. the pointer) fixed on
    /// screen. The result stays inside the wheel limits, or moves toward
    /// them from a ladder stop outside; the pan is clamped to the frame.
    pub fn zoom_at(&mut self, factor: f32, anchor: (f32, f32), frame: (f32, f32)) {
        if !factor.is_finite() || factor <= 0.0 {
            return;
        }
        let old = self.zoom;
        let new = (old * factor).clamp(VIEWER_ZOOM_MIN.min(old), VIEWER_ZOOM_MAX.max(old));
        self.set_zoom(new, anchor, frame);
    }

    fn set_zoom(&mut self, new: f32, anchor: (f32, f32), frame: (f32, f32)) {
        let old = self.zoom;
        if !new.is_finite() || new <= 0.0 || (new - old).abs() < f32::EPSILON {
            return;
        }
        let ratio = new / old;
        let (ax, ay) = (anchor.0.clamp(0.0, frame.0), anchor.1.clamp(0.0, frame.1));
        self.zoom = new;
        self.pan = (
            ax - (ax - self.pan.0) * ratio,
            ay - (ay - self.pan.1) * ratio,
        );
        self.clamp_pan(frame);
    }

    /// Drag-pan by a mouse delta in px. Ignored at fit zoom (nothing to pan).
    pub fn pan_by(&mut self, dx: f32, dy: f32, frame: (f32, f32)) {
        if !self.is_zoomed() {
            return;
        }
        self.pan = (self.pan.0 + dx, self.pan.1 + dy);
        self.clamp_pan(frame);
    }

    fn clamp_pan(&mut self, frame: (f32, f32)) {
        let zoom = self.zoom;
        let axis = |pan: f32, size: f32| {
            let zoomed = size * zoom;
            if zoomed > size {
                pan.clamp(size - zoomed, 0.0)
            } else {
                (size - zoomed) / 2.0
            }
        };
        self.pan = (axis(self.pan.0, frame.0), axis(self.pan.1, frame.1));
    }
}

/// Rotate a packed RGBA image by `turns` quarter-turns clockwise, in 90°
/// steps (the viewer Rotate button cycles 0 → 90 → 180 → 270). Pure pixel
/// math — no `image` crate, so the no-default-features test build covers
/// it; the UI layer converts to/from `RgbaImage` at the boundary.
///
/// Returns `(pixels, width, height)`: odd quarter-turns swap the
/// dimensions.
/// (Secret/spoiler media are excluded from the viewer; see module docs.)
pub fn rotate_rgba_quarter_turns(
    pixels: &[u8],
    width: u32,
    height: u32,
    turns: u8,
) -> (Vec<u8>, u32, u32) {
    let turns = turns % 4;
    if turns == 0 || pixels.len() != (width * height * 4) as usize {
        return (pixels.to_vec(), width, height);
    }
    let (w, h) = (width as usize, height as usize);
    let mut out = vec![0u8; pixels.len()];
    // Clockwise: out(x, y) = in(w - 1 - y, x) for the 90° case, composed.
    let src_index = |x: usize, y: usize| (y * w + x) * 4;
    for y in 0..h {
        for x in 0..w {
            let (dx, dy, dw) = match turns {
                // 90° cw: (x, y) -> (h - 1 - y, x)
                1 => (h - 1 - y, x, h),
                // 180°: (x, y) -> (w - 1 - x, h - 1 - y)
                2 => (w - 1 - x, h - 1 - y, w),
                // 270° cw: (x, y) -> (y, w - 1 - x)
                _ => (y, w - 1 - x, h),
            };
            let src = src_index(x, y);
            let dst = (dy * dw + dx) * 4;
            if let (Some(from), Some(to)) = (pixels.get(src..src + 4), out.get_mut(dst..dst + 4)) {
                to.copy_from_slice(from);
            }
        }
    }
    let (ow, oh) = if turns % 2 == 1 { (h, w) } else { (w, h) };
    (out, ow as u32, oh as u32)
}

/// Largest size with `natural`'s aspect ratio that fits in `frame`.
pub fn fit_within(natural: (f32, f32), frame: (f32, f32)) -> (f32, f32) {
    let (nw, nh) = natural;
    let (fw, fh) = frame;
    if nw <= 0.0 || nh <= 0.0 {
        return frame;
    }
    let scale = (fw / nw).min(fh / nh);
    (nw * scale, nh * scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_defaults_to_fit_and_steps_around_center() {
        let mut zoom = ViewerZoom::new();
        assert!(!zoom.is_zoomed());
        zoom.step(true, (720.0, 480.0));
        assert!(zoom.is_zoomed());
        assert!((zoom.zoom - VIEWER_ZOOM_STEP).abs() < 1e-6);
        // Center stays fixed: pan moves the visual's top-left up-left.
        assert!((zoom.pan.0 - (360.0 - 360.0 * VIEWER_ZOOM_STEP)).abs() < 1e-3);
        assert!((zoom.pan.1 - (240.0 - 240.0 * VIEWER_ZOOM_STEP)).abs() < 1e-3);
    }

    #[test]
    fn zoom_clamps_to_min_and_max() {
        let mut zoom = ViewerZoom::new();
        zoom.step(false, (720.0, 480.0));
        assert_eq!(zoom.zoom, VIEWER_ZOOM_MIN);
        for _ in 0..40 {
            zoom.step(true, (720.0, 480.0));
        }
        assert_eq!(zoom.zoom, VIEWER_ZOOM_MAX);
        for _ in 0..40 {
            zoom.step(false, (720.0, 480.0));
        }
        assert_eq!(zoom.zoom, VIEWER_ZOOM_MIN);
        assert_eq!(zoom.pan, (0.0, 0.0));
    }

    #[test]
    fn zoom_at_keeps_the_pointer_point_fixed() {
        let frame = (720.0, 480.0);
        let mut zoom = ViewerZoom::new();
        let anchor = (600.0, 100.0);
        zoom.zoom_at(2.0, anchor, frame);
        assert!((zoom.zoom - 2.0).abs() < 1e-6);
        // The image point under the pointer before: (anchor - pan) / zoom.
        let before = (anchor.0, anchor.1);
        let after = (
            (anchor.0 - zoom.pan.0) / zoom.zoom,
            (anchor.1 - zoom.pan.1) / zoom.zoom,
        );
        assert!((after.0 - before.0).abs() < 1e-3);
        assert!((after.1 - before.1).abs() < 1e-3);
        // Zooming again keeps it fixed.
        zoom.zoom_at(1.5, anchor, frame);
        let again = (
            (anchor.0 - zoom.pan.0) / zoom.zoom,
            (anchor.1 - zoom.pan.1) / zoom.zoom,
        );
        assert!((again.0 - before.0).abs() < 1e-3);
        assert!((again.1 - before.1).abs() < 1e-3);
    }

    #[test]
    fn zoom_at_corner_clamps_pan_and_ignores_bad_factors() {
        let frame = (720.0, 480.0);
        let mut zoom = ViewerZoom::new();
        zoom.zoom_at(2.0, (0.0, 0.0), frame);
        assert_eq!(zoom.pan, (0.0, 0.0));
        zoom.zoom_at(f32::NAN, (10.0, 10.0), frame);
        zoom.zoom_at(0.0, (10.0, 10.0), frame);
        assert!((zoom.zoom - 2.0).abs() < 1e-6);
        zoom.zoom_at(0.01, (10.0, 10.0), frame);
        assert_eq!(zoom.zoom, VIEWER_ZOOM_MIN);
        assert_eq!(zoom.pan, (0.0, 0.0));
    }

    #[test]
    fn wheel_factor_scales_with_delta_and_clamps() {
        assert!((wheel_zoom_factor(0.0) - 1.0).abs() < 1e-6);
        assert!((wheel_zoom_factor(VIEWER_WHEEL_NOTCH_PX) - VIEWER_ZOOM_STEP).abs() < 1e-5);
        assert!(wheel_zoom_factor(-VIEWER_WHEEL_NOTCH_PX) < 1.0);
        assert!(wheel_zoom_factor(4.0) < wheel_zoom_factor(20.0));
        let cap = VIEWER_ZOOM_STEP.powf(VIEWER_WHEEL_MAX_NOTCHES);
        assert!((wheel_zoom_factor(100_000.0) - cap).abs() < 1e-4);
        assert!((wheel_zoom_factor(-100_000.0) - 1.0 / cap).abs() < 1e-4);
    }

    #[test]
    fn pinch_factor_follows_the_scale_change_and_clamps() {
        assert!((pinch_zoom_factor(0.1) - 1.1).abs() < 1e-6);
        assert!((pinch_zoom_factor(-0.1) - 0.9).abs() < 1e-6);
        let cap = VIEWER_ZOOM_STEP.powf(VIEWER_WHEEL_MAX_NOTCHES);
        assert!((pinch_zoom_factor(10.0) - cap).abs() < 1e-4);
        assert!((pinch_zoom_factor(-10.0) - 1.0 / cap).abs() < 1e-4);
        assert_eq!(pinch_zoom_factor(f32::NAN), 1.0);
    }

    #[test]
    fn orientation_flips_follow_the_screen_after_a_quarter_turn() {
        let mut o = ViewerOrientation::default();
        assert!(o.is_identity());
        o.flip_horizontal();
        assert!(o.flip_h && !o.flip_v && !o.is_identity());
        o.flip_horizontal();
        assert!(o.is_identity());
        o.rotate_cw();
        assert!(o.swaps_axes());
        // On screen the image is turned 90 degrees: H flips the stored
        // vertical axis.
        o.flip_horizontal();
        assert!(o.flip_v && !o.flip_h);
        o.flip_vertical();
        assert!(o.flip_h);
        let codes: std::collections::HashSet<u8> = (0..4u8)
            .flat_map(|t| {
                [(false, false), (true, false), (false, true), (true, true)].map(|(h, v)| {
                    ViewerOrientation {
                        turns: t,
                        flip_h: h,
                        flip_v: v,
                    }
                    .code()
                })
            })
            .collect();
        assert_eq!(codes.len(), 16);
    }

    #[test]
    fn flip_rgba_mirrors_pixels() {
        let (px, w, h) = tiny_rgba();
        assert_eq!(flip_rgba(&px, w, h, false, false), px);
        let hflip = flip_rgba(&px, w, h, true, false);
        let vflip = flip_rgba(&px, w, h, false, true);
        assert_eq!(flip_rgba(&hflip, w, h, true, false), px);
        assert_eq!(flip_rgba(&vflip, w, h, false, true), px);
        let both = flip_rgba(&px, w, h, true, true);
        let (rot, _, _) = rotate_rgba_quarter_turns(&px, w, h, 2);
        assert_eq!(both, rot);
        assert_ne!(hflip, px);
        // First pixel of the flipped row is the last of the original.
        let w = w as usize;
        assert_eq!(&hflip[0..4], &px[(w - 1) * 4..w * 4]);
    }

    #[test]
    fn orient_applies_flip_then_rotation() {
        let (px, w, h) = tiny_rgba();
        let o = ViewerOrientation {
            turns: 1,
            flip_h: true,
            flip_v: false,
        };
        let (out, ow, oh) = orient_rgba(&px, w, h, o);
        let flipped = flip_rgba(&px, w, h, true, false);
        let (expect, ew, eh) = rotate_rgba_quarter_turns(&flipped, w, h, 1);
        assert_eq!((out, ow, oh), (expect, ew, eh));
    }

    #[test]
    fn pan_clamps_inside_frame_and_ignores_fit_zoom() {
        let mut zoom = ViewerZoom::new();
        zoom.pan_by(50.0, 50.0, (720.0, 480.0));
        assert_eq!(zoom.pan, (0.0, 0.0));
        zoom.step(true, (720.0, 480.0));
        zoom.pan_by(10_000.0, -10_000.0, (720.0, 480.0));
        assert_eq!(zoom.pan.0, 0.0);
        assert_eq!(zoom.pan.1, 480.0 - 480.0 * zoom.zoom);
        zoom.reset();
        assert!(!zoom.is_zoomed());
        assert_eq!(zoom.pan, (0.0, 0.0));
    }

    fn ladder(levels: &ZoomLevels, from: f32, zoom_in: bool, steps: usize) -> Vec<f32> {
        let mut scale = from;
        (0..steps)
            .map(|_| {
                scale = levels.step(scale, zoom_in);
                scale
            })
            .collect()
    }

    fn assert_scales(got: &[f32], want: &[f32]) {
        assert_eq!(got.len(), want.len(), "{got:?} vs {want:?}");
        for (g, w) in got.iter().zip(want) {
            assert!((g - w).abs() < 1e-4, "{got:?} vs {want:?}");
        }
    }

    #[test]
    fn level_scales_follow_tdesktop() {
        assert_eq!(zoom_level_scale(0), 1.0);
        assert_eq!(zoom_level_scale(1), 2.0);
        assert_eq!(zoom_level_scale(7), 8.0);
        assert_eq!(zoom_level_scale(-1), 0.5);
        assert_eq!(zoom_level_scale(-7), 0.125);
    }

    #[test]
    fn ladder_needs_real_sizes() {
        assert!(ZoomLevels::new((0.0, 100.0), (400.0, 300.0)).is_none());
        assert!(ZoomLevels::new((100.0, 100.0), (400.0, 0.0)).is_none());
        assert!(ZoomLevels::new((f32::NAN, 100.0), (400.0, 300.0)).is_none());
        let levels = ZoomLevels::new((200.0, 100.0), (400.0, 300.0)).unwrap();
        assert_eq!(levels.fit(), 2.0);
    }

    #[test]
    fn fit_on_a_level_is_one_stop() {
        let levels = ZoomLevels::new((200.0, 100.0), (400.0, 200.0)).unwrap();
        assert_eq!(levels.fit(), zoom_level_scale(1));
        assert_scales(&ladder(&levels, 2.0, true, 2), &[3.0, 4.0]);
        assert_scales(&ladder(&levels, 2.0, false, 2), &[1.0, 0.5]);
    }

    #[test]
    fn steps_from_fit_to_natural_and_back() {
        let levels = ZoomLevels::new((1000.0, 500.0), (400.0, 400.0)).unwrap();
        assert_scales(&[levels.fit()], &[0.4]);
        assert_scales(&ladder(&levels, 0.4, true, 3), &[0.5, 1.0, 2.0]);
        assert_scales(&ladder(&levels, 2.0, false, 4), &[1.0, 0.5, 0.4, 1.0 / 3.0]);
    }

    #[test]
    fn steps_stop_at_the_level_limits() {
        let levels = ZoomLevels::new((4000.0, 3000.0), (800.0, 600.0)).unwrap();
        let down = ladder(&levels, levels.fit(), false, 6);
        assert_scales(&down[3..], &[0.125, 0.125, 0.125]);
        let up = ladder(&levels, 1.0, true, 9);
        assert_scales(&up[6..], &[8.0, 8.0, 8.0]);
    }

    #[test]
    fn snaps_past_the_limits_stay_reachable() {
        // A tiny picture fits at 30x, past level 7.
        let levels = ZoomLevels::new((10.0, 10.0), (400.0, 300.0)).unwrap();
        assert_scales(&ladder(&levels, 8.0, true, 2), &[30.0, 30.0]);
        assert_scales(&ladder(&levels, 30.0, false, 2), &[8.0, 7.0]);
    }

    #[test]
    fn tiny_pictures_never_shrink_below_a_point() {
        let levels = ZoomLevels::new((4.0, 2.0), (400.0, 300.0)).unwrap();
        assert_scales(&ladder(&levels, 1.0, false, 3), &[0.5, 0.5, 0.5]);
    }

    #[test]
    fn viewer_zoom_steps_along_the_ladder_and_centers_below_fit() {
        let frame = (400.0, 200.0);
        let levels = ZoomLevels::new((1000.0, 500.0), (400.0, 400.0));
        let mut zoom = ViewerZoom::fitted(levels);
        assert!((zoom.percent() - 40.0).abs() < 1e-3);
        zoom.step(true, frame);
        assert!((zoom.zoom - 1.25).abs() < 1e-5);
        assert!((zoom.percent() - 50.0).abs() < 1e-3);
        zoom.step(false, frame);
        assert_eq!(zoom.zoom, 1.0, "fit is an exact stop");
        assert_eq!(zoom.pan, (0.0, 0.0));
        zoom.step(false, frame);
        assert!(!zoom.is_zoomed());
        assert!((zoom.percent() - 100.0 / 3.0).abs() < 1e-3);
        // Smaller than the frame: centered, and drag does not move it.
        let centered = zoom.pan;
        assert!((centered.0 - (400.0 - 400.0 * zoom.zoom) / 2.0).abs() < 1e-3);
        assert!((centered.1 - (200.0 - 200.0 * zoom.zoom) / 2.0).abs() < 1e-3);
        zoom.pan_by(30.0, 30.0, frame);
        assert_eq!(zoom.pan, centered);
        zoom.reset();
        assert_eq!(zoom, ViewerZoom::fitted(levels));
    }

    #[test]
    fn a_wheel_below_fit_only_moves_back_toward_fit() {
        let frame = (400.0, 200.0);
        let mut zoom = ViewerZoom::fitted(ZoomLevels::new((1000.0, 500.0), (400.0, 400.0)));
        zoom.step(false, frame);
        let below = zoom.zoom;
        assert!(below < 1.0);
        zoom.zoom_at(
            wheel_zoom_factor(-VIEWER_WHEEL_NOTCH_PX),
            (10.0, 10.0),
            frame,
        );
        assert_eq!(zoom.zoom, below, "no further out on the wheel");
        zoom.zoom_at(
            wheel_zoom_factor(VIEWER_WHEEL_NOTCH_PX),
            (10.0, 10.0),
            frame,
        );
        assert!((zoom.zoom - below * VIEWER_ZOOM_STEP).abs() < 1e-5);
        for _ in 0..10 {
            zoom.zoom_at(VIEWER_ZOOM_STEP, (10.0, 10.0), frame);
        }
        assert!(zoom.zoom > 1.0, "continuous on through fit");
    }

    /// 2×1 RGBA: red | green. 90° cw -> 1×2: green on top, red below.
    fn tiny_rgba() -> (Vec<u8>, u32, u32) {
        (
            vec![
                255, 0, 0, 255, // red
                0, 255, 0, 255, // green
            ],
            2,
            1,
        )
    }

    #[test]
    fn rotate_zero_turns_is_identity() {
        let (px, w, h) = tiny_rgba();
        assert_eq!(rotate_rgba_quarter_turns(&px, w, h, 0), (px.clone(), w, h));
        assert_eq!(
            rotate_rgba_quarter_turns(&px, w, h, 4),
            (tiny_rgba().0, w, h)
        );
    }

    #[test]
    fn rotate_90_cw_swaps_dims_and_pixels() {
        let (px, w, h) = tiny_rgba();
        let (out, ow, oh) = rotate_rgba_quarter_turns(&px, w, h, 1);
        assert_eq!((ow, oh), (1, 2));
        // 90°cw: (x,y)->(h-1-y, x): red (0,0) at (0,0), green (1,0) at (0,1).
        assert_eq!(&out[0..4], &[255, 0, 0, 255]); // red on top
        assert_eq!(&out[4..8], &[0, 255, 0, 255]); // green below
    }

    #[test]
    fn rotate_180_and_270() {
        let (px, w, h) = tiny_rgba();
        let (out, ow, oh) = rotate_rgba_quarter_turns(&px, w, h, 2);
        assert_eq!((ow, oh), (2, 1));
        assert_eq!(&out[0..4], &[0, 255, 0, 255]); // green first
        assert_eq!(&out[4..8], &[255, 0, 0, 255]);
        let (out, ow, oh) = rotate_rgba_quarter_turns(&px, w, h, 3);
        assert_eq!((ow, oh), (1, 2));
        // 270°cw: (x,y)->(y, w-1-x): green (1,0) at (0,0), red (0,0) at (0,1).
        assert_eq!(&out[0..4], &[0, 255, 0, 255]); // green on top
        assert_eq!(&out[4..8], &[255, 0, 0, 255]); // red below
    }

    #[test]
    fn rotate_four_turns_returns_to_start() {
        let (px, w, h) = tiny_rgba();
        let once = rotate_rgba_quarter_turns(&px, w, h, 1);
        let back = rotate_rgba_quarter_turns(&once.0, once.1, once.2, 3);
        assert_eq!(back, (px, w, h));
    }

    #[test]
    fn fits_by_the_tighter_side() {
        assert_eq!(fit_within((1200.0, 800.0), (640.0, 400.0)), (600.0, 400.0));
        assert_eq!(fit_within((800.0, 1200.0), (640.0, 400.0)).1, 400.0);
        assert_eq!(fit_within((300.0, 200.0), (640.0, 400.0)), (600.0, 400.0));
        assert_eq!(fit_within((0.0, 0.0), (640.0, 400.0)), (640.0, 400.0));
    }
}
