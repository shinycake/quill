//! Cached child views of the one big `QuillApp` view ("slices").
//!
//! `QuillApp` renders as a single GPUI view, so any `cx.notify()` used to
//! lay out and paint the whole window again — including the frame clock's
//! 30 fps tick for one animated custom emoji. A slice is a tiny entity
//! whose render borrows `QuillApp` and draws one part of it (the chat list,
//! the conversation); `QuillApp` embeds it with `Entity::cached`, so GPUI
//! replays the slice's previous frame unless the slice itself (or something
//! rendered inside it) was notified.
//!
//! Invalidation contract:
//! - Every `QuillApp` notify notifies every slice (`observe_self`), so all
//!   existing `cx.notify()` call sites keep refreshing everything.
//! - Animated content asks the frame clock for a tick from inside a slice;
//!   the tick notifies only that slice (`SliceScope` records which one is
//!   being laid out or painted), and the other slices replay.
//! - Views rendered inside a slice (inputs, images loading, hover state)
//!   dirty the slice through GPUI's ancestor walk; `window.refresh()`
//!   bypasses every cache.
//!
//! GPUI re-renders every ancestor of a dirty view, and a re-rendered cached
//! view re-renders its whole subtree, so a slice per chat row would not
//! help: one animated emoji would still rebuild the list. Animated custom
//! emoji in chat-list previews are therefore painted by the sidebar's
//! animation layer instead ([`LayeredFrames`]): the row only reports where
//! the emoji sits, and a tick redraws the layer while the chat list
//! replays its cached frame.

use super::app::QuillApp;
use gpui_kit::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Which part of `QuillApp` a slice draws.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum SliceKind {
    /// The chat list column (ready mode).
    Sidebar,
    /// Header, history and composer of the open chat.
    Conversation,
}

pub(super) struct AppSlice {
    app: WeakEntity<QuillApp>,
    kind: SliceKind,
}

/// `QuillApp`'s slices and the frame-clock bookkeeping that targets them.
#[derive(Default)]
pub(super) struct Slices {
    sidebar: Option<Entity<AppSlice>>,
    conversation: Option<Entity<AppSlice>>,
    /// Paints the chat list's animated emoji over its cached frame.
    layer: Option<Entity<AnimationLayer>>,
    /// The layer's items, reported by the chat list's last real paint.
    layer_items: LayerItems,
    /// The window's bounded path-image cache (`image_budget`).
    image_cache: Option<Entity<super::image_budget::BoundedImageCache>>,
    /// The slice whose content is being rendered, laid out or painted
    /// (`None`: `QuillApp` itself).
    scope: Rc<Cell<Option<EntityId>>>,
    /// The conversation slice rendered since the last `QuillApp` render.
    pub(super) conversation_rendered: Cell<bool>,
    /// The conversation slot was part of the last `QuillApp` render.
    pub(super) conversation_shown: Cell<bool>,
}

type LayerItems = Rc<RefCell<Vec<LayerItem>>>;

impl Slices {
    /// The slice whose content is being drawn now, if any.
    pub(super) fn current(&self) -> Option<EntityId> {
        self.scope.get()
    }

    /// Whether content being built now belongs to the chat list, whose
    /// animated emoji the layer paints.
    pub(super) fn in_sidebar(&self) -> bool {
        self.scope.get().is_some()
            && self.scope.get() == self.sidebar.as_ref().map(Entity::entity_id)
    }

    pub(super) fn image_cache(&self) -> Option<Entity<super::image_budget::BoundedImageCache>> {
        self.image_cache.clone()
    }

    /// The frame-clock target that redraws the layer only.
    pub(super) fn layer_target(&self) -> Option<EntityId> {
        self.layer.as_ref().map(Entity::entity_id)
    }

    /// An element that reports `clip` to the layer when painted.
    pub(super) fn layered(&self, clip: LayeredClip) -> LayeredFrames {
        LayeredFrames {
            clip,
            items: self.layer_items.clone(),
            style: StyleRefinement::default(),
        }
    }

    /// The fastest frame rate the layer's items play at (0: none animate).
    fn layer_fps(&self) -> u32 {
        self.layer_items
            .borrow()
            .iter()
            .filter(|item| item.clip.animates())
            .map(|item| item.clip.fps.ceil() as u32)
            .max()
            .unwrap_or(0)
    }
}

impl QuillApp {
    /// Create the long-lived slices; every `QuillApp` notify re-renders
    /// them all.
    pub(super) fn init_slices(&mut self, cx: &mut Context<Self>) {
        let app = cx.entity().downgrade();
        self.slices.sidebar = Some(cx.new(|_| AppSlice {
            app: app.clone(),
            kind: SliceKind::Sidebar,
        }));
        self.slices.conversation = Some(cx.new(|_| AppSlice {
            app,
            kind: SliceKind::Conversation,
        }));
        let items = self.slices.layer_items.clone();
        self.slices.layer = Some(cx.new(|_| AnimationLayer { items }));
        let scope = self.slices.scope.clone();
        self.slices.image_cache =
            Some(cx.new(|_| super::image_budget::BoundedImageCache::new(scope)));
        cx.observe_self(|this, cx| this.notify_slices(cx)).detach();
    }

    fn notify_slices(&self, cx: &mut Context<Self>) {
        super::frame_clock::trace_slice_render("(app notified)");
        super::frame_clock::trace_notify_origin();
        let ids: Vec<EntityId> = self
            .slices
            .sidebar
            .iter()
            .chain(&self.slices.conversation)
            .map(Entity::entity_id)
            .collect();
        let app: &mut App = cx;
        for id in ids {
            app.notify(id);
        }
    }

    /// Redraw only the conversation (state that nothing else shows
    /// changed); the whole app before the slices exist.
    pub(super) fn notify_conversation(&self, cx: &mut Context<Self>) {
        match &self.slices.conversation {
            Some(slice) => {
                let id = slice.entity_id();
                let app: &mut App = cx;
                app.notify(id);
            }
            None => cx.notify(),
        }
    }

    /// Keep the layer's emoji playing while the chat list replays.
    pub(super) fn tick_animation_layer(&self, cx: &mut Context<Self>) {
        let fps = self.slices.layer_fps();
        if fps > 0 && self.window_active.get() {
            self.request_animation_tick_for(self.slices.layer_target(), fps.min(30), cx);
        }
    }

    /// The chat list, as a cached slice filling a `sidebar_width` column,
    /// with the layer painting its animated emoji just above it.
    pub(super) fn sidebar_slot(&self) -> AnyElement {
        let column = div().relative().w(self.sidebar_width).flex_none().h_full();
        match (self.slices.sidebar.clone(), self.slices.layer.clone()) {
            (Some(slice), Some(layer)) => {
                super::image_budget::slice_shown(slice.entity_id());
                column
                    .child(slice.cached(full()))
                    .child(layer)
                    .into_any_element()
            }
            _ => column.into_any_element(),
        }
    }

    /// The conversation, as a cached slice filling the remaining width.
    pub(super) fn conversation_slot(&self) -> AnyElement {
        self.slices.conversation_shown.set(true);
        let slot = div().flex().flex_1().min_w_0().min_h_0();
        match self.slices.conversation.clone() {
            Some(slice) => {
                super::image_budget::slice_shown(slice.entity_id());
                slot.child(slice.cached(full())).into_any_element()
            }
            None => slot.into_any_element(),
        }
    }

    fn render_slice(&mut self, kind: SliceKind, cx: &mut Context<Self>) -> AnyElement {
        match kind {
            SliceKind::Sidebar => {
                let auth_state = self.current_auth();
                let auth = quill::auth::view_for(&auth_state);
                self.sidebar(&auth, false, false, false, false, cx)
            }
            SliceKind::Conversation => {
                self.slices.conversation_rendered.set(true);
                div()
                    .size_full()
                    .flex()
                    .child(self.conversation(cx))
                    .into_any_element()
            }
        }
    }
}

fn full() -> StyleRefinement {
    StyleRefinement::default().size_full()
}

impl Render for AppSlice {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let id = cx.entity_id();
        let kind = self.kind;
        super::frame_clock::trace_slice_render(match kind {
            SliceKind::Sidebar => "sidebar",
            SliceKind::Conversation => "conversation",
        });
        let Some(app) = self.app.upgrade() else {
            return Empty.into_any_element();
        };
        super::image_budget::slice_rendered(id);
        app.update(cx, |app, cx| {
            let scope = app.slices.scope.clone();
            let outer = scope.replace(Some(id));
            let child = app.render_slice(kind, cx);
            scope.set(outer);
            // The chat list's real paint reports the layer's items afresh.
            let layer = (kind == SliceKind::Sidebar).then(|| app.slices.layer_items.clone());
            SliceScope {
                id,
                scope,
                layer,
                child,
            }
            .into_any_element()
        })
    }
}

/// Marks a slice's subtree while it is laid out and painted, so animated
/// content built lazily there (virtual list rows) asks the frame clock on
/// behalf of the right slice.
struct SliceScope {
    id: EntityId,
    scope: Rc<Cell<Option<EntityId>>>,
    layer: Option<LayerItems>,
    child: AnyElement,
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
        let outer = self.scope.replace(Some(self.id));
        let layout_id = self.child.request_layout(window, cx);
        self.scope.set(outer);
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
        let outer = self.scope.replace(Some(self.id));
        self.child.prepaint(window, cx);
        self.scope.set(outer);
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
        if let Some(layer) = &self.layer {
            layer.borrow_mut().clear();
        }
        let outer = self.scope.replace(Some(self.id));
        self.child.paint(window, cx);
        self.scope.set(outer);
    }
}

/// A decoded animation and when it started, so the frame to show can be
/// picked at paint time.
#[derive(Clone)]
pub(super) struct LayeredClip {
    pub(super) frames: Arc<[Arc<RenderImage>]>,
    pub(super) fps: f64,
    pub(super) started: Instant,
    pub(super) duration: Duration,
    pub(super) looping: bool,
}

impl LayeredClip {
    fn animates(&self) -> bool {
        self.frames.len() > 1 && (self.looping || self.started.elapsed() < self.duration)
    }

    fn frame_now(&self) -> Option<&Arc<RenderImage>> {
        let count = self.frames.len();
        let elapsed = self.started.elapsed();
        let index = if !self.looping && elapsed >= self.duration {
            count.saturating_sub(1)
        } else {
            (elapsed.as_secs_f64() * self.fps) as usize % count.max(1)
        };
        self.frames.get(index)
    }
}

/// Where a chat-list emoji was painted, for the layer to draw into.
struct LayerItem {
    clip: LayeredClip,
    bounds: Bounds<Pixels>,
    mask: ContentMask<Pixels>,
}

/// An animated custom emoji in the chat list: it takes its place in the
/// layout and reports its bounds; [`AnimationLayer`] paints the frames.
#[derive(Clone)]
pub(super) struct LayeredFrames {
    clip: LayeredClip,
    items: LayerItems,
    style: StyleRefinement,
}

impl Styled for LayeredFrames {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl IntoElement for LayeredFrames {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for LayeredFrames {
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
        let mut style = Style::default();
        style.refine(&self.style);
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Window,
        _: &mut App,
    ) -> Self::PrepaintState {
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        _: &mut App,
    ) {
        self.items.borrow_mut().push(LayerItem {
            clip: self.clip.clone(),
            bounds,
            mask: window.content_mask(),
        });
    }
}

/// Paints the chat list's animated emoji at the bounds its last real paint
/// reported, choosing each frame by the time of this paint. Rendered right
/// after the chat list, so everything painted later (menus, dialogs, drag
/// previews) still covers it.
pub(super) struct AnimationLayer {
    items: LayerItems,
}

impl Render for AnimationLayer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        LayerPainter {
            items: self.items.clone(),
        }
    }
}

struct LayerPainter {
    items: LayerItems,
}

impl IntoElement for LayerPainter {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for LayerPainter {
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
        // Out of the flow: it paints at the bounds the chat list reported.
        let mut style = Style::default();
        style.refine(&StyleRefinement::default().absolute().size_0());
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Window,
        _: &mut App,
    ) -> Self::PrepaintState {
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        _: &mut App,
    ) {
        for item in self.items.borrow().iter() {
            let Some(frame) = item.clip.frame_now().cloned() else {
                continue;
            };
            window.with_content_mask(Some(item.mask), |window| {
                let _ = window.paint_image(
                    item.bounds,
                    item.bounds,
                    Corners::default(),
                    frame,
                    0,
                    false,
                );
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LayeredClip;
    use gpui_kit::RenderImage;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    fn clip(looping: bool, elapsed_ms: u64) -> LayeredClip {
        let frames: Vec<Arc<RenderImage>> = (0..4)
            .map(|_| {
                Arc::new(RenderImage::new(smallvec::SmallVec::from_buf([
                    image::Frame::new(image::RgbaImage::new(1, 1)),
                ])))
            })
            .collect();
        LayeredClip {
            frames: frames.into(),
            fps: 10.,
            started: Instant::now() - Duration::from_millis(elapsed_ms),
            duration: Duration::from_millis(400),
            looping,
        }
    }

    fn index(clip: &LayeredClip) -> usize {
        let frame = clip.frame_now().expect("a frame");
        clip.frames
            .iter()
            .position(|f| Arc::ptr_eq(f, frame))
            .expect("one of the clip's frames")
    }

    #[test]
    fn frames_follow_the_clock() {
        assert_eq!(index(&clip(true, 250)), 2);
        // Looping wraps around.
        assert_eq!(index(&clip(true, 650)), 2);
        assert!(clip(true, 650).animates());
    }

    #[test]
    fn a_finished_one_shot_holds_its_last_frame() {
        let done = clip(false, 900);
        assert_eq!(index(&done), 3);
        assert!(!done.animates());
    }
}
