//! The quote line of a reply strip with the quote's own styling (bold,
//! italic, underline, strikethrough, spoiler), for quotes chosen from rich
//! messages. tdesktop draws the same entities in the reply header.
#![deny(clippy::indexing_slicing)]

use gpui_kit::*;
use quill::rich::quote::one_line;
use quill::text::{TextEntity, styled_runs};
use std::ops::Range;

/// `text` on one line and the text styling of its entities. A spoiler stays
/// a solid block: the strip never reveals it.
pub(super) fn quote_highlights(
    text: &str,
    entities: &[TextEntity],
    color: Hsla,
) -> (String, Vec<(Range<usize>, HighlightStyle)>) {
    let (line, entities) = one_line(text, entities);
    let mut highlights = Vec::new();
    let mut at = 0usize;
    for run in styled_runs(&line, &entities) {
        let end = at.saturating_add(run.text.len());
        let style = &run.style;
        if !style.is_plain() {
            highlights.push((
                at..end,
                HighlightStyle {
                    font_weight: style.bold.then_some(FontWeight::BOLD),
                    font_style: style.italic.then_some(FontStyle::Italic),
                    underline: style.underline.then(|| UnderlineStyle {
                        thickness: px(1.),
                        color: None,
                        wavy: false,
                    }),
                    strikethrough: style.strikethrough.then(|| StrikethroughStyle {
                        thickness: px(1.),
                        color: None,
                    }),
                    color: style.spoiler.then(|| color.opacity(0.)),
                    background_color: style.spoiler.then(|| color.opacity(0.35)),
                    ..Default::default()
                },
            ));
        }
        at = end;
    }
    (line, highlights)
}

pub(super) fn styled_quote_line(text: &str, entities: &[TextEntity], color: Hsla) -> StyledText {
    let (line, highlights) = quote_highlights(text, entities, color);
    StyledText::new(line).with_highlights(highlights)
}

#[cfg(test)]
mod tests {
    use super::quote_highlights;
    use gpui_kit::{FontStyle, FontWeight, Hsla};
    use quill::text::{TextEntity, TextEntityKind};

    fn entity(start: usize, end: usize, kind: TextEntityKind) -> TextEntity {
        TextEntity {
            utf8_start: start,
            utf8_end: end,
            kind,
        }
    }

    #[test]
    fn entities_become_text_styling_on_one_line() {
        let entities = [
            entity(0, 5, TextEntityKind::Bold),
            entity(8, 12, TextEntityKind::Italic),
            entity(13, 16, TextEntityKind::Spoiler),
        ];
        let (line, highlights) = quote_highlights("quote a\nfeel hide", &entities, Hsla::default());
        assert_eq!(line, "quote a feel hide");
        let [
            (bold, bold_style),
            (italic, italic_style),
            (spoiler, spoiler_style),
        ] = highlights.as_slice()
        else {
            panic!("three styled runs, got {highlights:?}");
        };
        assert_eq!(line.get(bold.clone()), Some("quote"));
        assert_eq!(bold_style.font_weight, Some(FontWeight::BOLD));
        assert_eq!(line.get(italic.clone()), Some("feel"));
        assert_eq!(italic_style.font_style, Some(FontStyle::Italic));
        assert_eq!(line.get(spoiler.clone()), Some("hid"));
        assert!(spoiler_style.background_color.is_some());
    }

    #[test]
    fn plain_text_has_no_highlights() {
        let (line, highlights) = quote_highlights("just words", &[], Hsla::default());
        assert_eq!(line, "just words");
        assert!(highlights.is_empty());
    }
}
