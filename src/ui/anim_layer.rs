//! Animation layers: animated content drawn over a cached slice's frame.
//!
//! GPUI re-renders every ancestor of a notified view, and a re-rendered
//! cached view re-renders its whole subtree (`view.rs`, `window.rs`
//! `mark_view_dirty`). So anything animating inside the chat list or the
//! conversation would rebuild the whole slice on every frame. Instead, the
//! slice only reports where its animated content sits, and a small layer
//! view rendered right after the slice (its sibling) draws that content
//! at the current time. A frame-clock tick notifies only the layer; the
//! slice replays its cached frame (`app_slice`).
//!
//! How it fits GPUI's frame (`Window::draw_roots`): every element of the
//! window is prepainted, then everything is painted, in tree order.
//!
//! - While a slice really renders, [`Layered`] elements register their
//!   bounds and content mask in the slice's [`Layer`] during prepaint
//!   (bounds are final there), and [`Occluder`]s register the bounds of
//!   whatever floats over the slice's content (popovers, jump buttons, the
//!   date pill).
//! - The layer prepaints after the slice: it decides which items an
//!   occluder covers (those are painted by the slice itself, "inline", and
//!   tick the slice as before) and builds the elements it redraws.
//! - The slice paints; each item confirms it was painted (prepaint can be
//!   retried and discarded, paint can't) and paints itself when inline.
//! - The layer paints the confirmed items in paint order, then asks the
//!   frame clock for the next tick (`QuillApp::tick_animation_layer`).
//!
//! When the slice replays, none of that runs: the items of its last real
//! frame stay, which is exactly what the replay shows.
//!
//! The layer paints after the whole slice, so something the slice paints
//! over an item must be an [`Occluder`] (the item then falls back to the
//! slice) or a [`mirror`] (redrawn by the layer above the item). Anything
//! painted after the slice (menus, dialogs, the media viewer, tooltips,
//! deferred popovers) covers the layer as it covered the slice.

use super::app::QuillApp;
use gpui_kit::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

thread_local! {
    /// The layer of the slice being rendered, laid out or painted.
    static CURRENT: RefCell<Option<Layer>> = const { RefCell::new(None) };
    /// Paint order of layered items (see [`Item::seq`]).
    static PAINT_SEQ: Cell<u64> = const { Cell::new(0) };
}

/// The layer that content built now belongs to, if any: only content of a
/// slice with a layer is drawn by one.
pub(super) fn current() -> Option<Layer> {
    CURRENT.with(|current| current.borrow().clone())
}

/// Run `f` with `layer` as the current one.
pub(super) fn with_layer<R>(layer: Option<&Layer>, f: impl FnOnce() -> R) -> R {
    let outer = CURRENT.with(|current| current.replace(layer.cloned()));
    let result = f();
    CURRENT.with(|current| *current.borrow_mut() = outer);
    result
}

fn next_seq() -> u64 {
    PAINT_SEQ.with(|seq| {
        seq.set(seq.get() + 1);
        seq.get()
    })
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
    /// When the clip was last drawn, for its cache's eviction: a clip the
    /// layer draws is on screen although its row doesn't render.
    pub(super) shown: Option<Rc<Cell<Instant>>>,
}

impl LayeredClip {
    pub(super) fn animates(&self) -> bool {
        self.frames.len() > 1 && (self.looping || self.started.elapsed() < self.duration)
    }

    pub(super) fn frame_now(&self) -> Option<&Arc<RenderImage>> {
        let count = self.frames.len();
        let elapsed = self.started.elapsed();
        let index = if !self.looping && elapsed >= self.duration {
            count.saturating_sub(1)
        } else {
            (elapsed.as_secs_f64() * self.fps) as usize % count.max(1)
        };
        self.frames.get(index)
    }

    fn paint(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        if let Some(shown) = &self.shown {
            shown.set(Instant::now());
        }
        if let Some(frame) = self.frame_now().cloned() {
            let _ = window.paint_image(bounds, bounds, Corners::default(), frame, 0, false);
        }
    }
}

/// Paints time-driven content into the given bounds.
pub(super) type PaintFn = Rc<dyn Fn(Bounds<Pixels>, &mut Window)>;
/// Builds the element the layer draws, afresh each frame.
pub(super) type BuildFn = Rc<dyn Fn() -> Option<AnyElement>>;

/// What a layered item draws.
#[derive(Clone)]
pub(super) enum Content {
    /// An animated sticker or custom emoji, at most `max_fps`.
    Frames { clip: LayeredClip, max_fps: u32 },
    /// Clock-driven drawing (spoiler specks, typing dots) at `fps`.
    Paint { paint: PaintFn, fps: u32 },
    /// A live tile (an inline video and the badges over it), rebuilt from
    /// the player's current frame; the row draws the same tile from the
    /// frame it rendered with, underneath.
    Tile { build: BuildFn, fps: u32 },
    /// A copy of something the row paints over layered content (the time
    /// pill on a video), redrawn above it. Static.
    Mirror { build: BuildFn },
}

impl Content {
    fn fps(&self) -> u32 {
        match self {
            Content::Frames { clip, max_fps } => {
                if clip.animates() {
                    (clip.fps.ceil() as u32).clamp(1, *max_fps)
                } else {
                    0
                }
            }
            Content::Paint { fps, .. } | Content::Tile { fps, .. } => *fps,
            Content::Mirror { .. } => 0,
        }
    }

    fn is_mirror(&self) -> bool {
        matches!(self, Content::Mirror { .. })
    }

    /// Draw it where the slice paints it (an occluder covers it, or no
    /// layer draws it). Tiles and mirrors are the row's own children.
    fn paint_inline(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        match self {
            Content::Frames { clip, .. } => clip.paint(bounds, window),
            Content::Paint { paint, .. } => paint(bounds, window),
            Content::Tile { .. } | Content::Mirror { .. } => {}
        }
    }
}

struct Item {
    content: Content,
    bounds: Bounds<Pixels>,
    mask: ContentMask<Pixels>,
    /// The text style where the slice lays it out, for the elements the
    /// layer builds (tiles, mirrors) to lay out the same.
    text: Option<TextStyleRefinement>,
    /// The slice draws it: something it paints later covers the item, or
    /// (a mirror) nothing the layer draws is under it. A mirror the layer
    /// draws isn't painted by the slice at all, so a translucent pill
    /// doesn't darken twice.
    inline: Cell<bool>,
    /// Paint order once painted (0: registered by a prepaint that was
    /// discarded, or not painted yet).
    seq: Cell<u64>,
}

impl Item {
    /// The part of it that can show.
    fn visible(&self) -> Option<Bounds<Pixels>> {
        let visible = self.bounds.intersect(&self.mask.bounds);
        (visible.size.width > Pixels::ZERO && visible.size.height > Pixels::ZERO).then_some(visible)
    }
}

#[derive(Default)]
struct LayerState {
    items: RefCell<Vec<Item>>,
    occluders: RefCell<Vec<Bounds<Pixels>>>,
    /// The slice painted since it last prepainted: the items are final.
    painted: Cell<bool>,
}

/// A slice's animated content and what covers it, as of the slice's last
/// real frame.
#[derive(Clone, Default)]
pub(super) struct Layer(Rc<LayerState>);

impl Layer {
    /// The slice prepaints a new frame: forget the last one's content.
    pub(super) fn begin_frame(&self) {
        self.0.items.borrow_mut().clear();
        self.0.occluders.borrow_mut().clear();
        self.0.painted.set(false);
    }

    /// The slice painted its frame (`SliceScope`).
    pub(super) fn end_frame(&self) {
        self.0.painted.set(true);
    }

    fn register(
        &self,
        content: Content,
        bounds: Bounds<Pixels>,
        mask: ContentMask<Pixels>,
        text: Option<TextStyleRefinement>,
    ) -> usize {
        let mut items = self.0.items.borrow_mut();
        items.push(Item {
            content,
            bounds,
            mask,
            text,
            inline: Cell::new(false),
            seq: Cell::new(0),
        });
        items.len() - 1
    }

    fn occluded(&self, visible: Bounds<Pixels>) -> bool {
        self.0
            .occluders
            .borrow()
            .iter()
            .any(|occluder| occluder.intersects(&visible))
    }

    /// Register content the slice is painting now (its prepaint is over,
    /// so every occluder is known): drawn by the layer, or right here
    /// when covered.
    pub(super) fn paint_now(&self, content: Content, bounds: Bounds<Pixels>, window: &mut Window) {
        let mask = window.content_mask();
        let visible = bounds.intersect(&mask.bounds);
        if visible.size.width <= Pixels::ZERO || visible.size.height <= Pixels::ZERO {
            return;
        }
        let inline = self.occluded(visible);
        if inline {
            content.paint_inline(bounds, window);
        }
        let index = self.register(content, bounds, mask, None);
        let items = self.0.items.borrow();
        items[index].inline.set(inline);
        items[index].seq.set(next_seq());
    }

    /// The frame rates its painted content wants: drawn by the layer, and
    /// drawn inline by the slice.
    pub(super) fn demand(&self) -> (u32, u32) {
        let (mut layer, mut inline) = (0, 0);
        for item in self.0.items.borrow().iter() {
            if item.seq.get() == 0 || item.visible().is_none() {
                continue;
            }
            let fps = item.content.fps();
            if item.inline.get() {
                inline = inline.max(fps);
            } else {
                layer = layer.max(fps);
            }
        }
        (layer, inline)
    }

    /// Before the slice paints a new frame (the layer's prepaint): decide
    /// which items the slice draws (`inline`), and say which ones need an
    /// element built (tiles and mirrors the layer draws).
    fn plan(&self) -> Vec<bool> {
        let items = self.0.items.borrow();
        // A replayed slice: what its last real frame painted stays decided,
        // and what it didn't paint (a discarded prepaint) never shows.
        let replay = self.0.painted.get();
        let mut drawn: Vec<Bounds<Pixels>> = Vec::new();
        let mut build = Vec::with_capacity(items.len());
        for item in items.iter() {
            let visible = item.visible().filter(|_| !replay || item.seq.get() != 0);
            let Some(visible) = visible else {
                build.push(false);
                continue;
            };
            let mirror = item.content.is_mirror();
            if !replay {
                item.inline.set(if mirror {
                    !drawn.iter().any(|r| r.intersects(&visible))
                } else {
                    self.occluded(visible)
                });
            }
            let layered = !item.inline.get();
            if layered && !mirror {
                drawn.push(visible);
            }
            build.push(
                layered && matches!(item.content, Content::Tile { .. } | Content::Mirror { .. }),
            );
        }
        build
    }

    /// The layer's prepaint: the elements it redraws, laid out and
    /// prepainted at their items' bounds.
    fn prepaint(&self, window: &mut Window, cx: &mut App) -> Vec<Option<AnyElement>> {
        let plan = self.plan();
        let items = self.0.items.borrow();
        items
            .iter()
            .zip(plan)
            .map(|(item, build)| {
                let element = match &item.content {
                    Content::Tile { build: element, .. } | Content::Mirror { build: element }
                        if build =>
                    {
                        element()
                    }
                    _ => None,
                };
                element.map(|mut element| {
                    window.with_text_style(item.text.clone(), |window| {
                        element.layout_as_root(item.bounds.size.into(), window, cx);
                        window.with_content_mask(Some(item.mask), |window| {
                            element.prepaint_at(item.bounds.origin, window, cx);
                        });
                    });
                    element
                })
            })
            .collect()
    }

    /// The layer's paint: the painted, uncovered items in paint order.
    fn paint(&self, built: &mut [Option<AnyElement>], window: &mut Window, cx: &mut App) {
        let items = self.0.items.borrow();
        let mut order: Vec<usize> = (0..items.len())
            .filter(|&ix| items[ix].seq.get() != 0 && !items[ix].inline.get())
            .collect();
        order.sort_by_key(|&ix| items[ix].seq.get());
        for ix in order {
            let item = &items[ix];
            if item.visible().is_none() {
                continue;
            }
            window.with_content_mask(Some(item.mask), |window| match &item.content {
                Content::Frames { clip, .. } => clip.paint(item.bounds, window),
                Content::Paint { paint, .. } => paint(item.bounds, window),
                Content::Tile { .. } | Content::Mirror { .. } => {
                    if let Some(element) = built.get_mut(ix).and_then(Option::as_mut) {
                        window.with_text_style(item.text.clone(), |window| {
                            element.paint(window, cx);
                        });
                    }
                }
            });
        }
    }
}

/// Animated content in a slice: takes its place in the layout (its own
/// box, or `child`'s), and is drawn by the slice's layer.
pub(super) struct Layered {
    layer: Option<Layer>,
    content: Content,
    child: Option<AnyElement>,
    style: StyleRefinement,
}

/// An animated sticker or custom emoji, sized by its style.
pub(super) fn frames(clip: LayeredClip, max_fps: u32) -> Layered {
    Layered::new(Content::Frames { clip, max_fps }, None)
}

/// Clock-driven drawing at `fps`, sized by its style.
pub(super) fn painter(fps: u32, paint: impl Fn(Bounds<Pixels>, &mut Window) + 'static) -> Layered {
    Layered::new(
        Content::Paint {
            paint: Rc::new(paint),
            fps,
        },
        None,
    )
}

/// A live tile: `child` as the slice draws it, `build` the same from the
/// current frame, redrawn by the layer.
pub(super) fn tile(
    child: impl IntoElement,
    fps: u32,
    build: impl Fn() -> Option<AnyElement> + 'static,
) -> Layered {
    Layered::new(
        Content::Tile {
            build: Rc::new(build),
            fps,
        },
        Some(child.into_any_element()),
    )
}

/// `child`, which the slice paints over layered content: the layer redraws
/// a copy (`build`) above whatever it draws under it.
pub(super) fn mirror(child: impl IntoElement, build: impl Fn() -> AnyElement + 'static) -> Layered {
    Layered::new(
        Content::Mirror {
            build: Rc::new(move || Some(build())),
        },
        Some(child.into_any_element()),
    )
}

impl Layered {
    fn new(content: Content, child: Option<AnyElement>) -> Self {
        Self {
            layer: current(),
            content,
            child,
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for Layered {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl IntoElement for Layered {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Layered {
    type RequestLayoutState = ();
    /// The item's index in the layer.
    type PrepaintState = Option<usize>;

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
        if let Some(child) = self.child.as_mut() {
            return (child.request_layout(window, cx), ());
        }
        let mut style = Style::default();
        style.refine(&self.style);
        (window.request_layout(style, [], cx), ())
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
        if let Some(child) = self.child.as_mut() {
            child.prepaint(window, cx);
        }
        let layer = self.layer.as_ref()?;
        // What the built copies inherit here (the bubble's line height…).
        let text =
            matches!(self.content, Content::Tile { .. } | Content::Mirror { .. }).then(|| {
                window
                    .text_style()
                    .subtract(&TextStyleRefinement::default())
            });
        Some(layer.register(self.content.clone(), bounds, window.content_mask(), text))
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        index: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let inline = self.layer.as_ref().zip(*index).and_then(|(layer, index)| {
            let items = layer.0.items.borrow();
            let item = items.get(index)?;
            item.seq.set(next_seq());
            Some(item.inline.get())
        });
        // Without a layer, or covered: the slice draws it. A mirror the
        // layer draws is drawn there only.
        let drawn_by_layer = inline == Some(false);
        if let Some(child) = self.child.as_mut()
            && !(drawn_by_layer && self.content.is_mirror())
        {
            child.paint(window, cx);
        }
        if !drawn_by_layer {
            self.content.paint_inline(bounds, window);
        }
    }
}

/// Something the slice paints over its content (a popover, a floating
/// button): layered items under it are drawn by the slice instead.
pub(super) struct Occluder {
    layer: Option<Layer>,
    child: AnyElement,
}

pub(super) fn occluder(child: impl IntoElement) -> Occluder {
    Occluder {
        layer: current(),
        child: child.into_any_element(),
    }
}

impl IntoElement for Occluder {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Occluder {
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
        if let Some(layer) = &self.layer {
            let covered = bounds.intersect(&window.content_mask().bounds);
            layer.0.occluders.borrow_mut().push(covered);
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

/// Draws a slice's layered content over the slice's (possibly replayed)
/// frame, at the time of this frame. Rendered right after the slice.
pub(super) struct AnimationLayer {
    layer: Layer,
    app: WeakEntity<QuillApp>,
}

impl AnimationLayer {
    pub(super) fn new(layer: Layer, app: WeakEntity<QuillApp>) -> Self {
        Self { layer, app }
    }
}

impl Render for AnimationLayer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        LayerPainter {
            layer: self.layer.clone(),
            app: self.app.clone(),
            painter: cx.entity_id(),
        }
    }
}

struct LayerPainter {
    layer: Layer,
    app: WeakEntity<QuillApp>,
    /// The [`AnimationLayer`] view, the frame-clock target.
    painter: EntityId,
}

impl IntoElement for LayerPainter {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for LayerPainter {
    type RequestLayoutState = ();
    type PrepaintState = Vec<Option<AnyElement>>;

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
        // Out of the flow: it draws at the bounds the slice reported.
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
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.layer.prepaint(window, cx)
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        built: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.layer.paint(built, window, cx);
        // Keep the frame clock going while anything here animates (also
        // the first frame after new content appeared).
        let (layer, inline) = self.layer.demand();
        if layer > 0 || inline > 0 {
            let (app, painter) = (self.app.clone(), self.painter);
            cx.defer(move |cx| {
                let _ = app.update(cx, |app, cx| app.tick_animation_layer(painter, cx));
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Content, Layer, LayeredClip};
    use gpui_kit::{Bounds, ContentMask, RenderImage, point, px, size};
    use std::rc::Rc;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Bounds<gpui_kit::Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
    }

    fn mask() -> ContentMask<gpui_kit::Pixels> {
        ContentMask {
            bounds: rect(0., 0., 1000., 1000.),
        }
    }

    fn frames() -> Content {
        Content::Frames {
            clip: clip(true, 0),
            max_fps: 30,
        }
    }

    fn tile() -> Content {
        Content::Tile {
            build: Rc::new(|| None),
            fps: 30,
        }
    }

    fn mirror() -> Content {
        Content::Mirror {
            build: Rc::new(|| None),
        }
    }

    /// The slice paints every registered item, in order.
    fn paint_all(layer: &Layer) {
        for (seq, item) in layer.0.items.borrow().iter().enumerate() {
            item.seq.set(seq as u64 + 1);
        }
        layer.end_frame();
    }

    #[test]
    fn covered_content_is_drawn_by_the_slice() {
        let layer = Layer::default();
        layer.begin_frame();
        layer.0.occluders.borrow_mut().push(rect(0., 0., 50., 50.));
        layer.register(frames(), rect(40., 40., 20., 20.), mask(), None);
        layer.register(frames(), rect(100., 100., 20., 20.), mask(), None);
        // Hidden by its mask: neither drawn nor ticking.
        layer.register(
            frames(),
            rect(2000., 2000., 20., 20.),
            ContentMask {
                bounds: rect(0., 0., 10., 10.),
            },
            None,
        );
        layer.plan();
        paint_all(&layer);
        let items = layer.0.items.borrow();
        assert!(items[0].inline.get());
        assert!(!items[1].inline.get());
        drop(items);
        // The covered emoji ticks the slice, the other one the layer (at
        // the clip's 10 fps).
        assert_eq!(layer.demand(), (10, 10));
    }

    #[test]
    fn mirrors_are_built_only_over_layered_tiles() {
        let layer = Layer::default();
        layer.begin_frame();
        layer.register(tile(), rect(0., 0., 100., 100.), mask(), None);
        layer.register(mirror(), rect(80., 80., 10., 10.), mask(), None);
        layer.register(mirror(), rect(500., 500., 10., 10.), mask(), None);
        assert_eq!(layer.plan(), vec![true, true, false]);
        {
            // The pill over the video is the layer's alone (the slice skips
            // it, so a translucent pill doesn't darken twice); the other
            // one stays with the slice.
            let items = layer.0.items.borrow();
            assert!(!items[1].inline.get());
            assert!(items[2].inline.get());
        }
        // Covered, the tile is the slice's: no copy of the pill either.
        layer.begin_frame();
        layer.0.occluders.borrow_mut().push(rect(0., 0., 10., 10.));
        layer.register(tile(), rect(0., 0., 100., 100.), mask(), None);
        layer.register(mirror(), rect(80., 80., 10., 10.), mask(), None);
        assert_eq!(layer.plan(), vec![false, false]);
    }

    #[test]
    fn a_replay_keeps_what_the_slice_painted() {
        let layer = Layer::default();
        layer.begin_frame();
        layer.register(tile(), rect(0., 0., 100., 100.), mask(), None);
        layer.plan();
        paint_all(&layer);
        // A prepaint the slice discarded left an item it never painted.
        layer.register(tile(), rect(200., 0., 100., 100.), mask(), None);
        assert_eq!(layer.plan(), vec![true, false]);
        assert_eq!(layer.demand(), (30, 0));
        // Nothing animates once the slice prepaints an empty frame.
        layer.begin_frame();
        assert_eq!(layer.demand(), (0, 0));
    }

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
            shown: None,
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
