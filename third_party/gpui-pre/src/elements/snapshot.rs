// Added by the Quill project (2026). See third_party/gpui-pre/QUILL-CHANGES.md.
//! QUILL: the `snapshot` element (docs/decisions/snapshot-transitions.md).

use crate::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, Hsla, InspectorElementId,
    IntoElement, LayoutId, Pixels, Snapshot, SnapshotDraw, Window,
};

/// Draws `child` through `snapshot` while `draw` is `Some`: the child is laid
/// out, prepainted and painted as always, so its hitboxes, listeners and
/// focus are real, but its pixels go into the snapshot (drawn again only
/// when something in it was painted afresh rather than replayed from the
/// last frame, so a child that is a cached view costs one quad a frame),
/// and the snapshot is drawn as `draw` says. With `None`, or where the
/// window has no snapshots, the child paints live.
pub fn snapshot(child: impl IntoElement, snapshot: &Snapshot, draw: Option<SnapshotDraw>) -> SnapshotElement {
    SnapshotElement {
        child: Some(child.into_any_element()),
        snapshot: snapshot.share(),
        draw,
        backing: None,
        margin: Pixels::ZERO,
    }
}

/// An element that draws its child through a [`Snapshot`]; see [`snapshot`].
pub struct SnapshotElement {
    child: Option<AnyElement>,
    snapshot: Snapshot,
    draw: Option<SnapshotDraw>,
    backing: Option<Hsla>,
    margin: Pixels,
}

impl SnapshotElement {
    /// Clears the snapshot to an opaque colour before the child is drawn
    /// into it, which keeps subpixel text correct.
    pub fn backing(mut self, color: Hsla) -> Self {
        self.backing = Some(color);
        self
    }

    /// Captures `margin` beyond the child's bounds on every side, for what
    /// it paints outside them (a shadow).
    pub fn margin(mut self, margin: Pixels) -> Self {
        self.margin = margin;
        self
    }
}

impl Element for SnapshotElement {
    type RequestLayoutState = Option<AnyElement>;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
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
        _id: Option<&GlobalElementId>,
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
        match self.draw {
            Some(draw) if window.snapshots_supported() => {
                let region = bounds.dilate(self.margin);
                window.capture_snapshot(&self.snapshot, region, self.backing, |window| {
                    child.paint(window, cx)
                });
                window.paint_snapshot(&self.snapshot, region, draw);
            }
            Some(_) | None => child.paint(window, cx),
        }
    }
}
