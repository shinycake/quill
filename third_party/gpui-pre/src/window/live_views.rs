//! Quill: frames that redraw only [`crate::Entity::live`] views.
//!
//! A notified view dirties its ancestors, so a view that only animates
//! renders the whole window around it. A live view tells GPUI where it sits
//! in the frame instead: the entry below keeps what its place in the tree
//! gave it, and the paint operations it added to the scene. When only live
//! views were notified, [`Window::patch_live_views`] renders those alone and
//! builds the frame's scene from the last whole frame's, with their
//! operations swapped for new ones.
//!
//! `rendered_frame` stays the last whole frame, scene included: hit tests,
//! the key dispatch tree, the accessibility tree and the ranges cached views
//! replay from all read it, and what a live view builds here reaches none of
//! them. The next frame, patched or whole, starts over from it.

use super::{
    AnyImageCache, AnyView, App, Bounds, ContentMask, DrawPhase, ElementIdStack, EntityId, Pixels,
    Range, Scene, SmallVec, TextStyleRefinement, Window,
};
use crate::{AnyElement, IntoElement as _};

pub(crate) struct LiveView {
    view: AnyView,
    /// Its own id included, which comes back when it prepaints in a patch.
    id_stack: ElementIdStack,
    /// Inside another live view, which draws it again in a patch.
    nested: bool,
    text_style_stack: Vec<TextStyleRefinement>,
    image_cache_stack: Vec<AnyImageCache>,
    content_mask: ContentMask<Pixels>,
    opacity: f32,
    rem_size: Pixels,
    bounds: Bounds<Pixels>,
    /// Its operations in the frame's scene, once painted: a prepaint can be
    /// discarded.
    scene: Option<Range<usize>>,
}

impl LiveView {
    fn is(&self, view: EntityId) -> bool {
        self.scene.is_some() && self.view.entity_id() == view
    }
}

impl Window {
    /// A live view prepaints at `bounds`. Returns its entry in the frame.
    pub(crate) fn note_live_view(&mut self, view: AnyView, bounds: Bounds<Pixels>) -> usize {
        let id_stack = self.element_id_stack.clone();
        let nested = self
            .next_frame
            .live_views
            .iter()
            .any(|outer| id_stack.starts_with(&outer.id_stack[..]));
        self.next_frame.live_views.push(LiveView {
            view,
            id_stack,
            nested,
            text_style_stack: self.text_style_stack.clone(),
            image_cache_stack: self.image_cache_stack.clone(),
            content_mask: self.content_mask(),
            opacity: self.element_opacity,
            rem_size: self.rem_size(),
            bounds,
            scene: None,
        });
        self.next_frame.live_views.len() - 1
    }

    /// The live view `site` finished painting, from `start` in the scene.
    pub(crate) fn live_view_painted(&mut self, site: usize, start: usize) {
        let end = self.next_frame.scene.len();
        if let Some(site) = self.next_frame.live_views.get_mut(site) {
            site.scene = Some(start..end);
        }
    }

    /// The scene this window last drew.
    pub(crate) fn shown_scene(&self) -> &Scene {
        if self.scene_patched {
            &self.next_frame.scene
        } else {
            &self.rendered_frame.scene
        }
    }

    /// Whether this frame can be a patch: nothing but live views of the last
    /// whole frame changed, and nothing asks for a frame drawn in full.
    pub(super) fn only_live_views_changed(&self, cx: &App) -> bool {
        #[cfg(feature = "profiler")]
        if self.debug_frame_overlay.mode() != crate::DebugFrameOverlayMode::Hidden {
            return false;
        }
        #[cfg(any(feature = "inspector", debug_assertions))]
        if self.inspector.is_some() {
            return false;
        }
        if self.refreshing
            || self.paint_filter.is_some()
            || cx.mode.skip_drawing()
            || !self.a11y.tree_is_current()
        {
            return false;
        }
        let sites = &self.rendered_frame.live_views;
        self.invalidator
            .all_views(|view| sites.iter().any(|site| site.is(view)))
    }

    /// A whole frame follows patches, which drew every live view of the last
    /// whole frame again. A cached view holding one would replay its older
    /// paint, so each renders again, and its ancestors with it.
    pub(super) fn dirty_patched_live_views(&mut self) {
        let views: SmallVec<[EntityId; 4]> = self
            .rendered_frame
            .live_views
            .iter()
            .map(|site| site.view.entity_id())
            .collect();
        for view in views {
            self.mark_view_dirty(view);
        }
    }

    /// Draw this frame by rendering every live view again, the ones not
    /// notified too (the scene they were painted into may be older than
    /// what they last showed).
    pub(super) fn patch_live_views(&mut self, cx: &mut App) {
        cx.entities.clear_accessed();
        self.invalidator.clear_views();
        self.invalidator.set_dirty(false);
        self.requested_autoscroll = None;
        self.invalidator.set_phase(DrawPhase::Prepaint);
        let tree = self.a11y.pause_tree();
        let layouts = self.text_system.layout_index();

        let mut sites = std::mem::take(&mut self.rendered_frame.live_views);
        sites.retain(|site| !site.nested && site.scene.is_some());
        sites.sort_by_key(|site| site.scene.as_ref().map(|scene| scene.start));

        let mut elements: SmallVec<[AnyElement; 4]> = SmallVec::new();
        for site in &sites {
            let mut element = site.view.clone().into_any_element();
            self.in_live_view(site, |window| {
                element.layout_as_root(site.bounds.size.into(), window, cx);
                element.prepaint_at(site.bounds.origin, window, cx);
            });
            elements.push(element);
        }

        self.invalidator.set_phase(DrawPhase::Paint);
        let mut replayed = 0;
        for (site, element) in sites.iter().zip(&mut elements) {
            let Some(scene) = site.scene.clone() else {
                continue;
            };
            self.next_frame
                .scene
                .replay(replayed..scene.start, &self.rendered_frame.scene);
            self.in_live_view(site, |window| element.paint(window, cx));
            replayed = scene.end;
        }
        let end = self.rendered_frame.scene.len();
        self.next_frame
            .scene
            .replay(replayed..end, &self.rendered_frame.scene);
        self.next_frame.scene.finish();
        self.scene_patched = true;
        drop(elements);

        self.element_id_stack.clear();
        self.text_style_stack.clear();
        self.image_cache_stack.clear();
        self.element_opacity = 1.0;
        self.rendered_frame.live_views = sites;
        let states = std::mem::take(&mut self.next_frame.element_states);
        self.rendered_frame.element_states.extend(states);
        self.next_frame.clear_all_but_scene();
        if let Some(layout) = self.layout_engine.as_mut() {
            layout.clear();
        }
        self.text_system.truncate_layouts(layouts);
        self.a11y.resume_tree(tree);
        let entities = std::mem::take(cx.entities.accessed_entities.get_mut());
        cx.add_entities_accessed(self.handle, self.invalidator.clone(), &entities);
        self.invalidator.set_phase(DrawPhase::None);
        self.needs_present.set(true);
    }

    fn in_live_view(&mut self, site: &LiveView, f: impl FnOnce(&mut Self)) {
        self.element_id_stack.clone_from(&site.id_stack);
        self.element_id_stack.pop();
        self.text_style_stack.clone_from(&site.text_style_stack);
        self.image_cache_stack.clone_from(&site.image_cache_stack);
        self.element_opacity = site.opacity;
        self.with_rem_size(Some(site.rem_size), |window| {
            window.with_content_mask(Some(site.content_mask), f)
        });
    }
}
