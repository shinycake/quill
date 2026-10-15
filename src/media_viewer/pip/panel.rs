//! The Picture-in-Picture window's geometry: tdesktop's `PipPanel` math
//! (`Telegram/SourceFiles/media/view/media_view_pip.cpp`), pure and in
//! logical pixels. The UI feeds window frames, screen areas and pointer
//! positions in and applies the frames that come out.
//!
//! Quill draws no shadow margin around the picture (tdesktop's opaque path,
//! `useTransparency` false), so the window frame is the picture's rectangle
//! (`inner` in tdesktop).

use serde::{Deserialize, Serialize};

/// `pipDefaultSize`: the longer side of a new window.
pub const DEFAULT_SIZE: i32 = 320;
/// `pipMinimalSize`: the shorter side never gets smaller.
pub const MINIMAL_SIZE: i32 = 120;
/// `pipBorderSkip`: the gap a snapped window keeps from the screen edge.
pub const BORDER_SKIP: i32 = 20;
/// `pipBorderSnapArea`: how close to the snap line a move snaps.
pub const BORDER_SNAP_AREA: i32 = 16;
/// `pipResizeArea`: the resize band along each free edge.
pub const RESIZE_AREA: i32 = 10;
/// Qt's `QApplication::startDragDistance()` default: a press becomes a
/// drag once the pointer moved this far (Manhattan length).
pub const START_DRAG_DISTANCE: i32 = 10;
/// `st::slideWrapDuration`: the snap animation (`moveAnimated`).
pub const MOVE_ANIMATION_MS: u64 = 150;

/// A point in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipPoint {
    pub x: i32,
    pub y: i32,
}

impl PipPoint {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Qt's `manhattanLength`.
    pub fn manhattan(self) -> i32 {
        self.x
            .saturating_abs()
            .saturating_add(self.y.saturating_abs())
    }

    pub fn minus(self, other: PipPoint) -> PipPoint {
        PipPoint::new(
            self.x.saturating_sub(other.x),
            self.y.saturating_sub(other.y),
        )
    }
}

/// A size in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipSize {
    pub w: i32,
    pub h: i32,
}

impl PipSize {
    pub const fn new(w: i32, h: i32) -> Self {
        Self { w, h }
    }

    /// Qt's `QSize::isEmpty`: a side is zero or less.
    pub fn is_empty(self) -> bool {
        self.w <= 0 || self.h <= 0
    }
}

/// A rectangle in logical pixels (Qt's `QRect` without the off-by-one
/// `right()`: `right` here is `x + w`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl PipRect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    pub fn from_parts(origin: PipPoint, size: PipSize) -> Self {
        Self::new(origin.x, origin.y, size.w, size.h)
    }

    pub fn right(self) -> i32 {
        self.x.saturating_add(self.w)
    }

    pub fn bottom(self) -> i32 {
        self.y.saturating_add(self.h)
    }

    pub fn origin(self) -> PipPoint {
        PipPoint::new(self.x, self.y)
    }

    pub fn size(self) -> PipSize {
        PipSize::new(self.w, self.h)
    }

    pub fn is_empty(self) -> bool {
        self.size().is_empty()
    }

    /// Qt's `QRect::contains` (the right and bottom edges are outside).
    pub fn contains(self, point: PipPoint) -> bool {
        point.x >= self.x && point.x < self.right() && point.y >= self.y && point.y < self.bottom()
    }

    pub fn translated(self, by: PipPoint) -> Self {
        Self::new(
            self.x.saturating_add(by.x),
            self.y.saturating_add(by.y),
            self.w,
            self.h,
        )
    }

    /// The rectangle `skip` in from every side.
    pub fn inset(self, skip: i32) -> Self {
        Self::new(
            self.x.saturating_add(skip),
            self.y.saturating_add(skip),
            self.w.saturating_sub(skip.saturating_mul(2)),
            self.h.saturating_sub(skip.saturating_mul(2)),
        )
    }
}

/// Sides of a rectangle (tdesktop `RectParts` restricted to the sides).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Sides(u8);

impl Sides {
    pub const NONE: Sides = Sides(0);
    pub const LEFT: Sides = Sides(1);
    pub const TOP: Sides = Sides(2);
    pub const RIGHT: Sides = Sides(4);
    pub const BOTTOM: Sides = Sides(8);
    pub const ALL: Sides = Sides(15);

    pub fn contains(self, side: Sides) -> bool {
        self.0 & side.0 != 0
    }

    pub fn with(self, side: Sides) -> Sides {
        Sides(self.0 | side.0)
    }

    pub fn bits(self) -> u8 {
        self.0
    }

    pub fn from_bits(bits: u8) -> Sides {
        Sides(bits & Self::ALL.0)
    }
}

/// Where a press landed on the window (tdesktop `RectPart`): an edge or a
/// corner resizes, the center moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RectPart {
    Center,
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
}

impl RectPart {
    fn is_corner(self) -> bool {
        matches!(
            self,
            RectPart::TopLeft | RectPart::TopRight | RectPart::BottomRight | RectPart::BottomLeft
        )
    }
}

/// How [`scaled`] keeps the aspect (Qt's `Qt::AspectRatioMode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AspectMode {
    /// As large as possible inside the target.
    Keep,
    /// As small as possible around the target.
    Expand,
}

fn mul_div(a: i32, b: i32, c: i32) -> i32 {
    if c == 0 {
        return a;
    }
    let value = i64::from(a) * i64::from(b) / i64::from(c);
    i32::try_from(value).unwrap_or(if value < 0 { i32::MIN } else { i32::MAX })
}

/// Qt's `QSize::scaled(target, mode)`: `ratio` scaled to fit `target`.
pub fn scaled(ratio: PipSize, target: PipSize, mode: AspectMode) -> PipSize {
    if ratio.w == 0 || ratio.h == 0 {
        return target;
    }
    let rw = mul_div(target.h, ratio.w, ratio.h);
    let use_height = match mode {
        AspectMode::Keep => rw <= target.w,
        AspectMode::Expand => rw >= target.w,
    };
    if use_height {
        PipSize::new(rw, target.h)
    } else {
        PipSize::new(target.w, mul_div(target.w, ratio.h, ratio.w))
    }
}

/// The picture's aspect, never empty (`PipPanel::setAspectRatio`).
pub fn aspect(width: u32, height: u32) -> PipSize {
    let w = i32::try_from(width).unwrap_or(i32::MAX);
    let h = i32::try_from(height).unwrap_or(i32::MAX);
    let size = PipSize::new(w, h);
    if size.is_empty() {
        PipSize::new(1, 1)
    } else {
        size
    }
}

/// `MaxAllowedSizeForScreen`: each side stays three border skips short of
/// the screen's, so the window never snaps to two opposite sides.
pub fn max_allowed_size(screen: PipSize) -> PipSize {
    let skip = 3 * BORDER_SKIP;
    PipSize::new(screen.w.saturating_sub(skip), screen.h.saturating_sub(skip))
}

/// The smallest and largest window for `ratio` on a screen this big
/// (`processDrag`, `handleScreenChanged`).
pub fn size_limits(ratio: PipSize, screen: PipSize) -> (PipSize, PipSize) {
    let minimal = scaled(
        ratio,
        PipSize::new(MINIMAL_SIZE, MINIMAL_SIZE),
        AspectMode::Expand,
    );
    let maximal = scaled(ratio, max_allowed_size(screen), AspectMode::Keep);
    (
        minimal,
        PipSize::new(maximal.w.max(minimal.w), maximal.h.max(minimal.h)),
    )
}

/// `ClampToEdges`: the window's top-left after snapping each edge that
/// came within [`BORDER_SNAP_AREA`] of the line [`BORDER_SKIP`] inside the
/// screen.
pub fn clamp_to_edges(screen: PipRect, inner: PipRect) -> PipPoint {
    let skip = BORDER_SKIP;
    let area = BORDER_SNAP_AREA;
    let sleft = screen.x + skip;
    let stop = screen.y + skip;
    let sright = screen.right() - skip;
    let sbottom = screen.bottom() - skip;
    let (ileft, itop, iright, ibottom) = (inner.x, inner.y, inner.right(), inner.bottom());
    let mut shiftx = 0;
    let mut shifty = 0;
    if iright + shiftx >= sright - area && iright + shiftx < sright + area {
        shiftx += sright - iright;
    }
    if ileft + shiftx >= sleft - area && ileft + shiftx < sleft + area {
        shiftx += sleft - ileft;
    }
    if ibottom + shifty >= sbottom - area && ibottom + shifty < sbottom + area {
        shifty += sbottom - ibottom;
    }
    if itop + shifty >= stop - area && itop + shifty < stop + area {
        shifty += stop - itop;
    }
    PipPoint::new(inner.x + shiftx, inner.y + shifty)
}

/// `Transformed`: the drag's raw result, `delta` applied to the dragged
/// part of `original` within the size limits.
pub fn transformed(
    original: PipRect,
    minimal: PipSize,
    maximal: PipSize,
    delta: PipPoint,
    by: RectPart,
) -> PipRect {
    let (width, height) = (original.w, original.h);
    let x1 = width - minimal.w;
    let x2 = maximal.w - width;
    let y1 = height - minimal.h;
    let y2 = maximal.h - height;
    // `std::clamp` with a reversed range is undefined in C++; here a
    // window already outside its limits simply does not move that edge.
    let clamp = |value: i32, low: i32, high: i32| {
        if low > high {
            0
        } else {
            value.clamp(low, high)
        }
    };
    let mut r = original;
    let move_left = |r: &mut PipRect, dx: i32| {
        r.x += dx;
        r.w -= dx;
    };
    let move_top = |r: &mut PipRect, dy: i32| {
        r.y += dy;
        r.h -= dy;
    };
    match by {
        RectPart::Center => return original.translated(delta),
        RectPart::TopLeft => {
            move_top(&mut r, clamp(delta.y, -y2, y1));
            move_left(&mut r, clamp(delta.x, -x2, x1));
        }
        RectPart::TopRight => {
            move_top(&mut r, clamp(delta.y, -y2, y1));
            r.w += clamp(delta.x, -x1, x2);
        }
        RectPart::BottomRight => {
            r.h += clamp(delta.y, -y1, y2);
            r.w += clamp(delta.x, -x1, x2);
        }
        RectPart::BottomLeft => {
            r.h += clamp(delta.y, -y1, y2);
            move_left(&mut r, clamp(delta.x, -x2, x1));
        }
        RectPart::Left => move_left(&mut r, clamp(delta.x, -x2, x1)),
        RectPart::Top => move_top(&mut r, clamp(delta.y, -y2, y1)),
        RectPart::Right => r.w += clamp(delta.x, -x1, x2),
        RectPart::Bottom => r.h += clamp(delta.y, -y1, y2),
    }
    r
}

/// `Constrained`: the transformed rectangle brought back to `ratio`,
/// anchored at the corner or edge opposite the dragged part (an edge drag
/// keeps an attached side in place, else stays centered on the edge).
pub fn constrained(
    original: PipRect,
    minimal: PipSize,
    maximal: PipSize,
    ratio: PipSize,
    by: RectPart,
    attached: Sides,
) -> PipRect {
    if by == RectPart::Center {
        return original;
    }
    if original.w == 0 && original.h == 0 {
        return PipRect::from_parts(original.origin(), ratio);
    }
    let width_larger =
        i64::from(original.w) * i64::from(ratio.h) > i64::from(original.h) * i64::from(ratio.w);
    let keep = by.is_corner()
        || (matches!(by, RectPart::Top | RectPart::Bottom) && width_larger)
        || (matches!(by, RectPart::Left | RectPart::Right) && !width_larger);
    let desired = scaled(
        ratio,
        original.size(),
        if keep {
            AspectMode::Keep
        } else {
            AspectMode::Expand
        },
    );
    let size = PipSize::new(
        desired.w.max(minimal.w).min(maximal.w.max(minimal.w)),
        desired.h.max(minimal.h).min(maximal.h.max(minimal.h)),
    );
    let dw = original.w - size.w;
    let dh = original.h - size.h;
    let vertical = if attached.contains(Sides::TOP) {
        0
    } else if attached.contains(Sides::BOTTOM) {
        dh
    } else {
        dh / 2
    };
    let horizontal = if attached.contains(Sides::LEFT) {
        0
    } else if attached.contains(Sides::RIGHT) {
        dw
    } else {
        dw / 2
    };
    let shift = match by {
        RectPart::Center | RectPart::BottomRight => PipPoint::new(0, 0),
        RectPart::TopLeft => PipPoint::new(dw, dh),
        RectPart::TopRight => PipPoint::new(0, dh),
        RectPart::BottomLeft => PipPoint::new(dw, 0),
        RectPart::Left => PipPoint::new(dw, vertical),
        RectPart::Top => PipPoint::new(horizontal, 0),
        RectPart::Right => PipPoint::new(0, vertical),
        RectPart::Bottom => PipPoint::new(horizontal, dh),
    };
    PipRect::from_parts(original.origin(), size).translated(shift)
}

/// Where the window sits on its screen (tdesktop `PipPanel::Position`):
/// flush with a screen side (`attached`) or on the snap line
/// [`BORDER_SKIP`] inside it (`snapped`). Saved between runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipPosition {
    #[serde(default)]
    pub attached: Sides,
    #[serde(default)]
    pub snapped: Sides,
    pub geometry: PipRect,
    /// The whole screen's bounds when saved.
    pub screen: PipRect,
}

impl PipPosition {
    /// `setPositionDefault`: the default size, snapped to the screen's
    /// top-left corner.
    pub fn default_on(screen: PipRect) -> Self {
        Self {
            attached: Sides::NONE,
            snapped: Sides::LEFT.with(Sides::TOP),
            geometry: PipRect::new(0, 0, DEFAULT_SIZE, DEFAULT_SIZE),
            screen,
        }
    }
}

/// `countPosition`: which sides `geometry` is attached or snapped to on
/// `available`. A window being moved (or on Wayland, where a client
/// can't place itself) counts as neither.
pub fn count_position(
    geometry: PipRect,
    available: PipRect,
    moving: bool,
    wayland: bool,
) -> (Sides, Sides) {
    let mut attached = Sides::NONE;
    let mut snapped = Sides::NONE;
    if moving || wayland {
        return (attached, snapped);
    }
    let skip = BORDER_SKIP;
    if geometry.x == available.x {
        attached = attached.with(Sides::LEFT);
    } else if geometry.right() == available.right() {
        attached = attached.with(Sides::RIGHT);
    } else if geometry.x == available.x + skip {
        snapped = snapped.with(Sides::LEFT);
    } else if geometry.right() == available.right() - skip {
        snapped = snapped.with(Sides::RIGHT);
    }
    if geometry.y == available.y {
        attached = attached.with(Sides::TOP);
    } else if geometry.bottom() == available.bottom() {
        attached = attached.with(Sides::BOTTOM);
    } else if geometry.y == available.y + skip {
        snapped = snapped.with(Sides::TOP);
    } else if geometry.bottom() == available.bottom() - skip {
        snapped = snapped.with(Sides::BOTTOM);
    }
    (attached, snapped)
}

/// What [`place_on_screen`] decided: the window's frame and the limits a
/// system resize must keep (`setMinimumSize` / `setMaximumSize`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub geometry: PipRect,
    pub minimal: PipSize,
    pub maximal: PipSize,
}

/// `setPositionOnScreen`: `position`'s size brought to `ratio` and the
/// screen's limits, then placed by its attached and snapped sides inside
/// `available` (the screen minus the taskbar or the Dock).
pub fn place_on_screen(position: &PipPosition, available: PipRect, ratio: PipSize) -> Placement {
    let ratio = if ratio.is_empty() {
        PipSize::new(1, 1)
    } else {
        ratio
    };
    let requested = position.geometry.size();
    let max = requested.w.max(requested.h);
    // Apply the aspect ratio.
    let scaled = if ratio.w > ratio.h {
        PipSize::new(max, mul_div(max, ratio.h, ratio.w))
    } else {
        PipSize::new(mul_div(max, ratio.w, ratio.h), max)
    };
    // Apply the maximum size.
    let fit = max_allowed_size(available.size());
    let by_width = i64::from(scaled.w) * i64::from(fit.h) > i64::from(scaled.h) * i64::from(fit.w);
    let normalized = if by_width && scaled.w > fit.w {
        PipSize::new(fit.w, mul_div(fit.w, scaled.h, scaled.w))
    } else if !by_width && scaled.h > fit.h {
        PipSize::new(mul_div(fit.h, scaled.w, scaled.h), fit.h)
    } else {
        scaled
    };
    // Apply the minimal size.
    let min = MINIMAL_SIZE;
    let minimal = if ratio.w > ratio.h {
        PipSize::new(mul_div(min, ratio.w, ratio.h), min)
    } else {
        PipSize::new(min, mul_div(min, ratio.h, ratio.w))
    };
    let size = PipSize::new(normalized.w.max(minimal.w), normalized.h.max(minimal.h));
    // Apply the maximal size.
    let maximal = if by_width {
        PipSize::new(fit.w, mul_div(fit.w, ratio.h, ratio.w))
    } else {
        PipSize::new(mul_div(fit.h, ratio.w, ratio.h), fit.h)
    };
    // The screen borders.
    let screen = available;
    let inner = screen.inset(BORDER_SKIP);
    let mut geometry = PipRect::from_parts(position.geometry.origin(), size);
    if position.attached.contains(Sides::LEFT) || geometry.x < screen.x {
        geometry.x = screen.x;
    } else if position.attached.contains(Sides::RIGHT) || geometry.right() > screen.right() {
        geometry.x = screen.right() - geometry.w;
    } else if position.snapped.contains(Sides::LEFT) {
        geometry.x = inner.x;
    } else if position.snapped.contains(Sides::RIGHT) {
        geometry.x = inner.right() - geometry.w;
    }
    if position.attached.contains(Sides::TOP) || geometry.y < screen.y {
        geometry.y = screen.y;
    } else if position.attached.contains(Sides::BOTTOM) || geometry.bottom() > screen.bottom() {
        geometry.y = screen.bottom() - geometry.h;
    } else if position.snapped.contains(Sides::TOP) {
        geometry.y = inner.y;
    } else if position.snapped.contains(Sides::BOTTOM) {
        geometry.y = inner.bottom() - geometry.h;
    }
    Placement {
        geometry,
        minimal,
        maximal: PipSize::new(maximal.w.max(minimal.w), maximal.h.max(minimal.h)),
    }
}

/// `updateOverState`: the part of a `size` window under `point` (window
/// coordinates). Sides in `ignore` (attached or snapped) have no resize
/// band.
pub fn resize_part(point: PipPoint, size: PipSize, ignore: Sides) -> RectPart {
    let band = |side: Sides| {
        if ignore.contains(side) {
            0
        } else {
            RESIZE_AREA
        }
    };
    let left = band(Sides::LEFT);
    let top = band(Sides::TOP);
    let right = band(Sides::RIGHT);
    let bottom = band(Sides::BOTTOM);
    let (width, height) = (size.w, size.h);
    if point.x < left {
        if point.y < top {
            RectPart::TopLeft
        } else if point.y >= height - bottom {
            RectPart::BottomLeft
        } else {
            RectPart::Left
        }
    } else if point.x >= width - right {
        if point.y < top {
            RectPart::TopRight
        } else if point.y >= height - bottom {
            RectPart::BottomRight
        } else {
            RectPart::Right
        }
    } else if point.y < top {
        RectPart::Top
    } else if point.y >= height - bottom {
        RectPart::Bottom
    } else {
        RectPart::Center
    }
}

/// What one pointer move of a drag does to the window (`processDrag`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragStep {
    /// Set the window's frame.
    Frame(PipRect),
    /// Slide the window's top-left here ([`MOVE_ANIMATION_MS`]): a move
    /// came within the snap area of an edge.
    Slide(PipPoint),
}

/// `processDrag`: the window started the drag at `start`; the pointer
/// moved by `delta`. `screen` is the available area of the screen under
/// the pointer for a move, else of the window's own screen.
pub fn drag_step(
    start: PipRect,
    delta: PipPoint,
    part: RectPart,
    ratio: PipSize,
    screen: PipRect,
    attached: Sides,
) -> Option<DragStep> {
    if screen.is_empty() {
        return None;
    }
    let (minimal, maximal) = size_limits(ratio, screen.size());
    let geometry = transformed(start, minimal, maximal, delta, part);
    let valid = constrained(geometry, minimal, maximal, ratio, part, attached);
    let clamped = if part == RectPart::Center {
        clamp_to_edges(screen, valid)
    } else {
        valid.origin()
    };
    Some(if clamped != valid.origin() {
        DragStep::Slide(clamped)
    } else {
        DragStep::Frame(valid)
    })
}

/// `finishDrag`: the window's top-left after a drag, pulled back inside
/// `screen` (the available area under the pointer). `None` when it is
/// already inside (and always on Wayland, which places windows itself).
pub fn finish_drag(frame: PipRect, screen: PipRect, wayland: bool) -> Option<PipPoint> {
    if wayland {
        return None;
    }
    let mut result = frame.origin();
    if result.x > screen.right() - frame.w {
        result.x = screen.right() - frame.w;
    }
    if result.x < screen.x {
        result.x = screen.x;
    }
    if result.y > screen.bottom() - frame.h {
        result.y = screen.bottom() - frame.h;
    }
    if result.y < screen.y {
        result.y = screen.y;
    }
    (result != frame.origin()).then_some(result)
}

/// `anim::easeOutCirc` at `t` in 0–1.
pub fn ease_out_circ(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0) - 1.0;
    (1.0 - t * t).max(0.0).sqrt()
}

/// The point `progress` (eased) of the way from `from` to `to`
/// (`updatePositionAnimated`).
pub fn slide_point(from: PipPoint, to: PipPoint, progress: f32) -> PipPoint {
    let eased = ease_out_circ(progress);
    let lerp = |a: i32, b: i32| a + ((b - a) as f32 * eased).round() as i32;
    PipPoint::new(lerp(from.x, to.x), lerp(from.y, to.y))
}

/// `handleWaylandResize`: a compositor resize keeps the aspect by
/// shrinking the side that grew too much.
pub fn wayland_normalized(size: PipSize, ratio: PipSize) -> PipSize {
    if ratio.is_empty() || size.is_empty() {
        return size;
    }
    let max = size.w.max(size.h);
    let scaled = if ratio.w > ratio.h {
        PipSize::new(max, mul_div(max, ratio.h, ratio.w))
    } else {
        PipSize::new(mul_div(max, ratio.w, ratio.h), max)
    };
    let by_width =
        i64::from(scaled.w) * i64::from(size.h) > i64::from(scaled.h) * i64::from(size.w);
    if by_width && scaled.w > size.w {
        PipSize::new(size.w, mul_div(size.w, scaled.h, scaled.w))
    } else if !by_width && scaled.h > size.h {
        PipSize::new(mul_div(size.h, scaled.w, scaled.h), size.h)
    } else {
        scaled
    }
}

#[cfg(test)]
#[path = "panel_tests.rs"]
mod tests;
