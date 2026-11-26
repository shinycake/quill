// Added by the Quill project (2026). See third_party/gpui-pre/QUILL-CHANGES.md.
//! QUILL: `with_snapshot_animation` (docs/decisions/snapshot-transitions.md).
//!
//! Draws an element through a snapshot for one animation: from its first
//! frame the subtree paints into a snapshot the element's state owns, drawn
//! as the animator says for the time so far; when the animation ends the
//! state drops the snapshot and the subtree paints live. The state, and the
//! snapshot with it, also goes when the element is no longer rendered.

use scheduler::Instant;

use crate::{
    Animation, AnyElement, App, Bounds, Element, ElementId, GlobalElementId, Hsla,
    InspectorElementId, IntoElement, LayoutId, Pixels, Snapshot, SnapshotBacking, SnapshotDraw,
    Window,
};

/// An extension trait for drawing an element through a snapshot while an
/// animation runs.
pub trait SnapshotAnimationExt: IntoElement + Sized {
    /// Draws this element through a snapshot for `animation`: `animator`
    /// maps the eased time and the element's bounds to how the snapshot is
    /// drawn. It should give [`SnapshotDraw::IDENTITY`] at the end, where
    /// the live element takes over. Where the window has no snapshots, or
    /// motion is reduced, the run ends at once and the element paints live;
    /// the owner chooses its own fallback for that case.
    fn with_snapshot_animation(
        self,
        id: impl Into<ElementId>,
        animation: Animation,
        animator: impl Fn(f32, Bounds<Pixels>) -> SnapshotDraw + 'static,
    ) -> SnapshotAnimationElement {
        SnapshotAnimationElement {
            id: id.into(),
            child: Some(self.into_any_element()),
            animation,
            animator: Box::new(animator),
            backing: None,
            margin: Pixels::ZERO,
        }
    }
}

impl<E: IntoElement> SnapshotAnimationExt for E {}

/// An element drawn through a snapshot while an animation runs; see
/// [`SnapshotAnimationExt::with_snapshot_animation`].
pub struct SnapshotAnimationElement {
    id: ElementId,
    child: Option<AnyElement>,
    animation: Animation,
    animator: Box<dyn Fn(f32, Bounds<Pixels>) -> SnapshotDraw>,
    backing: Option<SnapshotBacking>,
    margin: Pixels,
}

impl SnapshotAnimationElement {
    /// Paints `color` under the bounds, rounded by `radius`, in the
    /// snapshot; see [`crate::SnapshotElement::backing`].
    pub fn backing(mut self, color: Hsla, radius: Pixels) -> Self {
        self.backing = Some(SnapshotBacking { color, radius });
        self
    }

    /// Captures `margin` beyond the bounds on every side, for a shadow.
    pub fn margin(mut self, margin: Pixels) -> Self {
        self.margin = margin;
        self
    }
}

/// Where the run is, kept in the element's state.
enum SnapshotRun {
    /// Drawing through `snapshot` since `started`.
    Running { started: Instant, snapshot: Snapshot },
    /// The animation ended: no snapshot, the element paints live.
    Ended,
}

impl Element for SnapshotAnimationElement {
    type RequestLayoutState = Option<AnyElement>;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let Some(mut child) = self.child.take() else {
            return (window.request_layout(crate::Style::default(), [], cx), None);
        };
        (child.request_layout(window, cx), Some(child))
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        child: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(child) = child {
            child.prepaint(window, cx);
        }
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        child: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(child) = child else {
            return;
        };
        let Some(id) = id else {
            child.paint(window, cx);
            return;
        };
        window.with_element_state::<SnapshotRun, _>(id, |run, window| {
            let now = cx.background_executor().now();
            let run = run.unwrap_or_else(|| SnapshotRun::Running {
                started: now,
                snapshot: window.new_snapshot(),
            });
            let (started, snapshot) = match run {
                SnapshotRun::Running { started, snapshot } => (started, snapshot),
                SnapshotRun::Ended => {
                    child.paint(window, cx);
                    return ((), SnapshotRun::Ended);
                }
            };
            let duration = self.animation.duration.as_secs_f32();
            let mut delta = if duration > 0.0 {
                now.duration_since(started).as_secs_f32() / duration
            } else {
                1.0
            };
            if delta >= 1.0 && !self.animation.oneshot {
                delta %= 1.0;
            }
            if cx.reduce_motion() || delta >= 1.0 || !window.snapshots_supported() {
                // Ended, or nothing to draw through (the owner's fallback
                // covers that): live, and the snapshot goes with the run.
                child.paint(window, cx);
                return ((), SnapshotRun::Ended);
            }
            let draw = (self.animator)((self.animation.easing)(delta), bounds);
            let region = bounds.dilate(self.margin);
            let backing = self.backing;
            window.capture_snapshot(&snapshot, region, None, |window| {
                if let Some(backing) = backing {
                    backing.paint(bounds, window);
                }
                child.paint(window, cx)
            });
            window.paint_snapshot(&snapshot, region, draw);
            window.request_animation_frame();
            ((), SnapshotRun::Running { started, snapshot })
        })
    }
}
