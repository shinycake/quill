//! Single-line text with right-to-left characters: chat-list previews, search
//! results, reply and pinned strips (codex:rtl-polish).
//!
//! Telegram Desktop draws these with a one-line `Ui::Text::String`
//! (`TextDialogOptions`, `elisionLines = 1`, left alignment). `Text::String`
//! takes each paragraph's direction from its first strong character, and the
//! renderer treats "left" as the *start edge* of that paragraph
//! (`lib_ui/ui/text/text_renderer.cpp`: `Qt::AlignLeft` with a right-to-left
//! paragraph shifts the line by the free width), so a Hebrew preview rests
//! against the right end of its box; what does not fit is elided at the end of
//! the reading order, i.e. at the *left* end of a Hebrew line, not wherever the
//! glyphs run out.
//!
//! GPUI's own single line (`div().truncate()`) shapes the text as one platform
//! line, left-aligned, cut at its visual right end. This element lays text
//! that holds right-to-left characters out with the bidi engine of the vendored
//! `gpui-base` (`BidiParagraph::layout_one_line`: typing-order elision, the
//! same per-run shaping on every platform); everything else is GPUI's
//! `StyledText`, untouched.
#![deny(clippy::indexing_slicing)]

use super::selectable_text::{BidiSource, text_runs};
use gpui_kit::base::input::bidi_paragraph::BidiParagraph;
use gpui_kit::*;
use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;

/// Text on one line, ending in an ellipsis where it does not fit, laid out for its
/// direction. The caller's box (`truncate()` and a width) decides how much room there is.
/// Use for text whose highlights are plain [`HighlightStyle`]s.
pub(crate) fn one_line(
    text: impl Into<SharedString>,
    highlights: Vec<(Range<usize>, HighlightStyle)>,
    font_families: Vec<(Range<usize>, SharedString)>,
) -> AnyElement {
    let text: SharedString = text.into();
    if quill::text::has_rtl_text(&text) {
        return BidiLine {
            text,
            source: BidiSource {
                highlights,
                font_families,
            },
            runs: Rc::from([]),
            measured: Rc::default(),
            paragraph: None,
        }
        .into_any_element();
    }
    StyledText::new(text)
        .with_highlights(highlights)
        .with_font_family_overrides(font_families)
        .into_any_element()
}

/// [`one_line`] for plain text.
pub(crate) fn one_line_plain(text: impl Into<SharedString>) -> AnyElement {
    one_line(text, Vec::new(), Vec::new())
}

/// Whether a multi-line block of user text rests against the end edge: its first strong
/// character reads right to left. Telegram Desktop aligns captions, bubbles and poll text
/// by the text's own direction (`Ui::Text::String` paragraph direction); lists of names
/// stay start-aligned and use [`one_line_plain`] instead.
pub(crate) fn aligns_end(text: &str) -> bool {
    quill::text::is_rtl_text(text)
}

/// A wrapping block of user text (caption, description, poll question) aligned by the
/// text's own direction. The text keeps GPUI's per-line bidi shaping, so a mixed-direction
/// string is ordered correctly within each line.
pub(crate) fn aligned_block(text: impl Into<SharedString>) -> Div {
    let text: SharedString = text.into();
    let end = aligns_end(&text);
    let block = div().min_w_0();
    if end {
        block.w_full().text_right().child(text)
    } else {
        block.child(text)
    }
}

/// The element behind [`one_line`] for right-to-left text.
struct BidiLine {
    text: SharedString,
    source: BidiSource,
    /// The styled runs, from layout.
    runs: Rc<[TextRun]>,
    /// The room GPUI last measured the line in and the width it took there; prepaint
    /// draws the paragraph laid out for that room when the final width allows.
    measured: Rc<Cell<Option<(Pixels, Pixels)>>>,
    paragraph: Option<Rc<BidiParagraph>>,
}

fn metrics(window: &Window) -> (TextStyle, Pixels, Pixels) {
    let style = window.text_style();
    let font_size = style.font_size.to_pixels(window.rem_size());
    let line_height = window.pixel_snap(
        style
            .line_height
            .to_pixels(font_size.into(), window.rem_size()),
    );
    (style, font_size, line_height)
}

impl IntoElement for BidiLine {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for BidiLine {
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
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        _cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let (style, font_size, line_height) = metrics(window);
        self.runs = text_runs(&self.text, &style, &self.source).into();
        let runs = self.runs.clone();
        let text = self.text.clone();
        let measured = self.measured.clone();
        let layout_id = window.request_measured_layout(Default::default(), {
            move |known, available, window, _| {
                // Elide at the room offered; with none stated the line is whole.
                let limit = known
                    .width
                    .or(match available.width {
                        AvailableSpace::Definite(width) => Some(width),
                        _ => None,
                    })
                    .unwrap_or(px(1.0e6));
                let Some(measure) = super::paragraph_cache::measure_one_line(
                    &text,
                    &runs,
                    font_size,
                    line_height,
                    limit,
                    window,
                ) else {
                    return Size::default();
                };
                measured.set(Some((limit, measure.width)));
                size(measure.width, line_height)
            }
        });
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        let room = match self.measured.get() {
            // Laid out for this room, or whole and still fitting.
            Some((limit, width))
                if limit == bounds.size.width
                    || (width <= bounds.size.width && limit > bounds.size.width) =>
            {
                limit
            }
            _ => bounds.size.width,
        };
        let (_, font_size, line_height) = metrics(window);
        self.paragraph = super::paragraph_cache::layout_one_line(
            &self.text,
            &self.runs,
            font_size,
            line_height,
            room,
            window,
        );
    }

    fn paint(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(paragraph) = &self.paragraph {
            paragraph.paint(bounds.origin, bounds.size.width, window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::aligns_end;

    #[test]
    fn right_to_left_text_rests_at_the_end() {
        assert!(aligns_end(
            "\u{645}\u{631}\u{62d}\u{628}\u{627} \u{628}\u{627}\u{644}\u{639}\u{627}\u{644}\u{645}"
        ));
        assert!(aligns_end("\u{633}\u{644}\u{627}\u{645} hello"));
    }

    #[test]
    fn left_to_right_and_neutral_text_stays_at_the_start() {
        assert!(!aligns_end("hello \u{633}\u{644}\u{627}\u{645}"));
        assert!(!aligns_end("12:30 - 4.5"));
        assert!(!aligns_end(""));
    }
}
