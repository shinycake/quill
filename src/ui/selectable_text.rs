//! Message text that takes part in the window's text selection, so part
//! of a message (or a span across messages) can be selected and copied, as
//! in Telegram Desktop. Modelled on `gpui_base::SelectableText`, which only
//! takes plain text: this element lays out a styled `StyledText`
//! (highlights for bold, links, spoilers…) and keeps the inline click
//! targets (links, spoilers) a plain `InteractiveText` had.

use gpui_kit::base::input::bidi_paragraph::BidiParagraph;
use gpui_kit::base::{
    RunGeometry, TextSelection, TextSelectionHandle, TextSelectionRegistration, TextSelectionRun,
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
    /// What the text is made of, for the bidirectional path (see [`Self::bidi`]).
    bidi_source: Option<BidiSource>,
    /// The paragraph laid out by the bidirectional path, once prepainted.
    paragraph: Option<Rc<BidiParagraph>>,
    /// The paragraph measured during layout, to reuse when its wrap width stands.
    measured: Rc<std::cell::RefCell<Option<(Option<Pixels>, Rc<BidiParagraph>)>>>,
}

/// Highlights and font overrides of a paragraph, as `StyledText` takes them.
#[derive(Clone)]
struct BidiSource {
    highlights: Vec<(Range<usize>, HighlightStyle)>,
    font_families: Vec<(Range<usize>, SharedString)>,
}

/// Text runs for `text` under `style`: the highlights over the base style, then the
/// font-family overrides, the way `StyledText` resolves them at layout time.
fn text_runs(text: &str, style: &TextStyle, source: &BidiSource) -> Vec<TextRun> {
    let mut runs = Vec::new();
    let mut ix = 0;
    for (range, highlight) in &source.highlights {
        if ix < range.start {
            runs.push(style.clone().to_run(range.start - ix));
        }
        runs.push(style.clone().highlight(*highlight).to_run(range.len()));
        ix = range.end;
    }
    if ix < text.len() {
        runs.push(style.to_run(text.len() - ix));
    }
    let mut offset = 0;
    for run in &mut runs {
        let end = offset + run.len;
        if let Some((_, family)) = source
            .font_families
            .iter()
            .find(|(range, _)| offset >= range.start && end <= range.end)
        {
            run.font.family = family.clone();
        }
        offset = end;
    }
    runs
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
            bidi_source: None,
            paragraph: None,
            measured: Rc::default(),
        }
    }

    /// Text with right-to-left characters is wrapped in typing order and each row ordered
    /// visually (GPUI wraps by glyph order, which puts the end of a Hebrew sentence on the
    /// first row). The highlights and font overrides given to the `StyledText` are needed
    /// again for that layout.
    pub(super) fn bidi(
        mut self,
        highlights: Vec<(Range<usize>, HighlightStyle)>,
        font_families: Vec<(Range<usize>, SharedString)>,
    ) -> Self {
        if quill::text::has_rtl_text(&self.text) {
            self.bidi_source = Some(BidiSource {
                highlights,
                font_families,
            });
        }
        self
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
}

/// The laid-out text a paragraph reads positions from, whichever path made it.
#[derive(Clone)]
enum Laid {
    Styled(TextLayout),
    Bidi {
        paragraph: Rc<BidiParagraph>,
        bounds: Bounds<Pixels>,
    },
}

impl Laid {
    fn index_for_position(&self, position: Point<Pixels>) -> Option<usize> {
        match self {
            Laid::Styled(layout) => layout.index_for_position(position).ok(),
            Laid::Bidi { paragraph, bounds } => paragraph
                .index_for_position(position - bounds.origin, bounds.size.width)
                .ok(),
        }
    }

    /// Rectangles covering the byte `range`, one per row (and per visual piece).
    fn range_rects(&self, range: Range<usize>) -> Vec<Bounds<Pixels>> {
        match self {
            Laid::Styled(layout) => {
                let (Some(start), Some(end)) = (
                    layout.position_for_index(range.start),
                    layout.position_for_index(range.end),
                ) else {
                    return Vec::new();
                };
                selection_quads(start, end, layout.bounds(), layout.line_height())
            }
            Laid::Bidi { paragraph, bounds } => paragraph
                .range_rects(range, bounds.size.width)
                .into_iter()
                .map(|rect| Bounds::new(rect.origin + bounds.origin, rect.size))
                .collect(),
        }
    }

    fn geometry(&self) -> Rc<dyn RunGeometry> {
        match self {
            Laid::Styled(layout) => Rc::new(layout.clone()),
            Laid::Bidi { paragraph, bounds } => {
                paragraph.geometry(bounds.origin, bounds.size.width)
            }
        }
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
        if let Some(source) = self.bidi_source.clone() {
            let style = window.text_style();
            let font_size = style.font_size.to_pixels(window.rem_size());
            let line_height = window.pixel_snap(
                style
                    .line_height
                    .to_pixels(font_size.into(), window.rem_size()),
            );
            let runs = text_runs(&self.text, &style, &source);
            let text = self.text.clone();
            let measured = self.measured.clone();
            let layout_id = window.request_measured_layout(Default::default(), {
                move |known, available, window, _| {
                    let wrap = known.width.or(match available.width {
                        AvailableSpace::Definite(width) => Some(width),
                        _ => None,
                    });
                    let paragraph =
                        BidiParagraph::layout(&text, &runs, font_size, line_height, wrap, window)
                            .map(Rc::new);
                    let size = paragraph.as_ref().map_or_else(Size::default, |p| p.size());
                    if let Some(paragraph) = paragraph {
                        *measured.borrow_mut() = Some((wrap, paragraph));
                    }
                    size
                }
            });
            return (layout_id, handle);
        }
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
        if let Some(source) = self.bidi_source.clone() {
            // The measured paragraph stands when it was wrapped at this width; else lay it out
            // again at the width it was given.
            let measured = self
                .measured
                .borrow()
                .as_ref()
                .filter(|(wrap, _)| *wrap == Some(bounds.size.width))
                .map(|(_, paragraph)| paragraph.clone());
            self.paragraph = measured.or_else(|| {
                let style = window.text_style();
                let font_size = style.font_size.to_pixels(window.rem_size());
                let line_height = window.pixel_snap(
                    style
                        .line_height
                        .to_pixels(font_size.into(), window.rem_size()),
                );
                let runs = text_runs(&self.text, &style, &source);
                BidiParagraph::layout(
                    &self.text,
                    &runs,
                    font_size,
                    line_height,
                    Some(bounds.size.width),
                    window,
                )
                .map(Rc::new)
            });
        } else {
            self.styled
                .prepaint(global_id, inspector_id, bounds, &mut (), window, cx);
        }
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
        let layout = match &self.paragraph {
            Some(paragraph) => Laid::Bidi {
                paragraph: paragraph.clone(),
                bounds,
            },
            None => Laid::Styled(self.styled.layout().clone()),
        };
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
                TextSelectionRun::with_geometry(self.text.clone(), layout.geometry(), bounds)
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
            for quad in layout.range_rects(range.clone()) {
                window.paint_quad(fill(quad, self.selection_color));
            }
        }
        match &self.paragraph {
            Some(paragraph) => paragraph.paint(bounds.origin, bounds.size.width, window, cx),
            None => self.styled.paint(
                global_id,
                inspector_id,
                bounds,
                &mut (),
                &mut (),
                window,
                cx,
            ),
        }
        if !self.spoilers.is_empty() {
            let color = window.text_style().color;
            for (range, opacity) in &self.spoilers {
                for rect in layout.range_rects(range.clone()) {
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
                && layout
                    .index_for_position(window.mouse_position())
                    .is_some_and(|index| self.click_ranges.iter().any(|r| r.contains(&index)))
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
                let Some(index) = layout.index_for_position(event.position) else {
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
