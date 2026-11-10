//! Text appearing line by line, drawn the way Telegram Desktop draws a
//! streamed bot reply (`Message::paintText` with `TextAppearing`): the
//! lines above the current one in full, the current line up to its
//! revealed width with the last [`GRADIENT`] px fading out, nothing below.
//!
//! The text is laid out in full, so no word moves while it appears. Its
//! paragraphs ([`super::selectable_text::SelectableRichText`]) report their
//! laid-out lines into a [`RevealSink`] as they prepaint; the reveal's
//! timing (`bot_stream::appear`) reads them on the next frame and says
//! what shows ([`RevealCut`]). The paragraphs pick the cut up while the
//! text block is built inside [`scope`].
#![deny(clippy::indexing_slicing)]

use gpui_kit::*;
use std::cell::RefCell;
use std::rc::Rc;

/// `st::textRevealGradient`: the soft edge of the revealed part.
pub(crate) const GRADIENT: f32 = 24.;
/// The soft edge is drawn as this many bands of falling opacity.
const BANDS: usize = 8;

/// One laid-out line of the appearing text, in the text block's
/// coordinates (logical px).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RevealLine {
    pub(crate) top: f32,
    pub(crate) bottom: f32,
    pub(crate) width: f32,
    pub(crate) rtl: bool,
}

/// What the text block's paragraphs laid out on the last frame.
#[derive(Debug, Default)]
pub(crate) struct RevealGeometry {
    /// The block's top left in the window, this frame.
    origin: Point<Pixels>,
    /// The block's full height: room below the last line (the time
    /// footer's own line) belongs to that line.
    height: f32,
    lines: Vec<RevealLine>,
    /// The room the text leaves at its end for the time footer
    /// (`skipBlockWidth`): the trailing spaces of its last line.
    skip: f32,
}

impl RevealGeometry {
    /// The lines, top to bottom.
    pub(crate) fn lines(&self) -> Vec<RevealLine> {
        let mut lines = self.lines.clone();
        lines.sort_by(|a, b| a.top.total_cmp(&b.top));
        if let Some(last) = lines.last_mut() {
            last.bottom = last.bottom.max(self.height);
        }
        lines
    }

    /// The footer's room at the end of the text.
    pub(crate) fn skip(&self) -> f32 {
        self.skip
    }
}

pub(crate) type RevealSink = Rc<RefCell<RevealGeometry>>;

/// How much of the text shows this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum RevealCut {
    /// All of it.
    All,
    /// The lines above `line` in full, `line` up to `revealed` px from its
    /// start edge, nothing below it.
    Line { line: RevealLine, revealed: f32 },
}

/// A text block appearing: what of it shows and how big it is drawn.
#[derive(Clone, Debug)]
pub(crate) struct TextReveal {
    pub(crate) paint: RevealPaint,
    pub(crate) height: Pixels,
    /// How wide the block is drawn (`shownWidth`: the bubble follows the
    /// revealed edge, the time footer just past it), when it is narrower
    /// than the text.
    pub(crate) width: Option<Pixels>,
}

/// The cut and where the paragraphs report their lines.
#[derive(Clone, Debug)]
pub(crate) struct RevealPaint {
    pub(crate) cut: RevealCut,
    pub(crate) sink: RevealSink,
}

thread_local! {
    static CURRENT: RefCell<Option<RevealPaint>> = const { RefCell::new(None) };
}

/// Build the text block in `f` with `paint` applying to its paragraphs.
pub(crate) fn scope<R>(paint: Option<RevealPaint>, f: impl FnOnce() -> R) -> R {
    let previous = CURRENT.with(|current| current.replace(paint));
    let result = f();
    CURRENT.with(|current| current.replace(previous));
    result
}

/// The text block `build` makes, drawn through `reveal` when it appears.
/// `wrap` is the width the text wraps at in the bubble: while the block
/// is drawn narrower, the text keeps that layout.
pub(crate) fn revealing(
    reveal: Option<TextReveal>,
    wrap: Pixels,
    build: impl FnOnce() -> AnyElement,
) -> AnyElement {
    match reveal {
        Some(reveal) => {
            let sink = reveal.paint.sink.clone();
            let child = scope(Some(reveal.paint), build);
            block(
                child,
                reveal.height,
                reveal.width.map(|width| (width, wrap)),
                sink,
            )
        }
        None => build(),
    }
}

/// The reveal of the text block being built, if any.
pub(crate) fn current() -> Option<RevealPaint> {
    CURRENT.with(|current| current.borrow().clone())
}

/// `child` (the text block) clipped to `height`, the bubble's open part,
/// and to `width` (shown, wrap) while that is narrower than the text,
/// noting each frame where the block starts so its paragraphs can report
/// their lines against it.
fn block(
    child: AnyElement,
    height: Pixels,
    width: Option<(Pixels, Pixels)>,
    sink: RevealSink,
) -> AnyElement {
    let inner = div().flex_none().child(RevealBlock { child, sink });
    let outer = div().flex().flex_col().overflow_hidden().h(height);
    match width {
        Some((shown, wrap)) => outer.w(shown).child(inner.w(wrap)),
        None => outer.child(inner),
    }
    .into_any_element()
}

struct RevealBlock {
    child: AnyElement,
    sink: RevealSink,
}

impl IntoElement for RevealBlock {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for RevealBlock {
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
        // A list may prepaint a row more than once a frame: each pass
        // starts the lines over.
        if let Ok(mut geometry) = self.sink.try_borrow_mut() {
            geometry.origin = bounds.origin;
            geometry.height = f32::from(bounds.size.height);
            geometry.lines.clear();
            geometry.skip = 0.;
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

/// One visual line of a paragraph, in window coordinates.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LaidLine {
    pub(crate) top: f32,
    pub(crate) bottom: f32,
    pub(crate) width: f32,
    /// How much of `width` is trailing spaces.
    pub(crate) trailing: f32,
}

/// The visual lines of `text` laid out in `layout` with its top left at
/// `origin`.
pub(crate) fn styled_lines(
    layout: &TextLayout,
    text: &str,
    origin: Point<Pixels>,
) -> Vec<LaidLine> {
    let line_height = f32::from(layout.line_height());
    let mut out = Vec::new();
    let mut top = f32::from(origin.y);
    let mut sources = text.split('\n');
    for line in layout.line_layouts() {
        let source = sources.next().unwrap_or_default();
        let unwrapped = &line.unwrapped_layout;
        let glyphs = || unwrapped.runs.iter().flat_map(|run| run.glyphs.iter());
        // Where each visual line starts (x, byte): a wrap boundary's glyph
        // begins the next line.
        let mut starts = vec![(0., 0)];
        for boundary in line.wrap_boundaries() {
            let glyph = unwrapped
                .runs
                .get(boundary.run_ix)
                .and_then(|run| run.glyphs.get(boundary.glyph_ix));
            starts.push(glyph.map_or((0., source.len()), |glyph| {
                (f32::from(glyph.position.x), glyph.index)
            }));
        }
        let end = (f32::from(unwrapped.width), source.len());
        for (index, (x, byte)) in starts.iter().enumerate() {
            let (next_x, next_byte) = starts.get(index + 1).copied().unwrap_or(end);
            // Where the line's trailing spaces start.
            let trimmed = source
                .get(*byte..next_byte)
                .map_or(next_byte, |part| byte + part.trim_end().len());
            let spaces_x = glyphs()
                .find(|glyph| glyph.index >= trimmed && glyph.index < next_byte)
                .map_or(next_x, |glyph| f32::from(glyph.position.x));
            out.push(LaidLine {
                top,
                bottom: top + line_height,
                width: (next_x - x).max(0.),
                trailing: (next_x - spaces_x).max(0.),
            });
            top += line_height;
        }
    }
    out
}

impl RevealPaint {
    /// Note a paragraph's lines (window coordinates) for the next frame.
    pub(crate) fn report(&self, lines: &[LaidLine], rtl: bool) {
        let Ok(mut geometry) = self.sink.try_borrow_mut() else {
            return;
        };
        let y = f32::from(geometry.origin.y);
        for line in lines {
            geometry.lines.push(RevealLine {
                top: line.top - y,
                bottom: line.bottom - y,
                width: line.width,
                rtl,
            });
            geometry.skip = line.trailing;
        }
    }

    /// Whether a box of the paragraph (an inline emoji) shows.
    pub(crate) fn shows(&self, rect: Bounds<Pixels>, paragraph: Bounds<Pixels>) -> bool {
        let RevealCut::Line { line, revealed } = self.cut else {
            return true;
        };
        let top = self.top_in_window(line.top);
        let bottom = self.top_in_window(line.bottom);
        if rect.bottom() <= top + px(0.5) {
            return true;
        }
        if rect.top() >= bottom - px(0.5) {
            return false;
        }
        if line.rtl {
            rect.left() >= paragraph.right() - px(revealed)
        } else {
            rect.right() <= paragraph.left() + px(revealed)
        }
    }

    fn top_in_window(&self, y: f32) -> Pixels {
        let origin = self.sink.try_borrow().map_or(px(0.), |g| g.origin.y);
        origin + px(y)
    }

    /// Paint a paragraph in `bounds` through the cut: `draw` paints its
    /// text and may run several times, each under a narrower mask.
    pub(crate) fn paint(
        &self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
        draw: &mut dyn FnMut(&mut Window, &mut App),
    ) {
        let RevealCut::Line { line, revealed } = self.cut else {
            draw(window, cx);
            return;
        };
        let top = self.top_in_window(line.top);
        let bottom = self.top_in_window(line.bottom);
        // The lines above the current one, in full.
        let above = top.min(bounds.bottom());
        if above > bounds.top() {
            let mask = Bounds::from_corners(bounds.origin, point(bounds.right(), above));
            masked(mask, None, window, cx, draw);
        }
        if bottom <= bounds.top() || top >= bounds.bottom() || revealed <= 0. {
            return;
        }
        // The current line up to its soft edge (`ApplyRevealGradient`).
        let gradient = GRADIENT.min(revealed);
        let solid = revealed - gradient;
        let span = |from: f32, to: f32| -> Bounds<Pixels> {
            let (left, right) = if line.rtl {
                (bounds.right() - px(to), bounds.right() - px(from))
            } else {
                (bounds.left() + px(from), bounds.left() + px(to))
            };
            Bounds::from_corners(point(left, top), point(right, bottom))
        };
        if solid > 0. {
            masked(span(0., solid), None, window, cx, draw);
        }
        let band = gradient / BANDS as f32;
        for index in 0..BANDS {
            let from = solid + band * index as f32;
            let opacity = 1. - (index as f32 + 0.5) / BANDS as f32;
            masked(span(from, from + band), Some(opacity), window, cx, draw);
        }
    }
}

fn masked(
    bounds: Bounds<Pixels>,
    opacity: Option<f32>,
    window: &mut Window,
    cx: &mut App,
    draw: &mut dyn FnMut(&mut Window, &mut App),
) {
    if bounds.size.width <= px(0.) || bounds.size.height <= px(0.) {
        return;
    }
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        window.with_element_opacity(opacity, |window| draw(window, cx));
    });
}
