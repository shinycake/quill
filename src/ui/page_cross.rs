//! One page giving way to the next through snapshots (Floating's chat
//! switch and folder pages, Web K's W11 and W7).
//!
//! A `PageStage` sits where the page is drawn. When the page it is about
//! to draw is not the one it drew last, it starts a crossing: the old page
//! is the last frame's pixels (`Window::snapshot_last_frame`), the new one
//! is drawn live through a snapshot the stage owns (the `snapshot`
//! element), so a page still loading is captured again as it changes and
//! never shown stale. Where the last frame cannot be taken (glass in the
//! region, a picture too large) only the new page moves; where the window
//! has no snapshots the new page fades in. Pointer input to the page waits
//! for the crossing. Both sides read the stage's clock, so a capture can
//! hold them (`QUILL_MOTION_HOLD_MS`).

use super::motion::elapsed_since;
use gpui_kit::*;
use quill::layout_style::Curve;
use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// How a crossing looks: its length and where each page is drawn at eased
/// time `t` in a region `width` wide, `forward` or back.
pub(in crate::ui) struct CrossLook {
    /// The region's element id, and its pointer hold's.
    pub name: &'static str,
    pub hold: &'static str,
    pub duration: Duration,
    /// Texels per device pixel for the old page's picture.
    pub old_scale: f32,
    pub leaving: fn(t: f32, width: f32, forward: bool) -> SnapshotDraw,
    pub arriving: fn(t: f32, width: f32, forward: bool) -> SnapshotDraw,
}

/// A page crossing into `to` since `started`.
pub(in crate::ui) struct Crossing<K> {
    pub to: K,
    /// The old page's last frame; `None` when it could not be taken.
    from: Option<Rc<Snapshot>>,
    /// The new page's pixels; `None` where the window has no snapshots.
    into: Option<Snapshot>,
    started: Instant,
    forward: bool,
}

/// Where the stage is.
pub(in crate::ui) enum PageCross<K> {
    Idle,
    Crossing(Crossing<K>),
}

/// The stage of one region: its crossing, the page it drew last and where.
pub(in crate::ui) struct PageStage<K> {
    pub cross: PageCross<K>,
    drawn: Option<K>,
    region: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl<K> Default for PageStage<K> {
    fn default() -> Self {
        Self {
            cross: PageCross::Idle,
            drawn: None,
            region: Rc::default(),
        }
    }
}

impl<K: Copy + PartialEq> PageStage<K> {
    /// The page being crossed into, and whether forward, if any.
    #[cfg(test)]
    pub(in crate::ui) fn crossing(&self) -> Option<(K, bool)> {
        match &self.cross {
            PageCross::Idle => None,
            PageCross::Crossing(run) => Some((run.to, run.forward)),
        }
    }

    /// The layout cuts: forget the stage, so turning the motion back on
    /// starts from the page drawn then.
    pub(in crate::ui) fn cut(&mut self) {
        self.cross = PageCross::Idle;
        self.drawn = None;
    }

    /// Before the region draws `shown` at `now`: start, keep or end the
    /// crossing. `forward` says which way a move from one page to another
    /// goes, `None` for a pair that does not cross. Returns whether it runs.
    pub(in crate::ui) fn settle(
        &mut self,
        shown: Option<K>,
        animate: bool,
        forward: impl FnOnce(K, K) -> Option<bool>,
        look: &CrossLook,
        window: &mut Window,
        now: Instant,
    ) -> bool {
        let drawn = std::mem::replace(&mut self.drawn, shown);
        if let (Some(to), Some(left)) = (shown, drawn)
            && to != left
        {
            // Another page mid-run replaces the stage: its pictures drop.
            self.cross = PageCross::Idle;
            if let Some(forward) = forward(left, to).filter(|_| animate) {
                let from = self
                    .region
                    .get()
                    .and_then(|region| window.snapshot_last_frame(region, look.old_scale))
                    .map(Rc::new);
                let into = window.snapshots_supported().then(|| window.new_snapshot());
                self.cross = PageCross::Crossing(Crossing {
                    to,
                    from,
                    into,
                    started: now,
                    forward,
                });
            }
        }
        let running = match &self.cross {
            PageCross::Idle => false,
            PageCross::Crossing(run) => {
                shown == Some(run.to) && animate && progress(run.started, now, look) < 1.
            }
        };
        if !running {
            self.cross = PageCross::Idle;
        }
        running
    }

    /// `page` as the stage draws it at `now`, filling a flex parent: as it
    /// is while idle (its bounds noted for the next crossing); while
    /// crossing, the new page through its snapshot (or fading in), the old
    /// page's picture over it, and a layer that keeps the pointer off both.
    pub(in crate::ui) fn draw(
        &self,
        page: AnyElement,
        look: &CrossLook,
        now: Instant,
    ) -> AnyElement {
        let region = self.region.clone();
        let note = canvas(
            move |bounds, _, _| region.set(Some(bounds)),
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0();
        let frame = div()
            .id(look.name)
            .relative()
            .flex()
            .flex_1()
            .min_w_0()
            .min_h_0();
        let PageCross::Crossing(run) = &self.cross else {
            return frame.child(page).child(note).into_any_element();
        };
        let t = Curve::Standard.ease(progress(run.started, now, look));
        let width = self
            .region
            .get()
            .map_or(0., |region| f32::from(region.size.width));
        let (forward, leaving) = (run.forward, look.leaving);
        let old = run.from.clone().map(|picture| {
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    window.paint_snapshot(&picture, bounds, leaving(t, width, forward));
                },
            )
            .absolute()
            .inset_0()
        });
        let new = div().size_full().flex().child(page);
        let new = match &run.into {
            Some(into) => {
                snapshot(new, into, Some((look.arriving)(t, width, forward))).into_any_element()
            }
            None => new.opacity(t).into_any_element(),
        };
        // A page arriving from beside the region is clipped to it.
        frame
            .overflow_hidden()
            .child(new)
            .children(old)
            .child(note)
            .child(
                div()
                    .id(look.hold)
                    .absolute()
                    .inset_0()
                    .occlude()
                    .test_support(),
            )
            .into_any_element()
    }
}

/// Linear progress of a run started at `started`, from 0 to 1.
fn progress(started: Instant, now: Instant, look: &CrossLook) -> f32 {
    (elapsed_since(started, now).as_secs_f32() / look.duration.as_secs_f32()).min(1.)
}
