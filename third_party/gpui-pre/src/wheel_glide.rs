//! Quill: mouse wheel notches glide instead of jumping.
//!
//! A wheel reports a notch as [`ScrollDelta::Lines`]. A scroll area that
//! glides does not move by the whole notch at once: it hands the distance to
//! its [`WheelGlider`], which moves the area a little on each frame until the
//! notch is covered, on an ease-out curve. A notch that arrives mid-glide
//! adds to what is left; one in the other direction drops what is left.
//! Trackpads report pixels and never glide, and neither does anything while
//! the app asks for reduced motion.

use std::{cell::Cell, rc::Rc, time::Duration};

use scheduler::Instant;

use crate::{
    App, IsZero as _, Pixels, Point, ScrollDelta, ScrollEvent, ScrollSource, Window, point, px,
};

/// How long a notch takes to glide to its target.
pub const WHEEL_GLIDE_DURATION: Duration = Duration::from_millis(150);

/// Whether `event` is a wheel notch that glides: a line delta from a pointer,
/// while the app does not ask for reduced motion.
pub fn wheel_notch_glides(event: &ScrollEvent, cx: &App) -> bool {
    matches!(event.delta, ScrollDelta::Lines(_))
        && event.source == ScrollSource::Pointer
        && !cx.reduce_motion()
}

/// Where one scroll area's wheel glide is.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum WheelGlide {
    /// Nothing to cover.
    #[default]
    Resting,
    /// Covering `distance` from `leg_start`, the last notch.
    Gliding {
        /// When the current leg started.
        leg_start: Instant,
        /// How far the current leg goes, in content movement (the sign of a
        /// scroll delta).
        distance: Point<Pixels>,
        /// How much of `distance` the area has been moved by already.
        moved: Point<Pixels>,
        /// When the first notch of this glide arrived. The frame steps a
        /// glide schedules carry it, so a step left from a glide that was
        /// stopped does not drive the next one.
        since: Instant,
    },
}

impl WheelGlide {
    /// The glide with a notch of `delta` added at `now`. What is left goes on
    /// in the same direction; on an axis where the notch points the other
    /// way, what is left is dropped.
    pub fn push(self, delta: Point<Pixels>, now: Instant) -> Self {
        let (left, since) = match self {
            Self::Resting => (Point::default(), now),
            Self::Gliding {
                distance,
                moved,
                since,
                ..
            } => (point(distance.x - moved.x, distance.y - moved.y), since),
        };
        let delta = delta.map(|axis| {
            if axis.as_f32().is_finite() {
                axis
            } else {
                px(0.)
            }
        });
        let distance = point(join(left.x, delta.x), join(left.y, delta.y));
        if distance.x.is_zero() && distance.y.is_zero() {
            return Self::Resting;
        }
        Self::Gliding {
            leg_start: now,
            distance,
            moved: Point::default(),
            since,
        }
    }

    /// The movement due at `now` since the last step, and the glide after
    /// it. A leg past its duration hands over the rest and rests.
    pub fn step(self, now: Instant) -> (Point<Pixels>, Self) {
        let Self::Gliding {
            leg_start,
            distance,
            moved,
            since,
        } = self
        else {
            return (Point::default(), Self::Resting);
        };
        let elapsed = now.saturating_duration_since(leg_start).as_secs_f32();
        let progress = elapsed / WHEEL_GLIDE_DURATION.as_secs_f32();
        if !progress.is_finite() || progress >= 1. {
            let rest = point(distance.x - moved.x, distance.y - moved.y);
            return (rest, Self::Resting);
        }
        let eased = ease_out_cubic(progress.max(0.));
        let reached = point(distance.x * eased, distance.y * eased);
        let due = point(reached.x - moved.x, reached.y - moved.y);
        (
            due,
            Self::Gliding {
                leg_start,
                distance,
                moved: reached,
                since,
            },
        )
    }

    /// When this glide's first notch arrived; `None` at rest.
    pub fn since(&self) -> Option<Instant> {
        match self {
            Self::Resting => None,
            Self::Gliding { since, .. } => Some(*since),
        }
    }

    /// How far the area still has to move.
    pub fn left(&self) -> Point<Pixels> {
        match self {
            Self::Resting => Point::default(),
            Self::Gliding {
                distance, moved, ..
            } => point(distance.x - moved.x, distance.y - moved.y),
        }
    }
}

/// One axis of a notch added to what a glide has left.
fn join(left: Pixels, delta: Pixels) -> Pixels {
    let reverses = left.as_f32() * delta.as_f32() < 0.;
    if reverses { delta } else { left + delta }
}

fn ease_out_cubic(t: f32) -> f32 {
    let inverse = 1. - t.min(1.);
    1. - inverse * inverse * inverse
}

/// The wheel glide of one scroll area, shared by the area's wheel listener,
/// the frame steps it schedules and the area's handle. Setting the area's
/// position from code (a jump, a scrollbar drag, a trackpad) stops it.
#[derive(Clone, Debug, Default)]
pub struct WheelGlider(Rc<Cell<WheelGlide>>);

impl WheelGlider {
    /// Where the glide is.
    pub fn glide(&self) -> WheelGlide {
        self.0.get()
    }

    /// Stop gliding; the area stays where it is.
    pub fn stop(&self) {
        self.0.set(WheelGlide::Resting);
    }

    /// Add a notch of `delta` (in scroll delta direction) and, when the area
    /// was at rest, step it on each following frame until it is covered.
    /// `apply` moves the area by a step's delta and says whether it moved;
    /// an area that cannot move (at its edge) ends the glide.
    pub fn push(
        &self,
        delta: Point<Pixels>,
        window: &Window,
        cx: &App,
        apply: impl Fn(Point<Pixels>, &mut Window, &mut App) -> bool + 'static,
    ) {
        let now = cx.background_executor().now();
        let before = self.0.get();
        let after = before.push(delta, now);
        self.0.set(after);
        if let (None, Some(since)) = (before.since(), after.since()) {
            schedule_step(self.clone(), since, Rc::new(apply), window);
        }
    }
}

type ApplyStep = Rc<dyn Fn(Point<Pixels>, &mut Window, &mut App) -> bool>;

fn schedule_step(glider: WheelGlider, since: Instant, apply: ApplyStep, window: &Window) {
    window.on_next_frame(move |window, cx| {
        let glide = glider.0.get();
        if glide.since() != Some(since) {
            return;
        }
        let (due, next) = glide.step(cx.background_executor().now());
        glider.0.set(next);
        let moved = (due.x.abs() < px(0.01) && due.y.abs() < px(0.01)) || apply(due, window, cx);
        if !moved {
            glider.stop();
        }
        if glider.0.get().since() == Some(since) {
            schedule_step(glider, since, apply, window);
        }
    });
}
