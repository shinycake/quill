//! The element that installs a `BoundedImageCache` over its child.

use super::{BoundedImageCache, SCOPED};
use gpui_kit::*;

/// Installs the bounded image cache over its child for every drawing
/// phase. GPUI's own `image_cache` element skips prepaint, where virtual
/// lists and cached views build their content.
pub(in crate::ui) struct CacheScope {
    cache: Entity<BoundedImageCache>,
    child: AnyElement,
}

impl CacheScope {
    pub(in crate::ui) fn new(cache: Entity<BoundedImageCache>, child: impl IntoElement) -> Self {
        Self {
            cache,
            child: child.into_any_element(),
        }
    }

    fn enter<R>(&self, window: &mut Window, f: impl FnOnce(&mut Window) -> R) -> R {
        SCOPED.with(|scoped| scoped.borrow_mut().push(self.cache.clone()));
        let result = window.with_image_cache(Some(self.cache.clone().into()), f);
        SCOPED.with(|scoped| scoped.borrow_mut().pop());
        result
    }
}

impl IntoElement for CacheScope {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for CacheScope {
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
        let layout_id = self.enter(window, |window| child.request_layout(window, cx));
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
        let mut child = std::mem::replace(&mut self.child, Empty.into_any_element());
        self.enter(window, |window| child.prepaint(window, cx));
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
        self.enter(window, |window| child.paint(window, cx));
        self.child = child;
    }
}
