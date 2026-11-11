//! Where the viewer's chrome may sit: clear of the Dock, taskbar or panel
//! that covers part of the window, and where the user dragged the video
//! controller to.
//!
//! tdesktop shows its viewer in a window of its own, maximized to the
//! screen's available area (`OverlayWidget::updateGeometry`), so its
//! controller is never under the taskbar. Quill draws the viewer inside
//! the window it was opened from, which may reach under the Dock; the
//! chrome keeps to the part of the window the screen leaves visible.

/// A rectangle in points: window coordinates, or the screen coordinates
/// the platform reports window frames and work areas in.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ViewRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl ViewRect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(self) -> f32 {
        self.y + self.h
    }

    fn finite(self) -> bool {
        [self.x, self.y, self.w, self.h]
            .iter()
            .all(|value| value.is_finite())
            && self.w > 0.0
            && self.h > 0.0
    }

    fn overlaps(self, other: Self) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }
}

/// How far in from each edge of the window the screen hides it: the
/// Dock, the taskbar, a panel, or the screen's own edge.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScreenCover {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl ScreenCover {
    /// The part of a `viewport` (window coordinates) the cover leaves.
    pub fn safe_area(self, viewport: (f32, f32)) -> ViewRect {
        ViewRect::new(
            self.left,
            self.top,
            (viewport.0 - self.left - self.right).max(0.0),
            (viewport.1 - self.top - self.bottom).max(0.0),
        )
    }
}

/// Covers thinner than this are left alone: the strip an auto-hidden Dock
/// keeps to show itself (4 pt) and the invisible resize border Windows
/// counts in a window's frame (7 px). The chrome's own margins clear them.
pub const COVER_SLIVER: f32 = 12.0;
/// No edge's cover takes more than this share of the window, so a window
/// dragged mostly off the screen still has room for its chrome.
const COVER_MAX_SHARE: f32 = 0.4;

/// The cover of a `window` frame by everything outside the `work` area of
/// its screen (both in the same screen coordinates). A frame that does not
/// meet the work area at all (the wrong screen's area) gets none.
pub fn screen_cover(window: ViewRect, work: ViewRect) -> ScreenCover {
    if !window.finite() || !work.finite() || !window.overlaps(work) {
        return ScreenCover::default();
    }
    let edge = |covered: f32, size: f32| {
        if covered < COVER_SLIVER {
            0.0
        } else {
            covered.min(size * COVER_MAX_SHARE)
        }
    };
    ScreenCover {
        top: edge(work.y - window.y, window.h),
        right: edge(window.right() - work.right(), window.w),
        bottom: edge(window.bottom() - work.bottom(), window.h),
        left: edge(work.x - window.x, window.w),
    }
}

/// Where the user left the controller: its home (centered at the bottom)
/// or dragged away from it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ControllerSpot {
    #[default]
    Home,
    /// Moved by this much from home, in points.
    Moved { dx: f32, dy: f32 },
}

impl ControllerSpot {
    fn offset(self) -> (f32, f32) {
        match self {
            Self::Home => (0.0, 0.0),
            Self::Moved { dx, dy } => (dx, dy),
        }
    }
}

/// A press on the controller's panel, from mouse-down to mouse-up.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ControllerDrag {
    #[default]
    Idle,
    /// Held: the pointer went down at `grab` with the controller at `from`
    /// (its offset from home then).
    Held { grab: (f32, f32), from: (f32, f32) },
}

/// Within this distance of home a dragged controller snaps back onto it.
pub const CONTROLLER_SNAP: f32 = 24.0;
/// A press that moves less than this is a click, not a drag.
const CONTROLLER_SLOP: f32 = 3.0;
/// The controller keeps this far inside the safe area when dragged.
pub const CONTROLLER_MARGIN: f32 = 8.0;

/// The video controller's place: where it rests and the drag moving it.
/// Lives as long as the viewer state of its account, so a moved
/// controller stays put from one video to the next.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ControllerPlace {
    spot: ControllerSpot,
    drag: ControllerDrag,
}

impl ControllerPlace {
    pub fn spot(&self) -> ControllerSpot {
        self.spot
    }

    pub fn is_held(&self) -> bool {
        matches!(self.drag, ControllerDrag::Held { .. })
    }

    /// The pointer went down on the panel at `at` (window points).
    pub fn press(&mut self, at: (f32, f32), home: ViewRect, area: ViewRect) {
        self.drag = ControllerDrag::Held {
            grab: at,
            from: self.offset(home, area),
        };
    }

    /// The pointer moved to `at` with the button held. Returns whether the
    /// controller moved. Close to home it snaps onto it.
    pub fn drag_to(&mut self, at: (f32, f32), home: ViewRect, area: ViewRect) -> bool {
        let ControllerDrag::Held { grab, from } = self.drag else {
            return false;
        };
        let (mx, my) = (at.0 - grab.0, at.1 - grab.1);
        if mx.hypot(my) < CONTROLLER_SLOP && self.spot.offset() == from {
            return false;
        }
        let (dx, dy) = clamp_offset((from.0 + mx, from.1 + my), home, area);
        let next = if dx.hypot(dy) < CONTROLLER_SNAP {
            ControllerSpot::Home
        } else {
            ControllerSpot::Moved { dx, dy }
        };
        let moved = next != self.spot;
        self.spot = next;
        moved
    }

    /// The button came up (or the press was lost): the drag ends where it
    /// is.
    pub fn release(&mut self) -> bool {
        let was_held = self.is_held();
        self.drag = ControllerDrag::Idle;
        was_held
    }

    /// Back home (a double-click on the panel, the reset button).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// The offset from home to draw the controller at: the spot, kept
    /// inside the safe `area` as the window resizes.
    pub fn offset(&self, home: ViewRect, area: ViewRect) -> (f32, f32) {
        clamp_offset(self.spot.offset(), home, area)
    }
}

/// `offset` limited so the controller at `home` moved by it stays inside
/// `area`, `CONTROLLER_MARGIN` in from its edges. An area too small for the
/// controller leaves it at home on that axis.
pub fn clamp_offset(offset: (f32, f32), home: ViewRect, area: ViewRect) -> (f32, f32) {
    let axis = |offset: f32, start: f32, size: f32, area_start: f32, area_size: f32| {
        if !offset.is_finite() {
            return 0.0;
        }
        let low = area_start + CONTROLLER_MARGIN - start;
        let high = area_start + area_size - CONTROLLER_MARGIN - size - start;
        if low > high {
            0.0
        } else {
            offset.clamp(low, high)
        }
    };
    (
        axis(offset.0, home.x, home.w, area.x, area.w),
        axis(offset.1, home.y, home.h, area.y, area.h),
    )
}

#[cfg(test)]
mod tests;
