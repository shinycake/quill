//! The elements a slice's content sits in.

use super::super::anim_layer::Layer;
use gpui_kit::*;
use std::cell::Cell;
use std::rc::Rc;

/// Marks a slice's subtree while it is laid out and painted, so animated
/// content built lazily there (virtual list rows) asks the frame clock on
/// behalf of the right slice, and reports to the slice's layer.
pub(super) struct SliceScope {
    pub(super) id: Option<EntityId>,
    pub(super) scope: Rc<Cell<Option<EntityId>>>,
    pub(super) layer: Option<Layer>,
    /// The layer whose frame this is: the slice's, or the media panel's,
    /// which only the panel's cells report to.
    pub(super) frames: Option<Layer>,
    pub(super) child: AnyElement,
}

impl SliceScope {
    fn enter<R>(&self, f: impl FnOnce() -> R) -> R {
        let outer = self.scope.replace(self.id);
        let result = super::super::anim_layer::with_layer(self.layer.as_ref(), f);
        self.scope.set(outer);
        result
    }
}

impl IntoElement for SliceScope {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SliceScope {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut child = std::mem::replace(&mut self.child, Empty.into_any_element());
        let layout_id = self.enter(|| child.request_layout(window, cx));
        self.child = child;
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        // The slice really draws: its layer gets this frame's content.
        if let Some(layer) = &self.frames {
            layer.begin_frame();
        }
        let mut child = std::mem::replace(&mut self.child, Empty.into_any_element());
        self.enter(|| child.prepaint(window, cx));
        self.child = child;
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let mut child = std::mem::replace(&mut self.child, Empty.into_any_element());
        self.enter(|| child.paint(window, cx));
        self.child = child;
        if let Some(layer) = &self.frames {
            layer.end_frame();
        }
    }
}

/// Records its child's laid-out height; when it changed, the slot that
/// sizes the composer slice is redrawn (`redraw`: `AppOnly`, or the chat
/// window).
pub(super) struct MeasureHeight {
    pub(super) height: Rc<Cell<Pixels>>,
    pub(super) redraw: Option<EntityId>,
    pub(super) child: AnyElement,
}

impl IntoElement for MeasureHeight {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for MeasureHeight {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let height = bounds.size.height.ceil();
        if self.height.replace(height) != height
            && let Some(id) = self.redraw
        {
            cx.notify(id);
        }
        self.child.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
    }
}
