//! Animation layers: animated content drawn over a cached slice's frame.
//!
//! GPUI re-renders every ancestor of a notified view, and a re-rendered
//! cached view re-renders its whole subtree (`view.rs`, `window.rs`
//! `mark_view_dirty`). So anything animating inside the chat list or the
//! conversation would rebuild the whole slice on every frame. Instead, the
//! slice only reports where its animated content sits, and a small layer
//! view rendered right after the slice (its sibling) draws that content
//! at the current time. A frame-clock tick notifies only the layer, a live
//! view (`Entity::live`): it renders alone, and all the rest of the window,
//! `QuillApp` and the slices included, stays as the last whole frame drew it.
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
//! On a tick the layers render, prepaint and paint on their own, and only
//! their paint reaches the screen: what a layer builds takes no clicks, no
//! hover and no part in the accessibility tree, as the slice under it has
//! all of those. It must build no hitboxes either (GPUI draws a live view
//! holding any in whole frames): a [`hover_tile`] that shows the pointer
//! asks the slice's hit test instead.
//!
//! The layer paints after the whole slice, so something the slice paints
//! over an item must be an [`Occluder`] (the item then falls back to the
//! slice) or a [`mirror`] (redrawn by the layer above the item). Anything
//! painted after the slice (menus, dialogs, the media viewer, tooltips,
//! deferred popovers) covers the layer as it covered the slice.
//!
//! A layer draws its own window: on a tick no window's state need be in
//! place, so it enters its window's scope and image cache itself, and draws
//! nothing when that window can't be entered.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::frame_clock::TickDraw;
use super::image_budget::WindowDraw;
use super::sticker_player::StickerView;
use gpui_kit::*;
use quill::ids::ChatId;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

thread_local! {
    /// The layer of the slice being rendered, laid out or painted.
    static CURRENT: Cell<Option<Layer>> = const { Cell::new(None) };
    /// Paint order of layered items (see [`Item::seq`]).
    static PAINT_SEQ: Cell<u64> = const { Cell::new(0) };
    /// The slice is painting a tile the layer draws again on top.
    static UNDER_LAYER: Cell<bool> = const { Cell::new(false) };
}

/// Whether what is painted now is a live tile's row copy, under the one
/// the layer draws: a video in it must not ask the slice for frames (the
/// layer's copy asks the layer).
pub(super) fn painting_under_layer() -> bool {
    UNDER_LAYER.with(Cell::get)
}

/// The layer that content built now belongs to, if any: only content of a
/// slice with a layer is drawn by one.
pub(super) fn current() -> Option<Layer> {
    CURRENT.with(|current| {
        let layer = current.take();
        current.set(layer.clone());
        layer
    })
}

/// Run `f` with `layer` as the current one.
pub(super) fn with_layer<R>(layer: Option<&Layer>, f: impl FnOnce() -> R) -> R {
    let outer = CURRENT.with(|current| current.replace(layer.cloned()));
    let result = f();
    CURRENT.with(|current| current.set(outer));
    result
}

fn next_seq() -> u64 {
    PAINT_SEQ.with(|seq| {
        seq.set(seq.get() + 1);
        seq.get()
    })
}

/// Paints time-driven content into the given bounds.
pub(super) type PaintFn = Rc<dyn Fn(Bounds<Pixels>, &mut Window)>;
/// Builds the element the layer draws, afresh each frame, told whether
/// the pointer is on the item.
pub(super) type BuildFn = Rc<dyn Fn(bool) -> Option<AnyElement>>;

/// What a layered item draws.
#[derive(Clone)]
pub(super) enum Content {
    /// An animated sticker or custom emoji.
    Frames(StickerView),
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
            Content::Frames(view) => view.fps(),
            Content::Paint { fps, .. } | Content::Tile { fps, .. } => *fps,
            Content::Mirror { .. } => 0,
        }
    }

    fn changes_in(&self, draw: TickDraw) -> bool {
        match self {
            Content::Frames(view) => view.changes_in(draw),
            Content::Paint { .. } | Content::Tile { .. } => true,
            Content::Mirror { .. } => false,
        }
    }

    fn is_mirror(&self) -> bool {
        matches!(self, Content::Mirror { .. })
    }

    /// Draw it where the slice paints it (an occluder covers it, or no
    /// layer draws it). Tiles and mirrors are the row's own children.
    fn paint_inline(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        match self {
            Content::Frames(view) => view.paint(bounds, window),
            Content::Paint { paint, .. } => paint(bounds, window),
            Content::Tile { .. } | Content::Mirror { .. } => {}
        }
    }
}

#[derive(Clone)]
struct Item {
    content: Content,
    bounds: Bounds<Pixels>,
    mask: ContentMask<Pixels>,
    /// The text style where the slice lays it out, for the elements the
    /// layer builds (tiles, mirrors) to lay out the same.
    text: Option<TextStyleRefinement>,
    /// Where the item stands in the slice's frame.
    slot: Cell<Slot>,
    /// Its place in the slice's hit test (a [`hover_tile`]).
    hitbox: Option<Hitbox>,
}

/// Where an item stands in the slice's frame: registered by its prepaint,
/// or painted.
#[derive(Clone, Copy)]
enum Slot {
    /// Registered by a prepaint; not painted (yet: a discarded prepaint
    /// leaves it here). `inline`: the slice draws it, because something it
    /// paints later covers the item, or (a mirror) nothing the layer draws
    /// is under it. A mirror the layer draws isn't painted by the slice at
    /// all, so a translucent pill doesn't darken twice.
    Registered { inline: bool },
    /// The slice painted it, at paint order `seq` and at `opacity` (a dimmed
    /// cell).
    Painted {
        inline: bool,
        seq: u64,
        opacity: f32,
    },
}

impl Item {
    fn inline(&self) -> bool {
        match self.slot.get() {
            Slot::Registered { inline } | Slot::Painted { inline, .. } => inline,
        }
    }

    fn set_inline(&self, inline: bool) {
        self.slot.set(match self.slot.get() {
            Slot::Registered { .. } => Slot::Registered { inline },
            Slot::Painted { seq, opacity, .. } => Slot::Painted {
                inline,
                seq,
                opacity,
            },
        });
    }

    /// Paint order and opacity, once the slice painted it.
    fn painted_at(&self) -> Option<(u64, f32)> {
        match self.slot.get() {
            Slot::Painted { seq, opacity, .. } => Some((seq, opacity)),
            Slot::Registered { .. } => None,
        }
    }

    fn painted(&self, window: &Window) {
        self.slot.set(Slot::Painted {
            inline: self.inline(),
            seq: next_seq(),
            opacity: window.element_opacity(),
        });
    }

    /// The part of it that can show.
    fn visible(&self) -> Option<Bounds<Pixels>> {
        let visible = self.bounds.intersect(&self.mask.bounds);
        (visible.size.width > Pixels::ZERO && visible.size.height > Pixels::ZERO).then_some(visible)
    }
}

/// The slice's frame: the items its prepaint registers, then, once it
/// painted, the same items as final.
enum Frame {
    /// The slice prepaints.
    Prepainting(Vec<Item>),
    /// The slice painted since it last prepainted: the items are final.
    Painted(Vec<Item>),
}

impl Default for Frame {
    fn default() -> Self {
        Self::Prepainting(Vec::new())
    }
}

impl Frame {
    fn items(&self) -> &[Item] {
        match self {
            Self::Prepainting(items) | Self::Painted(items) => items,
        }
    }

    fn items_mut(&mut self) -> &mut Vec<Item> {
        match self {
            Self::Prepainting(items) | Self::Painted(items) => items,
        }
    }
}

#[derive(Default)]
struct LayerState {
    frame: RefCell<Frame>,
    occluders: RefCell<Vec<Bounds<Pixels>>>,
}

/// A slice's animated content and what covers it, as of the slice's last
/// real frame.
#[derive(Clone, Default)]
pub(super) struct Layer(Rc<LayerState>);

impl Layer {
    /// Read the frame, unless it is being changed (a re-entrant call finds
    /// nothing instead of panicking). `f` never calls into GPUI.
    fn read<R>(&self, f: impl FnOnce(&Frame) -> R) -> Option<R> {
        self.0.frame.try_borrow().ok().map(|frame| f(&frame))
    }

    fn write<R>(&self, f: impl FnOnce(&mut Frame) -> R) -> Option<R> {
        self.0
            .frame
            .try_borrow_mut()
            .ok()
            .map(|mut frame| f(&mut frame))
    }

    /// The items as of now, copied out so nothing is held while the layer
    /// calls into GPUI.
    fn snapshot(&self) -> Vec<Item> {
        self.read(|frame| frame.items().to_vec())
            .unwrap_or_default()
    }

    fn add_occluder(&self, covered: Bounds<Pixels>) {
        if let Ok(mut occluders) = self.0.occluders.try_borrow_mut() {
            occluders.push(covered);
        }
    }

    /// The slice prepaints a new frame: forget the last one's content.
    pub(super) fn begin_frame(&self) {
        self.write(|frame| *frame = Frame::Prepainting(Vec::new()));
        if let Ok(mut occluders) = self.0.occluders.try_borrow_mut() {
            occluders.clear();
        }
    }

    /// The slice painted its frame (`SliceScope`).
    pub(super) fn end_frame(&self) {
        self.write(|frame| {
            let items = std::mem::take(frame.items_mut());
            *frame = Frame::Painted(items);
        });
    }

    fn register(
        &self,
        content: Content,
        bounds: Bounds<Pixels>,
        mask: ContentMask<Pixels>,
        text: Option<TextStyleRefinement>,
        hitbox: Option<Hitbox>,
    ) -> Option<usize> {
        self.write(|frame| {
            let items = frame.items_mut();
            items.push(Item {
                content,
                bounds,
                mask,
                text,
                slot: Cell::new(Slot::Registered { inline: false }),
                hitbox,
            });
            items.len().saturating_sub(1)
        })
    }

    /// What covered the slice's content in its last real frame.
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn occluders(&self) -> Vec<Bounds<Pixels>> {
        self.0
            .occluders
            .try_borrow()
            .map(|occluders| occluders.clone())
            .unwrap_or_default()
    }

    fn occluded(&self, visible: Bounds<Pixels>) -> bool {
        self.0
            .occluders
            .try_borrow()
            .is_ok_and(|occluders| occluders.iter().any(|o| o.intersects(&visible)))
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
        let Some(index) = self.register(content, bounds, mask, None, None) else {
            return;
        };
        self.read(|frame| {
            if let Some(item) = frame.items().get(index) {
                item.set_inline(inline);
                item.painted(window);
            }
        });
    }

    /// The frame rates its painted content wants: drawn by the layer, and
    /// drawn inline by the slice.
    pub(super) fn demand(&self) -> (u32, u32) {
        self.read(|frame| {
            let (mut layer, mut inline) = (0, 0);
            for item in frame.items() {
                if item.painted_at().is_none() || item.visible().is_none() {
                    continue;
                }
                let fps = item.content.fps();
                if item.inline() {
                    inline = inline.max(fps);
                } else {
                    layer = layer.max(fps);
                }
            }
            (layer, inline)
        })
        .unwrap_or_default()
    }

    /// Whether drawing the layer in `draw` would change what it shows. A
    /// sticker held to its file's rate ticks faster than its frames change,
    /// so each frame stays up for the same number of refreshes; the ticks
    /// between need no draw.
    pub(super) fn changes_in(&self, draw: TickDraw) -> bool {
        self.read(|frame| {
            frame.items().iter().any(|item| {
                item.painted_at().is_some()
                    && !item.inline()
                    && item.visible().is_some()
                    && item.content.changes_in(draw)
            })
        })
        .unwrap_or(false)
    }

    /// Before the slice paints a new frame (the layer's prepaint): decide
    /// which items the slice draws (`inline`), and say which ones need an
    /// element built (tiles and mirrors the layer draws).
    fn plan(&self) -> Vec<bool> {
        self.read(|frame| {
            // A replayed slice: what its last real frame painted stays
            // decided, and what it didn't paint (a discarded prepaint)
            // never shows.
            let replay = matches!(frame, Frame::Painted(_));
            let items = frame.items();
            let mut drawn: Vec<Bounds<Pixels>> = Vec::new();
            let mut build = Vec::with_capacity(items.len());
            for item in items {
                let visible = item
                    .visible()
                    .filter(|_| !replay || item.painted_at().is_some());
                let Some(visible) = visible else {
                    build.push(false);
                    continue;
                };
                let mirror = item.content.is_mirror();
                if !replay {
                    item.set_inline(if mirror {
                        !drawn.iter().any(|r| r.intersects(&visible))
                    } else {
                        self.occluded(visible)
                    });
                }
                let layered = !item.inline();
                if layered && !mirror {
                    drawn.push(visible);
                }
                build.push(
                    layered
                        && matches!(item.content, Content::Tile { .. } | Content::Mirror { .. }),
                );
            }
            build
        })
        .unwrap_or_default()
    }

    /// The layer's prepaint: the elements it redraws, laid out and
    /// prepainted at their items' bounds.
    fn prepaint(&self, window: &mut Window, cx: &mut App) -> Vec<Option<AnyElement>> {
        let plan = self.plan();
        // Copied out: building an element may register or paint layered
        // content, which must not find the frame held.
        self.snapshot()
            .iter()
            .zip(plan)
            .map(|(item, build)| {
                let element = match &item.content {
                    Content::Tile { build: element, .. } | Content::Mirror { build: element }
                        if build =>
                    {
                        let hovered = item
                            .hitbox
                            .as_ref()
                            .is_some_and(|hitbox| hitbox.is_hovered(window));
                        element(hovered)
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
        let items = self.snapshot();
        let mut order: Vec<(usize, u64, f32)> = items
            .iter()
            .enumerate()
            .filter(|(_, item)| !item.inline())
            .filter_map(|(ix, item)| {
                let (seq, opacity) = item.painted_at()?;
                Some((ix, seq, opacity))
            })
            .collect();
        order.sort_by_key(|&(_, seq, _)| seq);
        let painter_opacity = window.element_opacity();
        for (ix, _, item_opacity) in order {
            let Some(item) = items.get(ix) else {
                continue;
            };
            if item.visible().is_none() {
                continue;
            }
            let opacity = (painter_opacity > 0.).then(|| item_opacity / painter_opacity);
            window.with_element_opacity(opacity, |window| {
                window.with_content_mask(Some(item.mask), |window| match &item.content {
                    Content::Frames(view) => view.paint(item.bounds, window),
                    Content::Paint { paint, .. } => paint(item.bounds, window),
                    Content::Tile { .. } | Content::Mirror { .. } => {
                        if let Some(element) = built.get_mut(ix).and_then(Option::as_mut) {
                            window.with_text_style(item.text.clone(), |window| {
                                element.paint(window, cx);
                            });
                        }
                    }
                });
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
    hover: bool,
}

/// An animated sticker or custom emoji, sized by its style.
pub(super) fn frames(view: StickerView) -> Layered {
    Layered::new(Content::Frames(view), None)
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
    hover_tile(child, fps, move |_| build())
}

/// A [`tile`] that `build` draws differently under the pointer. The hover
/// shows from the tile's next frame.
pub(super) fn hover_tile(
    child: impl IntoElement,
    fps: u32,
    build: impl Fn(bool) -> Option<AnyElement> + 'static,
) -> Layered {
    let mut tile = Layered::new(
        Content::Tile {
            build: Rc::new(build),
            fps,
        },
        Some(child.into_any_element()),
    );
    tile.hover = true;
    tile
}

/// `child`, which the slice paints over layered content: the layer redraws
/// a copy (`build`) above whatever it draws under it.
pub(super) fn mirror(child: impl IntoElement, build: impl Fn() -> AnyElement + 'static) -> Layered {
    Layered::new(
        Content::Mirror {
            build: Rc::new(move |_| Some(build())),
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
            hover: false,
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
        let hitbox = self
            .hover
            .then(|| window.insert_hitbox(bounds, HitboxBehavior::Normal));
        layer.register(
            self.content.clone(),
            bounds,
            window.content_mask(),
            text,
            hitbox,
        )
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
            layer
                .read(|frame| {
                    let item = frame.items().get(index)?;
                    item.painted(window);
                    Some(item.inline())
                })
                .flatten()
        });
        // Without a layer, or covered: the slice draws it. A mirror the
        // layer draws is drawn there only.
        let drawn_by_layer = inline == Some(false);
        if let Some(child) = self.child.as_mut()
            && !(drawn_by_layer && self.content.is_mirror())
        {
            let under = drawn_by_layer && matches!(self.content, Content::Tile { .. });
            let outer = UNDER_LAYER.with(|flag| flag.replace(under));
            child.paint(window, cx);
            UNDER_LAYER.with(|flag| flag.set(outer));
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
            layer.add_occluder(covered);
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
    /// The chat window it draws in (`None`: the main window).
    window: Option<ChatId>,
}

impl AnimationLayer {
    pub(super) fn new(layer: Layer, app: WeakEntity<QuillApp>, window: Option<ChatId>) -> Self {
        Self { layer, app, window }
    }
}

impl Render for AnimationLayer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let target = self.window;
        super::frame_clock::trace_slice_render(match target {
            None => "animation layer",
            Some(_) => "chat window layer",
        });
        let id = cx.entity_id();
        let painter = LayerPainter {
            layer: self.layer.clone(),
            app: self.app.clone(),
            painter: id,
            window: target,
        };
        let Some(app) = self.app.upgrade() else {
            return painter.into_any_element();
        };
        let content = app.update(cx, |app, cx| {
            let outer_scope = app.windows.scope;
            let content = app.set_window_scope(target).then(|| {
                match target {
                    None => app.main_frame_starts(window, WindowDraw::Patch, cx),
                    Some(chat) => app.chat_frame_starts(chat, WindowDraw::Patch, window, cx),
                }
                super::image_budget::slice_rendered(id, target);
                let cache = app.image_cache_in_place();
                app.slices.painter_scope(id, cache, painter)
            });
            app.set_window_scope(outer_scope);
            content
        });
        let Some(content) = content else {
            return Empty.into_any_element();
        };
        super::chat_window::WindowScope::new(self.app.clone(), target, content).into_any_element()
    }
}

struct LayerPainter {
    layer: Layer,
    app: WeakEntity<QuillApp>,
    /// The [`AnimationLayer`] view, the frame-clock target.
    painter: EntityId,
    window: Option<ChatId>,
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
            let (app, painter, scope) = (self.app.clone(), self.painter, self.window);
            // screen-scope: the layer animates whatever is on screen now; the
            // tick redraws, it owns nothing of an account.
            cx.defer(move |cx| {
                let _ = app.update(cx, |app, cx| {
                    app.in_window_scope(scope, cx, |app, cx| {
                        app.tick_animation_layer(painter, cx);
                    })
                });
            });
        }
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "layer_tick_tests.rs"]
pub(in crate::ui) mod layer_tick_tests;

#[cfg(test)]
mod tests {
    use super::{Content, Layer, Slot};
    use gpui_kit::{Bounds, ContentMask, point, px, size};
    use std::rc::Rc;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Bounds<gpui_kit::Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
    }

    fn mask() -> ContentMask<gpui_kit::Pixels> {
        ContentMask {
            bounds: rect(0., 0., 1000., 1000.),
        }
    }

    /// Animated content at 10 fps.
    fn frames() -> Content {
        Content::Paint {
            paint: Rc::new(|_, _| {}),
            fps: 10,
        }
    }

    fn tile() -> Content {
        Content::Tile {
            build: Rc::new(|_| None),
            fps: 30,
        }
    }

    fn mirror() -> Content {
        Content::Mirror {
            build: Rc::new(|_| None),
        }
    }

    /// The slice paints every registered item, in order.
    fn paint_all(layer: &Layer) {
        layer.read(|frame| {
            for (seq, item) in frame.items().iter().enumerate() {
                item.slot.set(Slot::Painted {
                    inline: item.inline(),
                    seq: seq as u64 + 1,
                    opacity: 1.,
                });
            }
        });
        layer.end_frame();
    }

    #[test]
    fn covered_content_is_drawn_by_the_slice() {
        let layer = Layer::default();
        layer.begin_frame();
        layer.add_occluder(rect(0., 0., 50., 50.));
        layer.register(frames(), rect(40., 40., 20., 20.), mask(), None, None);
        layer.register(frames(), rect(100., 100., 20., 20.), mask(), None, None);
        // Hidden by its mask: neither drawn nor ticking.
        layer.register(
            frames(),
            rect(2000., 2000., 20., 20.),
            ContentMask {
                bounds: rect(0., 0., 10., 10.),
            },
            None,
            None,
        );
        layer.plan();
        paint_all(&layer);
        let inline =
            layer.read(|frame| frame.items().iter().map(|i| i.inline()).collect::<Vec<_>>());
        assert_eq!(
            inline.as_deref().and_then(|i| i.get(..2)),
            Some(&[true, false][..])
        );
        // The covered emoji ticks the slice, the other one the layer (at
        // the clip's 10 fps).
        assert_eq!(layer.demand(), (10, 10));
    }

    #[test]
    fn mirrors_are_built_only_over_layered_tiles() {
        let layer = Layer::default();
        layer.begin_frame();
        layer.register(tile(), rect(0., 0., 100., 100.), mask(), None, None);
        layer.register(mirror(), rect(80., 80., 10., 10.), mask(), None, None);
        layer.register(mirror(), rect(500., 500., 10., 10.), mask(), None, None);
        assert_eq!(layer.plan(), vec![true, true, false]);
        {
            // The pill over the video is the layer's alone (the slice skips
            // it, so a translucent pill doesn't darken twice); the other
            // one stays with the slice.
            let inline =
                layer.read(|frame| frame.items().iter().map(|i| i.inline()).collect::<Vec<_>>());
            assert_eq!(inline, Some(vec![false, false, true]));
        }
        // Covered, the tile is the slice's: no copy of the pill either.
        layer.begin_frame();
        layer.add_occluder(rect(0., 0., 10., 10.));
        layer.register(tile(), rect(0., 0., 100., 100.), mask(), None, None);
        layer.register(mirror(), rect(80., 80., 10., 10.), mask(), None, None);
        assert_eq!(layer.plan(), vec![false, false]);
    }

    #[test]
    fn a_replay_keeps_what_the_slice_painted() {
        let layer = Layer::default();
        layer.begin_frame();
        layer.register(tile(), rect(0., 0., 100., 100.), mask(), None, None);
        layer.plan();
        paint_all(&layer);
        // A prepaint the slice discarded left an item it never painted.
        layer.register(tile(), rect(200., 0., 100., 100.), mask(), None, None);
        assert_eq!(layer.plan(), vec![true, false]);
        assert_eq!(layer.demand(), (30, 0));
        // Nothing animates once the slice prepaints an empty frame.
        layer.begin_frame();
        assert_eq!(layer.demand(), (0, 0));
    }
}
