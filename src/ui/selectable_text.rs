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

/// A custom emoji drawn inside the text: the shaped text carries one
/// [`EMOJI_PLACEHOLDER`] (invisible, em wide) at `range`, and the picture
/// is painted over the glyph's box after layout. The emoji therefore wraps
/// and aligns exactly like a word of the paragraph (tdesktop shapes its
/// emoji as inline objects of one text block the same way).
pub(super) struct InlineEmoji {
    /// Byte range of the placeholder in the paragraph text.
    pub(super) range: Range<usize>,
    pub(super) visual: InlineEmojiVisual,
}

pub(super) enum InlineEmojiVisual {
    /// A still image (a sticker's static frame).
    Image(ImageSource),
    /// An animated clip drawn by the history's animation layer.
    Clip(super::anim_layer::LayeredClip),
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
    selection_color: Hsla,
    document_order: u64,
    message: Option<(i64, u64)>,
    /// Hidden spoiler runs and their specks' opacity.
    spoilers: Vec<(Range<usize>, f32)>,
    /// The animation layer that draws the specks (`anim_layer`), if any.
    layer: Option<super::anim_layer::Layer>,
    /// Custom emoji painted over placeholder glyphs.
    emoji: Vec<InlineEmoji>,
    /// The paragraph is right-aligned (RTL), so visual lines start
    /// further right than `position_for_index` reports.
    align_right: bool,
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
            emoji: Vec::new(),
            align_right: false,
        }
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

    /// Lay the paragraph's lines out right-aligned.
    pub(super) fn align_right(mut self, right: bool) -> Self {
        self.align_right = right;
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

    fn clicked_range(&self, layout: &TextLayout, position: Point<Pixels>) -> Option<usize> {
        let index = layout.index_for_position(position).ok()?;
        self.click_ranges
            .iter()
            .position(|range| range.contains(&index))
    }
}

/// Where a visual line of `layout` starts: `position_for_index`, shifted
/// for right-aligned paragraphs by the line's slack (GPUI aligns each
/// wrapped line inside the element's width at paint time).
fn glyph_origin(
    layout: &TextLayout,
    text: &str,
    index: usize,
    width: Pixels,
    align_right: bool,
) -> Option<Point<Pixels>> {
    let position = layout.position_for_index(index)?;
    if !align_right {
        return Some(position);
    }
    let line_start = text[..index].rfind('\n').map_or(0, |at| at + 1);
    let wrapped = layout.line_layout_for_index(index)?;
    let within = index - line_start;
    let mut ends: Vec<usize> = wrapped
        .wrap_boundaries
        .iter()
        .map(|boundary| {
            wrapped.unwrapped_layout.runs[boundary.run_ix].glyphs[boundary.glyph_ix].index
        })
        .collect();
    ends.push(wrapped.len());
    let mut start = 0;
    for end in ends {
        if within > end {
            start = end;
            continue;
        }
        let line_width =
            wrapped.unwrapped_layout.x_for_index(end) - wrapped.unwrapped_layout.x_for_index(start);
        return Some(position + point((width - line_width).max(Pixels::ZERO), Pixels::ZERO));
    }
    Some(position)
}

/// The box a custom emoji with the placeholder at `range` is painted in:
/// the placeholder glyph's box, enlarged a little and centred on the line.
fn emoji_bounds(
    layout: &TextLayout,
    text: &str,
    range: &Range<usize>,
    bounds: Bounds<Pixels>,
    align_right: bool,
    font: Pixels,
) -> Option<Bounds<Pixels>> {
    let width = bounds.size.width;
    let start = glyph_origin(layout, text, range.start, width, align_right)?;
    let end = glyph_origin(layout, text, range.end, width, align_right)?;
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
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        let mut emoji = Vec::new();
        if !self.emoji.is_empty() {
            let layout = self.styled.layout().clone();
            let font = window.text_style().font_size.to_pixels(window.rem_size());
            for item in &self.emoji {
                let Some(rect) = emoji_bounds(
                    &layout,
                    &self.text,
                    &item.range,
                    bounds,
                    self.align_right,
                    font,
                ) else {
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
                            super::anim_layer::frames(clip.clone(), 30)
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
        for element in emoji.iter_mut() {
            element.paint(window, cx);
        }
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
    let text = TextSelection::selected_text(window, cx).replace(EMOJI_PLACEHOLDER, "");
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

#[cfg(all(test, feature = "demo-capture"))]
mod tests {
    use super::{EMOJI_PLACEHOLDER, SelectableRichText, emoji_bounds};
    use gpui_kit::component::Root;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        AppContext, Context, IntoElement, ParentElement, Render, SharedString, Styled, StyledText,
        TestAppContext, TextLayout, Window, div, px, size,
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

    /// The emoji box sits on its neighbours' line, inside the column, and
    /// is roughly text sized.
    #[test]
    fn emoji_box_sits_inline() {
        let text = format!("Hello {EMOJI_PLACEHOLDER} world");
        lay_out(&text, |layout, lines| {
            assert_eq!(lines, 1);
            let at = text.find(EMOJI_PLACEHOLDER).unwrap();
            let range = at..at + EMOJI_PLACEHOLDER.len_utf8();
            let rect = emoji_bounds(layout, &text, &range, layout.bounds(), false, px(14.))
                .expect("laid out");
            let line = layout.bounds();
            assert!(rect.origin.x >= line.origin.x && rect.right() <= line.right());
            let centre = rect.origin.y + rect.size.height / 2.;
            assert!((centre - (line.origin.y + layout.line_height() / 2.)).abs() < px(1.));
            assert!(rect.size.width >= px(7.) && rect.size.width <= px(24.));
        });
    }
}
