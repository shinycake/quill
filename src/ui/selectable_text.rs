//! Message text that takes part in the window's text selection, so part
//! of a message (or a span across messages) can be selected and copied, as
//! in Telegram Desktop. Modelled on `gpui_base::SelectableText`, which only
//! takes plain text: this element lays out a styled `StyledText`
//! (highlights for bold, links, spoilers…) and keeps the inline click
//! targets (links, spoilers) a plain `InteractiveText` had.
#![deny(clippy::indexing_slicing)]

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
    /// Custom emoji inside the current selection, by (document order,
    /// byte offset): their fallback text replaces the placeholders in the
    /// copied string; see [`selected_message_text`].
    static SELECTED_EMOJI: std::cell::RefCell<std::collections::BTreeMap<(u64, usize), String>> =
        const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
    static OWNERS: std::cell::RefCell<std::collections::HashMap<EntityId, (i64, u64)>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Called with the index of the clicked range.
type ClickHandler = Rc<dyn Fn(usize, &mut Window, &mut App)>;

/// Called with the index of the range under a right press and the press
/// position.
type PressHandler = Rc<dyn Fn(usize, Point<Pixels>, &mut Window, &mut App)>;

/// Called when the pointer enters a range (its index and the pointer
/// position) or leaves every range (`None`).
type HoverHandler = Rc<dyn Fn(Option<(usize, Point<Pixels>)>, &mut Window, &mut App)>;

/// A custom emoji drawn inside the text: the shaped text carries one
/// [`EMOJI_PLACEHOLDER`] (invisible, em wide) at `range`, and the picture
/// is painted over the glyph's box after layout. The emoji therefore wraps
/// and aligns exactly like a word of the paragraph (tdesktop shapes its
/// emoji as inline objects of one text block the same way).
pub(super) struct InlineEmoji {
    /// Byte range of the placeholder in the paragraph text.
    pub(super) range: Range<usize>,
    pub(super) visual: InlineEmojiVisual,
    /// The emoji's own characters (the entity's text), copied in place of
    /// the placeholder.
    pub(super) fallback: String,
}

pub(super) enum InlineEmojiVisual {
    /// A still image (a sticker's static frame).
    Image(ImageSource),
    /// An animated clip drawn by the history's animation layer.
    Clip(super::sticker_player::StickerView),
}

/// The text standing in for an inline custom emoji: an em space, which
/// every font has and which is a break opportunity only *after* it, so an
/// emoji never separates from the word before it.
pub(super) const EMOJI_PLACEHOLDER: char = '\u{2003}';

/// Edge of an inline custom emoji relative to its placeholder's width.
const EMOJI_SCALE: f32 = 1.1;

/// A press that moved less than this is a click, not a selection drag.
const CLICK_SLOP: f32 = 4.;

pub(super) struct SelectableRichText {
    id: ElementId,
    text: SharedString,
    styled: StyledText,
    click_ranges: Vec<Range<usize>>,
    on_click: Option<ClickHandler>,
    on_secondary_press: Option<PressHandler>,
    on_hover: Option<HoverHandler>,
    /// Per click range: underline it while the pointer is over it.
    underline_on_hover: Vec<bool>,
    underline_color: Hsla,
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
    /// Custom emoji painted over placeholder glyphs.
    emoji: Vec<InlineEmoji>,
    /// What the shaped text dropped from the message: a copy puts it back.
    source: Option<quill::display_text::SourceMap>,
}

/// Highlights and font overrides of a paragraph, as `StyledText` takes them.
#[derive(Clone)]
pub(super) struct BidiSource {
    pub(super) highlights: Vec<(Range<usize>, HighlightStyle)>,
    pub(super) font_families: Vec<(Range<usize>, SharedString)>,
}

/// Text runs for `text` under `style`: the highlights over the base style, then the
/// font-family overrides, the way `StyledText` resolves them at layout time.
pub(super) fn text_runs(text: &str, style: &TextStyle, source: &BidiSource) -> Vec<TextRun> {
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
            on_secondary_press: None,
            on_hover: None,
            underline_on_hover: Vec::new(),
            underline_color: gpui_kit::hsla(0., 0., 0., 0.),
            selection_color: gpui_kit::hsla(0.58, 0.8, 0.6, 0.35),
            document_order: 0,
            message: None,
            spoilers: Vec::new(),
            layer: None,
            bidi_source: None,
            paragraph: None,
            emoji: Vec::new(),
            source: None,
        }
    }

    /// Copy and quote take the message's own text: what `map` says the
    /// shaped text dropped comes back.
    pub(super) fn source_map(mut self, map: quill::display_text::SourceMap) -> Self {
        self.source = Some(map);
        self
    }

    /// Custom emoji drawn over their placeholder glyphs (see
    /// [`InlineEmoji`]); animated ones register with the current layer.
    pub(super) fn inline_emoji(mut self, emoji: Vec<InlineEmoji>) -> Self {
        if !emoji.is_empty() {
            self.layer = super::anim_layer::current();
        }
        self.emoji = emoji;
        self
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

    /// A right press over one of the click ranges (the link context
    /// menu); runs in the capture phase, before the row's own handler.
    pub(super) fn on_secondary_press(
        mut self,
        handler: impl Fn(usize, Point<Pixels>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_secondary_press = Some(Rc::new(handler));
        self
    }

    /// Underline the click ranges flagged in `flags` (one per range, in
    /// `color`) while the pointer is over them, as Telegram Desktop does
    /// for links.
    pub(super) fn link_underline(mut self, flags: Vec<bool>, color: Hsla) -> Self {
        self.underline_on_hover = flags;
        self.underline_color = color;
        self
    }

    /// The pointer entering or leaving a click range (the link tooltip).
    pub(super) fn on_range_hover(
        mut self,
        handler: impl Fn(Option<(usize, Point<Pixels>)>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_hover = Some(Rc::new(handler));
        self
    }

    pub(super) fn selection_color(mut self, color: Hsla) -> Self {
        self.selection_color = color;
        self
    }

    /// The laid-out text, from whichever path made it (after prepaint).
    fn laid(&self, bounds: Bounds<Pixels>) -> Laid {
        match &self.paragraph {
            Some(paragraph) => Laid::Bidi {
                paragraph: paragraph.clone(),
                bounds,
            },
            None => Laid::Styled {
                layout: self.styled.layout().clone(),
                text: self.text.clone(),
            },
        }
    }
}

/// The laid-out text a paragraph reads positions from, whichever path made it.
#[derive(Clone)]
enum Laid {
    Styled {
        layout: TextLayout,
        text: SharedString,
    },
    Bidi {
        paragraph: Rc<BidiParagraph>,
        bounds: Bounds<Pixels>,
    },
}

impl Laid {
    fn index_for_position(&self, position: Point<Pixels>) -> Option<usize> {
        match self {
            Laid::Styled { layout, .. } => layout.index_for_position(position).ok(),
            Laid::Bidi { paragraph, bounds } => paragraph
                .index_for_position(position - bounds.origin, bounds.size.width)
                .ok(),
        }
    }

    /// Rectangles covering the byte `range`, one per row (and per visual piece),
    /// each only as wide as the glyphs it covers.
    fn range_rects(&self, range: Range<usize>) -> Vec<Bounds<Pixels>> {
        match self {
            Laid::Styled { layout, text } => super::text_rows::range_rects(layout, text, range),
            Laid::Bidi { paragraph, bounds } => paragraph
                .range_rects(range, bounds.size.width)
                .into_iter()
                .map(|rect| Bounds::new(rect.origin + bounds.origin, rect.size))
                .collect(),
        }
    }

    fn geometry(&self) -> Rc<dyn RunGeometry> {
        match self {
            Laid::Styled { layout, .. } => Rc::new(layout.clone()),
            Laid::Bidi { paragraph, bounds } => {
                paragraph.geometry(bounds.origin, bounds.size.width)
            }
        }
    }
}

/// The box for an emoji whose placeholder glyph occupies `rect` (one text
/// row): the glyph's box, enlarged a little and centred on the row.
fn emoji_box(rect: Bounds<Pixels>, font: Pixels) -> Bounds<Pixels> {
    let advance = rect.size.width;
    let edge = advance.max(font * 0.5) * EMOJI_SCALE;
    let origin = point(
        rect.origin.x - (edge - advance) / 2.,
        rect.origin.y + (rect.size.height - edge) / 2.,
    );
    Bounds::new(origin, size(edge, edge))
}

/// The box a custom emoji with the placeholder at `range` is painted in:
/// the placeholder glyph's box, enlarged a little and centred on the line.
fn emoji_bounds(layout: &TextLayout, range: &Range<usize>, font: Pixels) -> Option<Bounds<Pixels>> {
    let start = layout.position_for_index(range.start)?;
    let end = layout.position_for_index(range.end)?;
    let line_height = layout.line_height();
    // The glyph's own advance when it sits on one line; at a wrap point
    // `position_for_index` reports the previous line's end for `start`, so
    // the glyph is the `font`-wide box just before `end`.
    let (left, top, advance) = if (end.y - start.y).abs() < px(0.5) && end.x > start.x {
        (start.x, start.y, end.x - start.x)
    } else {
        (end.x - font, end.y, font)
    };
    let edge = advance.max(font * 0.5) * EMOJI_SCALE;
    let origin = point(
        left - (edge - advance) / 2.,
        top + (line_height - edge) / 2.,
    );
    Some(Bounds::new(origin, size(edge, edge)))
}

impl IntoElement for SelectableRichText {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SelectableRichText {
    type RequestLayoutState = TextSelectionHandle;
    type PrepaintState = (Hitbox, Vec<AnyElement>);

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
        // `id()` is always `Some`, so GPUI always passes a global id.
        let handle = match global_id {
            Some(global_id) => window.with_element_state(
                global_id,
                |retained: Option<(TextSelectionHandle, Subscription)>, window| {
                    let (handle, refresh) = retained.unwrap_or_else(|| {
                        let handle = TextSelectionHandle::new(self.text.clone(), cx);
                        let refresh = handle.refresh_window_on_change(window, cx);
                        (handle, refresh)
                    });
                    (handle.clone(), (handle, refresh))
                },
            ),
            None => TextSelectionHandle::new(self.text.clone(), cx),
        };
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
            // Measures go through the shared paragraph cache: taffy asks
            // for several widths per frame and the row is rebuilt every
            // frame, so a per-element slot would shape the text again.
            // A zero minimum width lets the paragraph shrink to its row and
            // wrap there. Its min-content is one unwrapped line, so with the
            // default minimum a right-aligned (flex) row lets a long Hebrew
            // paragraph spill out of the bubble instead of wrapping.
            let mut layout_style = Style::default();
            layout_style.min_size.width = px(0.).into();
            let layout_id = window.request_measured_layout(layout_style, {
                move |known, available, window, _| {
                    let wrap = known.width.or(match available.width {
                        AvailableSpace::Definite(width) => Some(width),
                        _ => None,
                    });
                    super::paragraph_cache::measure(
                        &text,
                        &runs,
                        font_size,
                        line_height,
                        wrap,
                        window,
                    )
                    .unwrap_or_default()
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
            let style = window.text_style();
            let font_size = style.font_size.to_pixels(window.rem_size());
            let line_height = window.pixel_snap(
                style
                    .line_height
                    .to_pixels(font_size.into(), window.rem_size()),
            );
            let runs = text_runs(&self.text, &style, &source);
            self.paragraph = super::paragraph_cache::layout(
                &self.text,
                &runs,
                font_size,
                line_height,
                bounds.size.width,
                window,
            );
        } else {
            self.styled
                .prepaint(global_id, inspector_id, bounds, &mut (), window, cx);
        }
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        let mut emoji = Vec::new();
        if !self.emoji.is_empty() {
            let font = window.text_style().font_size.to_pixels(window.rem_size());
            let laid = self.laid(bounds);
            for item in &self.emoji {
                let rect = match &laid {
                    // Visual positions of the placeholder, wherever the
                    // bidirectional layout put it.
                    Laid::Bidi { .. } => laid
                        .range_rects(item.range.clone())
                        .first()
                        .map(|rect| emoji_box(*rect, font)),
                    Laid::Styled { layout, .. } => emoji_bounds(layout, &item.range, font),
                };
                let Some(rect) = rect else {
                    continue;
                };
                let mut element = match &item.visual {
                    InlineEmojiVisual::Image(source) => img(source.clone())
                        .size(rect.size.width)
                        .object_fit(ObjectFit::Contain)
                        .with_fallback(|| div().into_any_element())
                        .into_any_element(),
                    InlineEmojiVisual::Clip(clip) => {
                        super::anim_layer::with_layer(self.layer.as_ref(), || {
                            super::anim_layer::frames(clip.clone())
                                .flex_none()
                                .size(rect.size.width)
                                .into_any_element()
                        })
                    }
                };
                element.prepaint_as_root(rect.origin, rect.size.map(Into::into), window, cx);
                emoji.push(element);
            }
        }
        (hitbox, emoji)
    }

    fn paint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        handle: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let (hitbox, emoji) = prepaint;
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
        let layout = self.laid(bounds);
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
        if let Some(map) = self.source.take() {
            let (text, ranges) = (self.text.clone(), projection.ranges().to_vec());
            handle.copy_with(
                move |_| {
                    ranges
                        .iter()
                        .flatten()
                        .filter_map(|range| map.source_of(&text, range.clone()))
                        .collect()
                },
                cx,
            );
        }
        if selected_before != TextSelection::selected_text(window, cx) {
            window.refresh();
        }
        if !self.emoji.is_empty() || SELECTED_EMOJI.with(|map| !map.borrow().is_empty()) {
            let order = self.document_order;
            let selected = selected_fallbacks(&self.emoji, projection.ranges());
            SELECTED_EMOJI.with(|map| {
                let mut map = map.borrow_mut();
                map.retain(|(o, _), _| *o != order);
                for (at, fallback) in selected {
                    map.insert((order, at), fallback);
                }
            });
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
        for element in emoji.iter_mut() {
            element.paint(window, cx);
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

        if let Some(handler) = self.on_secondary_press.clone() {
            let ranges = self.click_ranges.clone();
            let press_hitbox = hitbox.clone();
            let layout = layout.clone();
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                if phase != DispatchPhase::Capture
                    || event.button != MouseButton::Right
                    || !press_hitbox.is_hovered(window)
                {
                    return;
                }
                if let Some(index) = layout.index_for_position(event.position)
                    && let Some(ix) = ranges.iter().position(|range| range.contains(&index))
                {
                    handler(ix, event.position, window, cx);
                }
            });
        }
        if let Some(handler) = self.on_hover.clone() {
            let ranges = self.click_ranges.clone();
            let move_hitbox = hitbox.clone();
            let layout = layout.clone();
            let over: Rc<Cell<Option<usize>>> = retained_cell(global_id, window);
            if let Some(ix) = over.get()
                && self.underline_on_hover.get(ix).copied().unwrap_or(false)
                && let Some(range) = self.click_ranges.get(ix)
            {
                for quad in layout.range_rects(range.clone()) {
                    window.paint_quad(fill(
                        Bounds::new(
                            point(quad.origin.x, quad.bottom() - px(2.)),
                            size(quad.size.width, px(1.)),
                        ),
                        self.underline_color,
                    ));
                }
            }
            let hover_cell = over.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                let over = &hover_cell;
                if phase != DispatchPhase::Bubble {
                    return;
                }
                let now = if move_hitbox.is_hovered(window) {
                    layout
                        .index_for_position(event.position)
                        .and_then(|index| ranges.iter().position(|range| range.contains(&index)))
                } else {
                    None
                };
                if now != over.get() {
                    over.set(now);
                    handler(now.map(|ix| (ix, event.position)), window, cx);
                    window.refresh();
                }
            });
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
            let pressed_at: Rc<Cell<Option<Point<Pixels>>>> = retained_cell(global_id, window);
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

/// A per-element cell kept across frames. `SelectableRichText::id` is always
/// `Some`, so GPUI passes a global id; without one the cell lives one frame.
fn retained_cell<T: Default + 'static>(
    global_id: Option<&GlobalElementId>,
    window: &mut Window,
) -> Rc<T> {
    let Some(global_id) = global_id else {
        return Rc::default();
    };
    window.with_element_state(global_id, |retained: Option<Rc<T>>, _| {
        let cell = retained.unwrap_or_default();
        (cell.clone(), cell)
    })
}

/// The fallback text of the custom emoji whose placeholder lies within one
/// of the selected `ranges`, with the placeholder's byte offset.
fn selected_fallbacks(
    emoji: &[InlineEmoji],
    ranges: &[Option<Range<usize>>],
) -> Vec<(usize, String)> {
    emoji
        .iter()
        .filter(|item| {
            ranges
                .iter()
                .flatten()
                .any(|r| r.start <= item.range.start && item.range.end <= r.end)
        })
        .map(|item| (item.range.start, item.fallback.clone()))
        .collect()
}

/// `selected` with its first `fallbacks.len()` placeholders replaced by
/// the emoji's own text, in order; any further placeholder characters are
/// the bubble footer's reserved space and are dropped.
fn with_emoji_fallbacks(selected: &str, fallbacks: &[String]) -> String {
    let mut out = String::with_capacity(selected.len());
    let mut next = fallbacks.iter();
    for ch in selected.chars() {
        if ch == EMOJI_PLACEHOLDER {
            if let Some(fallback) = next.next() {
                out.push_str(fallback);
            }
        } else {
            out.push(ch);
        }
    }
    out
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
    let selected = TextSelection::selected_text(window, cx);
    let message = SELECTED_MESSAGE.with(Cell::get)?;
    let fallbacks: Vec<String> = SELECTED_EMOJI.with(|map| {
        map.borrow()
            .iter()
            .filter(|((order, _), _)| order >> 10 == message.1)
            .map(|(_, fallback)| fallback.clone())
            .collect()
    });
    let text = with_emoji_fallbacks(&selected, &fallbacks);
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    Some((message, text.to_string()))
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

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod tests {
    use super::{
        EMOJI_PLACEHOLDER, SelectableRichText, emoji_bounds, selected_message_text,
        with_emoji_fallbacks,
    };
    use gpui_kit::component::Root;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        AppContext, Context, IntoElement, Modifiers, MouseButton, ParentElement, Render,
        SharedString, Styled, StyledText, TestAppContext, TextLayout, Window, div, point, px, size,
    };
    use std::cell::RefCell;
    use std::rc::Rc;

    struct Paragraph {
        text: SharedString,
        layout: Rc<RefCell<Option<TextLayout>>>,
    }

    impl Render for Paragraph {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let styled = StyledText::new(self.text.clone());
            *self.layout.borrow_mut() = Some(styled.layout().clone());
            div()
                .w(px(300.))
                .text_size(px(14.))
                .child(SelectableRichText::new("par", self.text.clone(), styled))
        }
    }

    /// Lays `text` out in a 300px column and hands the layout and its
    /// number of visual lines to `check`.
    fn lay_out(text: &str, check: impl FnOnce(&TextLayout, usize)) {
        let mut cx = TestAppContext::single();
        cx.update(gpui_kit::init);
        let slot = Rc::new(RefCell::new(None));
        let handle = cx.open_window(size(px(320.), px(400.)), {
            let slot = slot.clone();
            let text: SharedString = text.to_string().into();
            move |window, cx| {
                let view = cx.new(|_| Paragraph { text, layout: slot });
                Root::new(view, window, cx)
            }
        });
        cx.update_window(handle.into(), |_, window, cx| {
            window.render_frame(cx);
            window.render_frame(cx);
        })
        .expect("window rendered");
        let layout = slot.borrow().clone().expect("rendered");
        let lines = (layout.bounds().size.height / layout.line_height()).round() as usize;
        check(&layout, lines);
        cx.quit();
    }

    /// One flowing paragraph: a placeholder glyph in the middle of a long
    /// sentence adds no line break, so the line count equals that of the
    /// same sentence with a plain character there.
    #[test]
    fn placeholder_flows_like_a_character() {
        let sentence = |mid: &str| {
            format!(
                "This week alone we distributed{mid}$222,000 among the winners of our two latest competitions, the Design Contest and the Digital Freedom Contest, worth more than $2 million."
            )
        };
        let mut expected = 0;
        lay_out(&sentence(" x "), |_, lines| expected = lines);
        assert!(expected >= 2, "the sentence wraps");
        let with_emoji = sentence(&format!(" {EMOJI_PLACEHOLDER} "));
        lay_out(&with_emoji, |_, lines| assert_eq!(lines, expected));
    }

    /// Copying maps each placeholder back to the emoji's own characters;
    /// the footer's reserved space (more placeholder characters) is dropped.
    #[test]
    fn copy_restores_emoji_text() {
        let p = EMOJI_PLACEHOLDER;
        let copied = format!("hi {p} and {p}!{p}{p}");
        let fallbacks = vec!["\u{1F3A8}".to_string(), "\u{1F680}".to_string()];
        assert_eq!(
            with_emoji_fallbacks(&copied, &fallbacks),
            "hi \u{1F3A8} and \u{1F680}!"
        );
        assert_eq!(with_emoji_fallbacks(&format!("a{p}{p}"), &[]), "a");
    }

    /// The emoji box sits on its neighbours' line, inside the column, and
    /// is roughly text sized.
    #[test]
    fn emoji_box_sits_inline() {
        let text = format!("Hello {EMOJI_PLACEHOLDER} world");
        lay_out(&text, |layout, lines| {
            assert_eq!(lines, 1);
            let at = text.find(EMOJI_PLACEHOLDER).unwrap();
            let range = at..at + EMOJI_PLACEHOLDER.len_utf8();
            let rect = emoji_bounds(layout, &range, px(14.)).expect("laid out");
            let line = layout.bounds();
            assert!(rect.origin.x >= line.origin.x && rect.right() <= line.right());
            let centre = rect.origin.y + rect.size.height / 2.;
            assert!((centre - (line.origin.y + layout.line_height() / 2.)).abs() < px(1.));
            assert!(rect.size.width >= px(7.) && rect.size.width <= px(24.));
        });
    }

    /// A link that wraps is underlined only under its own glyphs on every
    /// row: the first row stops at its last glyph, not the column's edge.
    #[test]
    fn wrapped_range_rects_hug_each_row() {
        let url = "https://locations.example.com/ll/us/tx/austin/13096-research-blvd/suite-a/order";
        let text = format!("{url} now");
        lay_out(&text, |layout, lines| {
            assert!(lines >= 2, "the link wraps");
            let start = text.find("https").unwrap_or(0);
            let rects =
                super::super::text_rows::range_rects(layout, &text, start..start + url.len());
            assert_eq!(rects.len(), lines, "one rect per row");
            let edge = layout.bounds().right();
            let mut covered = px(0.);
            for (ix, rect) in rects.iter().enumerate() {
                assert!(rect.size.width > px(0.) && rect.right() <= edge);
                assert!(
                    (rect.origin.y - (layout.bounds().top() + layout.line_height() * ix as f32))
                        .abs()
                        < px(0.5)
                );
                covered += rect.size.width;
            }
            // Together the rows are as wide as the URL set on one line.
            let first = rects.first().map_or(px(0.), |rect| rect.left());
            let start_x = layout.position_for_index(start).map_or(px(0.), |p| p.x);
            assert!((first - start_x).abs() < px(0.5));
            let one_line = layout
                .line_layout_for_index(start)
                .map(|line| {
                    line.unwrapped_layout.x_for_index(start + url.len())
                        - line.unwrapped_layout.x_for_index(start)
                })
                .unwrap_or_default();
            assert!(
                (covered - one_line).abs() < px(1.),
                "{covered:?} vs {one_line:?}"
            );
        });
    }

    struct Quoted {
        text: SharedString,
        map: quill::display_text::SourceMap,
        layout: Rc<RefCell<Option<TextLayout>>>,
    }

    impl Render for Quoted {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let styled = StyledText::new(self.text.clone());
            *self.layout.borrow_mut() = Some(styled.layout().clone());
            div().w(px(300.)).text_size(px(14.)).child(
                SelectableRichText::new("quoted", self.text.clone(), styled)
                    .message((1, 1))
                    .source_map(self.map.clone()),
            )
        }
    }

    /// A drag over text that lost a third mark and stray tags copies, and
    /// quotes, the message's own characters.
    #[gpui_kit::test]
    fn copy_and_quote_give_back_what_shaping_dropped(cx: &mut TestAppContext) {
        let source = "a book \u{1F4DA}\u{E0069}\u{E007F} and x\u{0300}\u{0300}\u{0300} end";
        let (mut shown, mut map) = (String::new(), quill::display_text::SourceMap::default());
        quill::display_text::push_filtered(
            &mut shown,
            source,
            &mut quill::display_text::FilterState::default(),
            &mut map,
        );
        assert_ne!(shown, source, "something was dropped");
        cx.update(gpui_kit::init);
        let slot = Rc::new(RefCell::new(None));
        let handle = cx.open_window(size(px(320.), px(200.)), {
            let (slot, text) = (slot.clone(), SharedString::from(shown.clone()));
            move |window, cx| {
                let view = cx.new(|_| Quoted {
                    text,
                    map,
                    layout: slot,
                });
                Root::new(view, window, cx)
            }
        });
        let mut vcx = gpui_kit::VisualTestContext::from_window(handle.into(), cx);
        vcx.update(|window, cx| window.render_frame(cx));
        let layout = slot.borrow().clone().expect("rendered");
        let half = layout.line_height() / 2.;
        let start = layout.position_for_index(0).expect("first character");
        let end = layout.position_for_index(shown.len()).expect("text end");
        // Both ends inside the text: past its box the drag scrolls.
        let (from, to) = (point(start.x, start.y + half), point(end.x, end.y + half));
        let none = Modifiers::none();
        vcx.simulate_mouse_move(from, None, none);
        vcx.simulate_mouse_down(from, MouseButton::Left, none);
        vcx.simulate_mouse_move(to, MouseButton::Left, none);
        vcx.simulate_mouse_up(to, MouseButton::Left, none);
        vcx.update(|window, cx| window.render_frame(cx));
        let copied = vcx.update(selected_message_text);
        assert_eq!(copied, Some(((1, 1), source.to_string())));
        let message = quill::quote_source::QuotableText::plain(source);
        assert!(
            message.quote_for(&shown).is_none(),
            "the shaped text is not a quote"
        );
        let quote = message
            .quote_for(source)
            .expect("the copy is a verbatim quote");
        assert_eq!(quote.text, source);
    }
}
