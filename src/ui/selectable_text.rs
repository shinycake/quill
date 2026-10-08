//! Message text that takes part in the window's text selection, so part
//! of a message (or a span across messages) can be selected and copied, as
//! in Telegram Desktop. Modelled on `gpui_base::SelectableText`, which only
//! takes plain text: this element lays out a styled `StyledText`
//! (highlights for bold, links, spoilers…) and keeps the inline click
//! targets (links, spoilers) a plain `InteractiveText` had.

use gpui_kit::base::{
    TextSelection, TextSelectionHandle, TextSelectionRegistration, TextSelectionRun,
};
use gpui_kit::*;
use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;

thread_local! {
    /// Bounds of the scrolling region whose paint is in progress; see
    /// [`SelectionViewport`].
    static VIEWPORT: Cell<Option<Bounds<Pixels>>> = const { Cell::new(None) };
    /// The message each painted text participant belongs to, by the
    /// participant's entity id; see [`SelectableRichText::message`].
    /// The message whose text the current selection covers, as last
    /// painted; see [`selected_message_text`].
    static SELECTED_MESSAGE: Cell<Option<(i64, u64)>> = const { Cell::new(None) };
    static OWNERS: std::cell::RefCell<std::collections::HashMap<EntityId, (i64, u64)>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Called with the index of the clicked range.
type ClickHandler = Rc<dyn Fn(usize, &mut Window, &mut App)>;

/// A press that moved less than this is a click, not a selection drag.
const CLICK_SLOP: f32 = 4.;

pub(super) struct SelectableRichText {
    id: ElementId,
    text: SharedString,
    styled: StyledText,
    click_ranges: Vec<Range<usize>>,
    on_click: Option<ClickHandler>,
    selection_color: Hsla,
    document_order: u64,
    message: Option<(i64, u64)>,
    /// Hidden spoiler runs and their specks' opacity.
    spoilers: Vec<(Range<usize>, f32)>,
    /// The animation layer that draws the specks (`anim_layer`), if any.
    layer: Option<super::anim_layer::Layer>,
}

impl SelectableRichText {
    pub(super) fn new(id: impl Into<ElementId>, text: SharedString, styled: StyledText) -> Self {
        Self {
            id: id.into(),
            text,
            styled,
            click_ranges: Vec::new(),
            on_click: None,
            selection_color: gpui_kit::hsla(0.58, 0.8, 0.6, 0.35),
            document_order: 0,
            message: None,
            spoilers: Vec::new(),
            layer: None,
        }
    }

    /// Byte ranges drawn as spoiler specks in the text's color (the text
    /// itself is styled invisible by the caller), with their opacity.
    pub(super) fn spoilers(mut self, spoilers: Vec<(Range<usize>, f32)>) -> Self {
        self.spoilers = spoilers;
        self.layer = super::anim_layer::current();
        self
    }

    /// Reading order among all selectable text in the window: a selection
    /// runs through paragraphs (and from a message to its edge) in this
    /// order.
    pub(super) fn document_order(mut self, order: u64) -> Self {
        self.document_order = order;
        self
    }

    /// The message (chat id, message id) this text belongs to. As in
    /// Telegram Desktop, a text selection stays inside the message it
    /// started in: dragged past it, the selection runs to that message's
    /// edge, and text of other messages neither highlights nor copies.
    pub(super) fn message(mut self, key: (i64, u64)) -> Self {
        self.message = Some(key);
        self
    }

    /// Byte ranges that act on a click (links, spoilers) and their handler.
    pub(super) fn on_click(
        mut self,
        ranges: Vec<Range<usize>>,
        handler: impl Fn(usize, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.click_ranges = ranges;
        self.on_click = Some(Rc::new(handler));
        self
    }

    pub(super) fn selection_color(mut self, color: Hsla) -> Self {
        self.selection_color = color;
        self
    }

    fn clicked_range(&self, layout: &TextLayout, position: Point<Pixels>) -> Option<usize> {
        let index = layout.index_for_position(position).ok()?;
        self.click_ranges
            .iter()
            .position(|range| range.contains(&index))
    }
}

/// Selection highlight rectangles from `start` to `end` over wrapped lines.
fn selection_quads(
    start: Point<Pixels>,
    end: Point<Pixels>,
    bounds: Bounds<Pixels>,
    line_height: Pixels,
) -> Vec<Bounds<Pixels>> {
    if start.y == end.y {
        return vec![Bounds::from_corners(
            start,
            point(end.x, end.y + line_height),
        )];
    }
    let mut quads = vec![Bounds::from_corners(
        start,
        point(bounds.right(), start.y + line_height),
    )];
    if end.y > start.y + line_height {
        quads.push(Bounds::from_corners(
            point(bounds.left(), start.y + line_height),
            point(bounds.right(), end.y),
        ));
    }
    quads.push(Bounds::from_corners(
        point(bounds.left(), end.y),
        point(end.x, end.y + line_height),
    ));
    quads
}

impl IntoElement for SelectableRichText {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SelectableRichText {
    type RequestLayoutState = TextSelectionHandle;
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        // The selection handle lives in element state, so a selection
        // survives re-renders of the row. A selection change redraws the
        // whole window: the text may sit in a cached view (`app_slice`)
        // that would otherwise replay its old highlight.
        let handle = window.with_element_state(
            global_id.expect("SelectableRichText has an element id"),
            |retained: Option<(TextSelectionHandle, Subscription)>, window| {
                let (handle, refresh) = retained.unwrap_or_else(|| {
                    let handle = TextSelectionHandle::new(self.text.clone(), cx);
                    let refresh = handle.refresh_window_on_change(window, cx);
                    (handle, refresh)
                });
                (handle.clone(), (handle, refresh))
            },
        );
        let (layout_id, ()) = self
            .styled
            .request_layout(global_id, inspector_id, window, cx);
        (layout_id, handle)
    }

    fn prepaint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _handle: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.styled
            .prepaint(global_id, inspector_id, bounds, &mut (), window, cx);
        window.insert_hitbox(bounds, HitboxBehavior::Normal)
    }

    fn paint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        handle: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        // Register here rather than in prepaint: a list may prepaint its rows
        // more than once and roll back all but the last pass, and a
        // registration from a discarded pass would carry stale bounds and
        // hitbox, pulling the selection cursor into the wrong message.
        // Drag auto-scroll measures the pointer against the hitbox's content
        // mask. Inside a message bubble that mask is the bubble's clip, so
        // any drag past the bubble would scroll the history; measure against
        // the enclosing history viewport instead.
        let mut selection_hitbox = hitbox.clone();
        if let Some(viewport) = VIEWPORT.with(Cell::get) {
            selection_hitbox.content_mask.bounds = viewport;
        }
        let registration = TextSelectionRegistration::new(selection_hitbox, bounds)
            .with_document_order(self.document_order)
            .with_text_bounds(vec![bounds])
            .with_rendered_element(handle, window, cx);
        handle.register(registration, window, cx);
        let layout = self.styled.layout().clone();
        let selected_before = TextSelection::selected_text(window, cx);
        if let Some(message) = self.message {
            OWNERS.with(|owners| {
                let mut owners = owners.borrow_mut();
                // Entries are tiny; drop them all now and then rather than
                // track every participant's lifetime. Painted text re-adds
                // itself the same frame.
                if owners.len() > 50_000 {
                    owners.clear();
                }
                owners.insert(handle.entity_id(), message);
            });
        }
        let anchor_message = handle
            .snapshot(cx)
            .and_then(|snapshot| snapshot.anchor().entity_id())
            .and_then(|anchor| OWNERS.with(|owners| owners.borrow().get(&anchor).copied()));
        let foreign =
            self.message.is_some() && anchor_message.is_some() && anchor_message != self.message;
        let runs = if foreign {
            Vec::new()
        } else {
            vec![
                TextSelectionRun::new(self.text.clone(), layout.clone(), bounds)
                    .with_document_order(self.document_order),
            ]
        };
        let projection = handle.update_runs(&runs, cx);
        if !foreign
            && let Some(message) = self.message
            && projection
                .ranges()
                .iter()
                .flatten()
                .any(|range| !range.is_empty())
        {
            SELECTED_MESSAGE.with(|selected| selected.set(Some(message)));
        }
        if selected_before != TextSelection::selected_text(window, cx) {
            window.refresh();
        }
        for range in projection.ranges().iter().flatten() {
            if let (Some(start), Some(end)) = (
                layout.position_for_index(range.start),
                layout.position_for_index(range.end),
            ) {
                for quad in selection_quads(start, end, layout.bounds(), layout.line_height()) {
                    window.paint_quad(fill(quad, self.selection_color));
                }
            }
        }
        self.styled.paint(
            global_id,
            inspector_id,
            bounds,
            &mut (),
            &mut (),
            window,
            cx,
        );
        if !self.spoilers.is_empty() {
            let color = window.text_style().color;
            let line_height = layout.line_height();
            for (range, opacity) in &self.spoilers {
                let (Some(start), Some(end)) = (
                    layout.position_for_index(range.start),
                    layout.position_for_index(range.end),
                ) else {
                    continue;
                };
                for rect in selection_quads(start, end, layout.bounds(), line_height) {
                    super::spoiler_fx::layer_text_specks(
                        self.layer.as_ref(),
                        rect,
                        bounds.origin,
                        color.opacity(color.a * opacity),
                        window,
                    );
                }
            }
        }

        // Links and spoilers: a press and release without a drag.
        if let Some(handler) = self.on_click.clone() {
            if hitbox.is_hovered(window)
                && self
                    .clicked_range(&layout, window.mouse_position())
                    .is_some()
            {
                window.set_cursor_style(CursorStyle::PointingHand, hitbox);
            }
            let pressed_at: Rc<std::cell::Cell<Option<Point<Pixels>>>> = window.with_element_state(
                global_id.expect("SelectableRichText has an element id"),
                |retained: Option<Rc<std::cell::Cell<Option<Point<Pixels>>>>>, _| {
                    let cell = retained.unwrap_or_default();
                    (cell.clone(), cell)
                },
            );
            let down_hitbox = hitbox.clone();
            let down_cell = pressed_at.clone();
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, _| {
                if phase == DispatchPhase::Bubble
                    && event.button == MouseButton::Left
                    && down_hitbox.is_hovered(window)
                {
                    down_cell.set(Some(event.position));
                }
            });
            let ranges = self.click_ranges.clone();
            let up_hitbox = hitbox.clone();
            window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble || event.button != MouseButton::Left {
                    return;
                }
                let Some(down) = pressed_at.take() else {
                    return;
                };
                let moved = (event.position.x - down.x).abs() > px(CLICK_SLOP)
                    || (event.position.y - down.y).abs() > px(CLICK_SLOP);
                if moved || !up_hitbox.is_hovered(window) {
                    return;
                }
                let Some(index) = layout.index_for_position(event.position).ok() else {
                    return;
                };
                if let Some(ix) = ranges.iter().position(|range| range.contains(&index)) {
                    handler(ix, window, cx);
                }
            });
        }
    }
}

/// The window's selected message text and the message it belongs to.
/// Footer padding (em spaces reserving room for the time) is stripped.
pub(super) fn selected_message_text(
    window: &mut Window,
    cx: &mut App,
) -> Option<((i64, u64), String)> {
    if !TextSelection::has_selection(window, cx) {
        return None;
    }
    let text = TextSelection::selected_text(window, cx).replace('\u{2003}', "");
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    Some((SELECTED_MESSAGE.with(Cell::get)?, text.to_string()))
}

/// Marks the scrolling region that selectable message text lives in, so a
/// selection drag scrolls it only when the pointer leaves this region (not
/// merely the bubble the drag started in).
pub(super) struct SelectionViewport {
    child: AnyElement,
}

pub(super) fn selection_viewport(child: impl IntoElement) -> SelectionViewport {
    SelectionViewport {
        child: child.into_any_element(),
    }
}

impl IntoElement for SelectionViewport {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SelectionViewport {
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
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.child.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let outer = VIEWPORT.with(|viewport| viewport.replace(Some(bounds)));
        self.child.paint(window, cx);
        VIEWPORT.with(|viewport| viewport.set(outer));
    }
}
