// Added by the Quill project (2026) to gpui-base 0.7.1; licensed Apache-2.0
// like the rest of this crate. See third_party/gpui-base/QUILL-CHANGES.md.
//! Formatting spans: named styles over UTF-8 byte ranges of the text, kept as
//! part of the document.
//!
//! This is the model Telegram Desktop's `InputField` keeps as `TextWithTags`
//! (a `QTextDocument` whose fragments carry a tag property): the text holds no
//! markers, a span says "bytes 4..9 are bold". Spans may overlap (bold and
//! italic over the same word) and different tags never merge. Edits move them
//! (see [`spans_after_edit`]), undo and redo restore them, copy and paste carry
//! them inside the input, and [`InputContent`](super::InputContent) round-trips
//! them with the text.
//!
//! What a tag looks like is presentation, not document: the view installs a
//! [`TextSpanStyler`] that turns a tag into a [`TextSpanStyle`]. Spans whose
//! style changes the font (weight, slant, family) are also handed to the
//! wrapper, so a bold word is measured bold when lines are wrapped.
use std::{ops::Range, rc::Rc};

use gpui::{
    App, ClipboardEntry, ClipboardItem, Context, EntityInputHandler as _, FontStyle, FontWeight,
    HighlightStyle, SharedString, Window,
};
use serde::{Deserialize, Serialize};

use super::{
    InlineToken, InlineTokenError, InputBaseState, InputContent, InputEvent, InputModeKind,
    change::Change, undo_manager::EditIntent,
};

/// One formatting span: `tag` over a half-open UTF-8 byte range of the text.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TextSpan {
    range: Range<usize>,
    tag: SharedString,
}

impl TextSpan {
    pub fn new(range: Range<usize>, tag: impl Into<SharedString>) -> Self {
        Self {
            range,
            tag: tag.into(),
        }
    }
    pub fn range(&self) -> Range<usize> {
        self.range.clone()
    }
    pub fn tag(&self) -> &SharedString {
        &self.tag
    }
    fn shifted(&self, delta: isize) -> Option<Self> {
        Some(Self {
            range: self.range.start.checked_add_signed(delta)?
                ..self.range.end.checked_add_signed(delta)?,
            tag: self.tag.clone(),
        })
    }
}

/// How a tag is drawn: a GPUI highlight (weight, slant, colour, background,
/// underline, strikethrough) plus an optional font family, which a highlight
/// cannot carry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextSpanStyle {
    pub highlight: HighlightStyle,
    pub font_family: Option<SharedString>,
}

/// Turns a span tag into its style; `None` draws the tag as plain text.
pub type TextSpanStyler = Rc<dyn Fn(&str) -> Option<TextSpanStyle>>;

/// The part of a span style that changes glyph advances, for line wrapping.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct FontChange {
    pub(crate) weight: Option<FontWeight>,
    pub(crate) style: Option<FontStyle>,
    pub(crate) family: Option<SharedString>,
}

impl FontChange {
    pub(crate) fn of(style: &TextSpanStyle) -> Option<Self> {
        let change = Self {
            weight: style.highlight.font_weight,
            style: style.highlight.font_style,
            family: style.font_family.clone(),
        };
        (change != Self::default()).then_some(change)
    }

    /// `font` with this change applied.
    pub(crate) fn apply(&self, font: &mut gpui::Font) {
        if let Some(weight) = self.weight {
            font.weight = weight;
        }
        if let Some(style) = self.style {
            font.style = style;
        }
        if let Some(family) = &self.family {
            font.family = family.clone();
        }
    }
}

/// Font changes over absolute byte ranges, sorted by start.
pub(crate) type SpanFonts = Rc<[(Range<usize>, FontChange)]>;

/// Undo record of an edit that changed the spans: the whole span list before
/// and after it. A composer holds a handful of spans, so a snapshot is cheaper
/// and simpler than a delta, and survives coalescing (merge keeps the first
/// `before` and the last `after`).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct SpanSnapshot {
    pub(super) before: Rc<[TextSpan]>,
    pub(super) after: Rc<[TextSpan]>,
}

impl SpanSnapshot {
    /// One snapshot for two consecutive changes.
    pub(super) fn merge(
        first: Option<Box<SpanSnapshot>>,
        second: Option<Box<SpanSnapshot>>,
    ) -> Option<Box<SpanSnapshot>> {
        match (first, second) {
            (Some(first), Some(second)) => Some(Box::new(SpanSnapshot {
                before: first.before,
                after: second.after,
            })),
            (first, second) => first.or(second),
        }
    }
}

/// The tags the next text inserted at `offset` takes, set while the document
/// stands at `revision` (Telegram Desktop's cursor char format: after a
/// markdown replacement the caret types plain text, a formatting toggle with
/// nothing selected makes the next typed text bold).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct TypingSpans {
    pub(super) offset: usize,
    pub(super) revision: u64,
    pub(super) tags: Vec<SharedString>,
}

/// What the text inserted by an edit is tagged with.
pub(super) enum Inserted<'a> {
    /// Telegram Desktop's rules: text typed inside a span joins it; text typed
    /// at a span's end continues it over its leading letters and digits only
    /// (`breakTagOnNotLetter`), so a space or punctuation ends the span.
    Inherit,
    /// Exactly these spans, relative to the inserted text, and nothing else.
    Exactly(&'a [TextSpan]),
}

/// The byte length of the leading letters and digits of `text`.
fn word_prefix_len(text: &str) -> usize {
    text.char_indices()
        .find(|(_, c)| !c.is_alphanumeric())
        .map_or(text.len(), |(ix, _)| ix)
}

/// The spans after `range` of the text was replaced by `inserted`.
pub(super) fn spans_after_edit(
    spans: &[TextSpan],
    range: &Range<usize>,
    inserted: &str,
    tags: Inserted<'_>,
) -> Vec<TextSpan> {
    let ins = inserted.len();
    let shift = ins as isize - range.len() as isize;
    let extend = match tags {
        Inserted::Inherit => word_prefix_len(inserted),
        Inserted::Exactly(_) => 0,
    };
    let mut out = Vec::with_capacity(spans.len() + 1);
    for span in spans {
        let (start, end) = (span.range.start, span.range.end);
        if end <= range.start {
            // Before the edit; text typed right at its end may continue it.
            let end = if end == range.start && end > start {
                end + extend
            } else {
                end
            };
            out.push(TextSpan::new(start..end, span.tag.clone()));
        } else if start >= range.end {
            // After the edit (also: an insertion right at its start, which
            // never joins it).
            if let Some(span) = span.shifted(shift) {
                out.push(span);
            }
        } else {
            // The edit replaces part of the span or inserts inside it. The
            // inserted text joins the span when the span holds the first
            // replaced character (or the insertion point).
            let inherits = start <= range.start && ins > 0;
            let new_start = if start <= range.start {
                start
            } else {
                range.start + ins
            };
            let new_end = if end > range.end {
                (end as isize + shift) as usize
            } else if inherits {
                range.start + ins
            } else {
                range.start
            };
            if new_start < new_end {
                out.push(TextSpan::new(new_start..new_end, span.tag.clone()));
            }
        }
    }
    if let Inserted::Exactly(inserted_spans) = tags {
        let at = range.start..range.start + ins;
        out = carve(&out, &at);
        out.extend(
            inserted_spans
                .iter()
                .filter(|span| span.range.end <= ins)
                .filter_map(|span| span.shifted(range.start as isize)),
        );
    }
    normalize(&mut out);
    out
}

/// `spans` with `range` cut out of every span (a span across it splits in two).
pub(super) fn carve(spans: &[TextSpan], range: &Range<usize>) -> Vec<TextSpan> {
    if range.is_empty() {
        return spans.to_vec();
    }
    let mut out = Vec::with_capacity(spans.len() + 1);
    for span in spans {
        if span.range.end <= range.start || span.range.start >= range.end {
            out.push(span.clone());
            continue;
        }
        if span.range.start < range.start {
            out.push(TextSpan::new(
                span.range.start..range.start,
                span.tag.clone(),
            ));
        }
        if span.range.end > range.end {
            out.push(TextSpan::new(range.end..span.range.end, span.tag.clone()));
        }
    }
    out
}

/// Drop empty spans, sort by start, and merge overlapping or touching spans of
/// the same tag.
pub(super) fn normalize(spans: &mut Vec<TextSpan>) {
    spans.retain(|span| span.range.start < span.range.end);
    spans.sort_by(|a, b| {
        (&a.tag, a.range.start, a.range.end).cmp(&(&b.tag, b.range.start, b.range.end))
    });
    let mut merged: Vec<TextSpan> = Vec::with_capacity(spans.len());
    for span in spans.drain(..) {
        match merged.last_mut() {
            Some(last) if last.tag == span.tag && span.range.start <= last.range.end => {
                last.range.end = last.range.end.max(span.range.end);
            }
            _ => merged.push(span),
        }
    }
    merged.sort_by(|a, b| {
        (a.range.start, a.range.end, &a.tag).cmp(&(b.range.start, b.range.end, &b.tag))
    });
    *spans = merged;
}

/// Spans clipped to a text of `len` bytes whose character boundaries are
/// answered by `is_boundary`, then normalized.
pub(super) fn clip_spans(
    spans: Vec<TextSpan>,
    len: usize,
    is_boundary: impl Fn(usize) -> bool,
) -> Vec<TextSpan> {
    let mut out: Vec<TextSpan> = spans
        .into_iter()
        .filter(|span| !span.tag.is_empty())
        .filter_map(|mut span| {
            let mut start = span.range.start.min(len);
            let mut end = span.range.end.min(len);
            while start > 0 && !is_boundary(start) {
                start -= 1;
            }
            while end < len && !is_boundary(end) {
                end += 1;
            }
            span.range = start..end;
            (start < end).then_some(span)
        })
        .collect();
    normalize(&mut out);
    out
}

/// What the clipboard carries for a copy from an input that has spans or
/// tokens, so a paste into an input restores them. Ranges are relative to
/// `text`, which must equal the clipboard text for the metadata to apply.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct RichClipboard {
    pub(super) text: String,
    pub(super) spans: Vec<TextSpan>,
    /// `(start, end, id, text, label)` of each token.
    pub(super) tokens: Vec<(usize, usize, String, String, String)>,
}

/// Which rows the wrapper wraps again after the span fonts changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SpanRewrap {
    /// None: an edit that rewraps its own rows right after.
    None,
    /// The rows of font entries that changed.
    Changed,
    /// Every row an old or a new entry touches.
    Touched,
}

impl<M: InputModeKind> InputBaseState<M> {
    /// Whether spans are drawn: never for masked text.
    pub(super) fn spans_shown(&self) -> bool {
        !self.masked && !self.text_spans.is_empty()
    }

    /// The font changes of the current spans, for the wrapper.
    fn span_fonts(&self) -> SpanFonts {
        let Some(styler) = self.span_styler.as_ref().filter(|_| !self.masked) else {
            return Rc::from([]);
        };
        self.text_spans
            .iter()
            .filter_map(|span| {
                let style = styler(span.tag())?;
                Some((span.range(), FontChange::of(&style)?))
            })
            .collect()
    }

    /// Hand the span fonts to the wrapper and wrap the rows they changed again.
    /// `touched` wraps every row an old or new span is on (after undo or redo,
    /// which move text and spans together).
    pub(super) fn refresh_span_layout(
        &mut self,
        previous: &Rc<[TextSpan]>,
        touched: bool,
        cx: &mut App,
    ) {
        if previous.is_empty() && self.text_spans.is_empty() {
            return;
        }
        let fonts = self.span_fonts();
        let rewrap = if touched {
            SpanRewrap::Touched
        } else {
            SpanRewrap::Changed
        };
        self.display_map.set_span_fonts(fonts, rewrap, cx);
        if self.is_multi_line() {
            self.mode.update_auto_grow(&self.display_map);
        }
    }

    /// Move the spans along an edit that replaced `range` by `new_text` (the
    /// document stood at `revision` before it). Returns the undo snapshot when
    /// the spans changed.
    pub(super) fn edit_spans(
        &mut self,
        range: &Range<usize>,
        new_text: &str,
        revision: u64,
    ) -> Option<Box<SpanSnapshot>> {
        if self.replaying_history {
            return None;
        }
        let typing = self
            .typing_spans
            .take()
            .filter(|typing| typing.revision == revision && typing.offset == range.start);
        let pending = self.pending_spans.take();
        if self.text_spans.is_empty()
            && pending.is_none()
            && typing.as_ref().is_none_or(|typing| typing.tags.is_empty())
        {
            return None;
        }
        let typed: Vec<TextSpan>;
        let tags = if let Some(pending) = &pending {
            Inserted::Exactly(pending)
        } else if let Some(typing) = &typing {
            typed = typing
                .tags
                .iter()
                .map(|tag| TextSpan::new(0..new_text.len(), tag.clone()))
                .collect();
            Inserted::Exactly(&typed)
        } else {
            Inserted::Inherit
        };
        let before = self.text_spans.clone();
        let after: Rc<[TextSpan]> = spans_after_edit(&before, range, new_text, tags).into();
        if before == after {
            return None;
        }
        self.text_spans = after.clone();
        // The edit wraps its own rows right after this, with these fonts.
        let fonts = self.span_fonts();
        self.display_map.stage_span_fonts(fonts);
        Some(Box::new(SpanSnapshot { before, after }))
    }

    /// Adopt the spans of content whose text `set_value` just installed.
    pub(super) fn install_spans(&mut self, content: &InputContent, cx: &mut App) {
        let previous = std::mem::replace(&mut self.text_spans, Rc::from([]));
        if !M::CODE_EDITOR && self.text == content.text().as_ref() {
            let text = self.text.clone();
            self.text_spans = clip_spans(content.spans().to_vec(), text.len(), |ix| {
                text.is_char_boundary(ix)
            })
            .into();
        }
        self.refresh_span_layout(&previous, true, cx);
    }

    /// The highlight of every span over `visible`, for the text runs.
    pub(super) fn span_highlights(
        &self,
        visible: &Range<usize>,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        let Some(styler) = self.span_styler.as_ref().filter(|_| self.spans_shown()) else {
            return Vec::new();
        };
        self.text_spans
            .iter()
            .filter(|span| span.range.start < visible.end && visible.start < span.range.end)
            .filter_map(|span| Some((span.range(), styler(span.tag())?.highlight)))
            .collect()
    }

    /// The font families of the spans over `range`, which a highlight cannot carry.
    pub(super) fn span_families(&self, range: &Range<usize>) -> Vec<(Range<usize>, SharedString)> {
        let Some(styler) = self.span_styler.as_ref().filter(|_| self.spans_shown()) else {
            return Vec::new();
        };
        self.text_spans
            .iter()
            .filter(|span| span.range.start < range.end && range.start < span.range.end)
            .filter_map(|span| Some((span.range(), styler(span.tag())?.font_family?)))
            .collect()
    }

    /// The clipboard item for copying `text`, the selected text: with the
    /// spans and tokens of a single selection as metadata, so a paste into an
    /// input restores them.
    pub(super) fn clipboard_item(&self, text: String) -> ClipboardItem {
        let range = self.selected_range();
        let rich = self.selections.is_single()
            && !self.masked
            && !self.token_is_secret()
            && (!self.text_spans.is_empty() || !self.token_spans().is_empty())
            && self.text.slice(range.clone()).to_string() == text;
        if !rich {
            return ClipboardItem::new_string(text);
        }
        let spans: Vec<TextSpan> = self
            .text_spans
            .iter()
            .filter(|span| span.range.start < range.end && range.start < span.range.end)
            .map(|span| {
                TextSpan::new(
                    span.range.start.max(range.start) - range.start
                        ..span.range.end.min(range.end) - range.start,
                    span.tag.clone(),
                )
            })
            .collect();
        let tokens: Vec<(usize, usize, String, String, String)> = self
            .token_spans()
            .iter()
            .filter(|token| range.start <= token.range().start && token.range().end <= range.end)
            .map(|token| {
                (
                    token.range().start - range.start,
                    token.range().end - range.start,
                    token.token().id().to_string(),
                    token.token().text().to_string(),
                    token.token().label().to_string(),
                )
            })
            .collect();
        if spans.is_empty() && tokens.is_empty() {
            return ClipboardItem::new_string(text);
        }
        ClipboardItem::new_string_with_json_metadata(
            text.clone(),
            RichClipboard {
                text,
                spans,
                tokens,
            },
        )
    }

    /// Paste `text` with the spans and tokens the clipboard carries for it.
    /// `false` when it carries none (or they no longer match the text), so the
    /// caller pastes plain text.
    pub(super) fn paste_rich(
        &mut self,
        clipboard: &ClipboardItem,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if M::CODE_EDITOR || self.masked || (!self.is_multi_line() && text.contains('\n')) {
            return false;
        }
        let Some(rich) = clipboard.entries().iter().find_map(|entry| match entry {
            ClipboardEntry::String(string) => string.metadata_json::<RichClipboard>(),
            _ => None,
        }) else {
            return false;
        };
        if rich.text != text {
            return false;
        }
        let mut content = InputContent::new(text.to_string());
        for span in rich.spans {
            if let Ok(next) = content.clone().with_span(span.range(), span.tag().clone()) {
                content = next;
            }
        }
        for (start, end, id, token_text, label) in rich.tokens {
            let token = InlineToken::new(id, token_text).with_label(label);
            if let Ok(next) = content.clone().with_token(start..end, token) {
                content = next;
            }
        }
        if content.spans().is_empty() && content.tokens().is_empty() {
            return false;
        }
        self.insert_content(self.selected_range(), content, window, cx)
            .is_ok()
    }

    /// Replace `range` with `content`: its text, tokens and spans, as one undo
    /// step. The inserted text gets exactly the content's spans.
    pub(super) fn insert_content(
        &mut self,
        range: Range<usize>,
        content: InputContent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), InlineTokenError> {
        if self.ime_marked_range.is_some() {
            return Err(InlineTokenError::CompositionActive);
        }
        if M::CODE_EDITOR || self.masked || !self.mask_pattern.is_none() {
            return Err(InlineTokenError::UnsupportedMode);
        }
        if range.start > range.end
            || range.end > self.text.len()
            || !self.text.is_char_boundary(range.start)
            || !self.text.is_char_boundary(range.end)
        {
            return Err(InlineTokenError::InvalidRange);
        }
        let text = content.text().to_string();
        if self.normalize_input(&text) != text.as_str() {
            return Err(InlineTokenError::TextMismatch);
        }
        let range = self.normalize_token_range(range);
        self.pending_spans = Some(content.spans().to_vec());
        self.pending_tokens = (!content.tokens().is_empty()).then(|| content.tokens().to_vec());
        self.undo_manager.break_transaction_coalescing();
        self.undo_manager.set_pending_intent(EditIntent::Atomic);
        let range_utf16 = self.range_to_utf16(&range);
        self.validated_token_edit = true;
        self.with_edits_allowed(|state| {
            state.replace_text_in_range(Some(range_utf16), &text, window, cx)
        });
        self.validated_token_edit = false;
        self.pending_spans = None;
        self.pending_tokens = None;
        self.undo_manager.break_transaction_coalescing();
        Ok(())
    }

    /// Replace the spans as one undo step, without touching the text.
    pub(super) fn record_spans(&mut self, spans: Vec<TextSpan>, cx: &mut Context<Self>) {
        let text = self.text.clone();
        let after: Rc<[TextSpan]> =
            clip_spans(spans, text.len(), |ix| text.is_char_boundary(ix)).into();
        if after == self.text_spans {
            return;
        }
        let before = std::mem::replace(&mut self.text_spans, after.clone());
        self.typing_spans = None;
        self.document_revision = self.document_revision.wrapping_add(1);
        if !self.undo_manager.is_ignoring() {
            let selection = *self.active_selection();
            let mut change = Change::new(0..0, "", 0..0, "");
            change.spans = Some(Box::new(SpanSnapshot {
                before: before.clone(),
                after,
            }));
            self.undo_manager.break_transaction_coalescing();
            if self
                .undo_manager
                .record_transaction(change, EditIntent::Atomic)
            {
                self.undo_manager
                    .record_selections(vec![selection], vec![selection]);
            }
            self.undo_manager.break_transaction_coalescing();
        }
        self.refresh_span_layout(&before, false, cx);
        if self.emit_events {
            cx.emit(InputEvent::Change);
        }
        cx.notify();
    }
}

macro_rules! span_api {
    ($mode:ty) => {
        impl InputBaseState<$mode> {
            /// The formatting spans, sorted by start (Quill patch).
            pub fn spans(&self) -> &[TextSpan] {
                &self.text_spans
            }
            /// Replace the formatting spans as one undo step; the text is
            /// unchanged. Spans are clipped to character boundaries, and
            /// touching or overlapping spans of one tag merge.
            pub fn set_spans(
                &mut self,
                spans: Vec<TextSpan>,
                _window: &mut Window,
                cx: &mut Context<Self>,
            ) {
                self.record_spans(spans, cx);
            }
            /// The tags the next text typed at the caret gets, replacing what it
            /// would inherit from the spans around it. Forgotten on the next
            /// edit or once the caret moves elsewhere.
            pub fn set_typing_spans(&mut self, tags: Vec<SharedString>) {
                let range = self.selected_range();
                self.typing_spans = range.is_empty().then(|| TypingSpans {
                    offset: range.start,
                    revision: self.document_revision,
                    tags,
                });
            }
            /// The tags set by [`Self::set_typing_spans`], while they still apply.
            pub fn typing_spans(&self) -> Option<&[SharedString]> {
                let range = self.selected_range();
                self.typing_spans
                    .as_ref()
                    .filter(|typing| {
                        range.is_empty()
                            && typing.offset == range.start
                            && typing.revision == self.document_revision
                    })
                    .map(|typing| typing.tags.as_slice())
            }
            /// Replace a UTF-8 byte range with content (text, tokens and spans)
            /// as one undo step; the caret lands after it. The inserted text
            /// gets exactly the content's spans.
            pub fn replace_range_with_content(
                &mut self,
                range: Range<usize>,
                content: InputContent,
                window: &mut Window,
                cx: &mut Context<Self>,
            ) -> Result<(), InlineTokenError> {
                self.insert_content(range, content, window, cx)
            }
            /// Install how span tags are drawn. Presentation only: no edit, no
            /// history, no change event.
            pub fn set_span_styler(
                &mut self,
                styler: Option<TextSpanStyler>,
                cx: &mut Context<Self>,
            ) {
                self.span_styler = styler;
                let spans = self.text_spans.clone();
                self.refresh_span_layout(&spans, true, cx);
                cx.notify();
            }
        }
    };
}
span_api!(super::InputMode);
span_api!(super::TextareaMode);

#[cfg(test)]
mod tests {
    use super::*;

    fn span(range: Range<usize>, tag: &str) -> TextSpan {
        TextSpan::new(range, SharedString::from(tag.to_string()))
    }

    fn edit(spans: &[TextSpan], range: Range<usize>, inserted: &str) -> Vec<TextSpan> {
        spans_after_edit(spans, &range, inserted, Inserted::Inherit)
    }

    #[test]
    fn typing_inside_a_span_joins_it() {
        // "hello" bold, type "XY" after "he".
        let spans = [span(0..5, "b")];
        assert_eq!(edit(&spans, 2..2, "XY"), [span(0..7, "b")]);
    }

    #[test]
    fn typing_at_a_span_end_continues_it_over_letters_only() {
        let spans = [span(0..5, "b")];
        assert_eq!(edit(&spans, 5..5, "ab"), [span(0..7, "b")]);
        assert_eq!(edit(&spans, 5..5, " ab"), [span(0..5, "b")]);
        assert_eq!(edit(&spans, 5..5, "ab cd"), [span(0..7, "b")]);
        // Cyrillic and digits are letters/digits too.
        assert_eq!(edit(&spans, 5..5, "щ1"), [span(0..8, "b")]);
    }

    #[test]
    fn typing_at_a_span_start_does_not_join_it() {
        let spans = [span(3..5, "b")];
        assert_eq!(edit(&spans, 3..3, "xy"), [span(5..7, "b")]);
    }

    #[test]
    fn deleting_inside_and_across_spans_clips_them() {
        let spans = [span(0..10, "b")];
        assert_eq!(edit(&spans, 3..5, ""), [span(0..8, "b")]);
        let spans = [span(5..10, "i")];
        assert_eq!(edit(&spans, 3..7, ""), [span(3..6, "i")]);
        assert_eq!(edit(&spans, 3..7, "ab"), [span(5..8, "i")]);
        // A span deleted whole disappears.
        assert_eq!(edit(&spans, 4..11, ""), []);
    }

    #[test]
    fn replacing_a_selection_that_starts_in_a_span_keeps_the_new_text_in_it() {
        let spans = [span(0..5, "b")];
        assert_eq!(edit(&spans, 3..8, "xy"), [span(0..5, "b")]);
    }

    #[test]
    fn exact_insertion_carves_the_inherited_tags_out() {
        // Typing plain text in the middle of a bold word splits it.
        let spans = [span(0..6, "b")];
        let out = spans_after_edit(&spans, &(3..3), "x", Inserted::Exactly(&[]));
        assert_eq!(out, [span(0..3, "b"), span(4..7, "b")]);
        // A mention inserted with its own tag.
        let mention = [span(0..4, "mention:7")];
        let out = spans_after_edit(&[], &(2..2), "John ", Inserted::Exactly(&mention));
        assert_eq!(out, [span(2..6, "mention:7")]);
    }

    #[test]
    fn same_tags_merge_and_different_tags_overlap() {
        let mut spans = vec![
            span(5..8, "b"),
            span(0..5, "b"),
            span(2..6, "i"),
            span(0..0, "u"),
        ];
        normalize(&mut spans);
        assert_eq!(spans, [span(0..8, "b"), span(2..6, "i")]);
        let mut links = vec![span(0..3, "link:a"), span(3..6, "link:b")];
        normalize(&mut links);
        assert_eq!(links.len(), 2);
    }

    #[test]
    fn clipping_snaps_to_character_boundaries() {
        let text = "aé b";
        let spans = clip_spans(vec![span(2..9, "b")], text.len(), |ix| {
            text.is_char_boundary(ix)
        });
        assert_eq!(spans, [span(1..5, "b")]);
    }

    #[test]
    fn snapshots_merge_first_before_last_after() {
        let a: Rc<[TextSpan]> = Rc::from(vec![span(0..1, "b")]);
        let b: Rc<[TextSpan]> = Rc::from(vec![span(0..2, "b")]);
        let c: Rc<[TextSpan]> = Rc::from(vec![span(0..3, "b")]);
        let merged = SpanSnapshot::merge(
            Some(Box::new(SpanSnapshot {
                before: a.clone(),
                after: b.clone(),
            })),
            Some(Box::new(SpanSnapshot {
                before: b,
                after: c.clone(),
            })),
        )
        .unwrap();
        assert_eq!((merged.before, merged.after), (a, c));
    }
}

#[cfg(test)]
mod state_tests {
    use super::*;
    use crate::input::{Backspace, Copy, Cut, Paste, Redo, TextareaState, Undo};
    use gpui::{
        AppContext as _, Entity, IntoElement, ParentElement as _, Render, Styled as _,
        TestAppContext, VisualTestContext, div,
    };

    struct Root(Entity<TextareaState>);
    impl Render for Root {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(self.0.clone())
        }
    }

    fn textarea(cx: &mut TestAppContext) -> (Entity<TextareaState>, VisualTestContext) {
        cx.update(crate::init);
        let mut input = None;
        let window = cx.update(|cx| {
            cx.open_window(Default::default(), |window, cx| {
                cx.set_global(crate::theme::Theme::default());
                let state = cx.new(|cx| TextareaState::new(window, cx));
                input = Some(state.clone());
                cx.new(|_| Root(state))
            })
            .unwrap()
        });
        let cx = VisualTestContext::from_window(window.into(), cx);
        (input.unwrap(), cx)
    }

    fn tags(state: &TextareaState) -> Vec<(Range<usize>, String)> {
        state
            .spans()
            .iter()
            .map(|span| (span.range(), span.tag().to_string()))
            .collect()
    }

    #[gpui::test]
    fn spans_follow_typing_and_undo_restores_them(cx: &mut TestAppContext) {
        let (input, mut cx) = textarea(cx);
        cx.update(|window, cx| {
            input.update(cx, |state, cx| {
                let content = InputContent::new("hello world")
                    .with_span(0..5, "bold")
                    .unwrap();
                state.set_value(content, window, cx);
                // Type inside the bold word.
                state.set_selected_range(2..2, cx);
                state.replace_text_in_range(None, "XY", window, cx);
                assert_eq!(state.value(), "heXYllo world");
                assert_eq!(tags(state), [(0..7, "bold".into())]);
                // Delete across the end of the span.
                state.set_selected_range(5..9, cx);
                state.replace_text_in_range(None, "", window, cx);
                assert_eq!(state.value(), "heXYlorld");
                assert_eq!(tags(state), [(0..5, "bold".into())]);
                state.undo(&Undo, window, cx);
                assert_eq!(state.value(), "heXYllo world");
                assert_eq!(tags(state), [(0..7, "bold".into())]);
                state.undo(&Undo, window, cx);
                assert_eq!(state.value(), "hello world");
                assert_eq!(tags(state), [(0..5, "bold".into())]);
                state.redo(&Redo, window, cx);
                assert_eq!(tags(state), [(0..7, "bold".into())]);
            });
        });
    }

    #[gpui::test]
    fn setting_spans_is_one_undo_step(cx: &mut TestAppContext) {
        let (input, mut cx) = textarea(cx);
        cx.update(|window, cx| {
            input.update(cx, |state, cx| {
                state.set_value("one two", window, cx);
                state.set_spans(vec![TextSpan::new(4..7, "italic")], window, cx);
                assert_eq!(tags(state), [(4..7, "italic".into())]);
                state.undo(&Undo, window, cx);
                assert!(state.spans().is_empty());
                assert_eq!(state.value(), "one two");
                state.redo(&Redo, window, cx);
                assert_eq!(tags(state), [(4..7, "italic".into())]);
            });
        });
    }

    #[gpui::test]
    fn content_inserts_text_tokens_and_spans_in_one_step(cx: &mut TestAppContext) {
        let (input, mut cx) = textarea(cx);
        cx.update(|window, cx| {
            input.update(cx, |state, cx| {
                state.set_value("hi @jo", window, cx);
                let content = InputContent::new("John 😀 ")
                    .with_span(0..4, "mention:7")
                    .unwrap()
                    .with_token(5..9, InlineToken::new("emoji:1", "😀"))
                    .unwrap();
                state
                    .replace_range_with_content(3..6, content, window, cx)
                    .unwrap();
                assert_eq!(state.value(), "hi John 😀 ");
                assert_eq!(tags(state), [(3..7, "mention:7".into())]);
                assert_eq!(state.tokens()[0].range(), 8..12);
                assert_eq!(state.selected_range(), 13..13);
                // Backspace over the emoji removes it whole.
                state.set_selected_range(12..12, cx);
                state.backspace(&Backspace, window, cx);
                assert_eq!(state.value(), "hi John  ");
                assert!(state.tokens().is_empty());
                state.undo(&Undo, window, cx);
                assert_eq!(state.tokens().len(), 1);
                state.undo(&Undo, window, cx);
                assert_eq!(state.value(), "hi @jo");
                assert!(state.spans().is_empty());
                assert!(state.tokens().is_empty());
            });
        });
    }

    #[gpui::test]
    fn typing_spans_tag_the_next_typed_text_only(cx: &mut TestAppContext) {
        let (input, mut cx) = textarea(cx);
        cx.update(|window, cx| {
            input.update(cx, |state, cx| {
                let content = InputContent::new("bold").with_span(0..4, "bold").unwrap();
                state.set_value(content, window, cx);
                state.set_selected_range(4..4, cx);
                // Plain text right after a bold word (a markdown replacement).
                state.set_typing_spans(vec![]);
                assert_eq!(state.typing_spans(), Some(&[][..]));
                state.replace_text_in_range(None, "x", window, cx);
                assert_eq!(state.value(), "boldx");
                assert_eq!(tags(state), [(0..4, "bold".into())]);
                // Without the override letters continue the span.
                state.set_selected_range(4..4, cx);
                state.replace_text_in_range(None, "y", window, cx);
                assert_eq!(tags(state), [(0..5, "bold".into())]);
                // An italic caret.
                state.set_selected_range(6..6, cx);
                state.set_typing_spans(vec!["italic".into()]);
                state.replace_text_in_range(None, "z", window, cx);
                assert_eq!(
                    tags(state),
                    [(0..5, "bold".into()), (6..7, "italic".into())]
                );
                assert_eq!(state.typing_spans(), None);
            });
        });
    }

    #[gpui::test]
    fn copy_and_paste_carry_spans_and_tokens(cx: &mut TestAppContext) {
        let (input, mut cx) = textarea(cx);
        cx.update(|window, cx| {
            input.update(cx, |state, cx| {
                let content = InputContent::new("a bold 😀 b")
                    .with_span(2..6, "bold")
                    .unwrap()
                    .with_token(7..11, InlineToken::new("emoji:9", "😀"))
                    .unwrap();
                state.set_value(content, window, cx);
                state.set_selected_range(2..11, cx);
                state.copy(&Copy, window, cx);
                let end = state.value().len();
                state.set_selected_range(end..end, cx);
                state.paste(&Paste, window, cx);
                assert_eq!(state.value(), "a bold 😀 bbold 😀");
                assert_eq!(
                    tags(state),
                    [(2..6, "bold".into()), (13..17, "bold".into())]
                );
                assert_eq!(state.tokens().len(), 2);
                assert_eq!(state.tokens()[1].range(), 18..22);
                // Cut keeps them too.
                state.set_selected_range(0..6, cx);
                state.cut(&Cut, window, cx);
                assert_eq!(tags(state), [(7..11, "bold".into())]);
                state.set_selected_range(0..0, cx);
                state.paste(&Paste, window, cx);
                assert_eq!(tags(state)[0], (2..6, "bold".into()));
            });
        });
    }

    #[gpui::test]
    fn plain_clipboard_text_pastes_plain(cx: &mut TestAppContext) {
        let (input, mut cx) = textarea(cx);
        cx.update(|window, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string("xyz".to_string()));
            input.update(cx, |state, cx| {
                let content = InputContent::new("ab").with_span(0..2, "bold").unwrap();
                state.set_value(content, window, cx);
                state.set_selected_range(2..2, cx);
                state.paste(&Paste, window, cx);
                // Letters pasted at the end of a span continue it, like typing.
                assert_eq!(state.value(), "abxyz");
                assert_eq!(tags(state), [(0..5, "bold".into())]);
            });
        });
    }
}
