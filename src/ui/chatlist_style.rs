//! Slice chatlist-list-style: the chat-list row preview line renderer.
//! Pure style inputs (`icon`, `entities`) come from
//! `quill::chatlist_style`; this module only paints them.
#![deny(clippy::indexing_slicing)]

use super::message_text::MONO_FONT;
use super::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::text::{TextEntity, styled_runs};
/// The chat-row preview line — optional media icon plus the preview
/// text, plain or formatted via `styled_runs`. Single line: the
/// container truncates, so runs never wrap. Spoilers render hidden
/// (open the chat to reveal); links render unstyled and don't open from
/// the list row.
pub(crate) fn chat_list_preview_line(
    icon: Option<&str>,
    preview: &str,
    entities: &[TextEntity],
    // Images for the preview's custom emoji (animated when decoded).
    emoji: &std::collections::HashMap<i64, ImageSource>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    chat_list_preview_line_layered(
        icon,
        preview,
        entities,
        emoji,
        &std::collections::HashMap::new(),
        None,
        muted,
    )
}

/// [`chat_list_preview_line`] whose decoded animated emoji (`layered`)
/// are painted by the chat list's animation layer (`anim_layer`).
pub(crate) fn chat_list_preview_line_layered(
    icon: Option<&str>,
    preview: &str,
    entities: &[TextEntity],
    emoji: &std::collections::HashMap<i64, ImageSource>,
    layered: &std::collections::HashMap<i64, super::sticker_player::StickerView>,
    // The sender ("Name: ") in the accent color. Right-to-left previews lay it out as the
    // first run of the one line (Telegram Desktop builds "from: text" as one
    // `Ui::Text::String`, so a Latin name makes the paragraph left-to-right); the caller
    // draws the prefix itself for every other preview.
    prefix: Option<String>,
    // Text colour (the selected row passes a higher-contrast one).
    muted: Hsla,
) -> AnyElement {
    // One line, as in Telegram Desktop: line breaks read as spaces (same
    // byte length, so entity offsets still hold).
    let preview = preview.replace(['\n', '\r'], " ");
    let prefix = prefix.map(|prefix| quill::display_text::filtered(&prefix).into_owned());
    // Emoji the active set draws need the run-by-run row below; the
    // system font keeps the single text node.
    let sprites =
        !quill::text::has_rtl_text(&preview) && super::emoji_sprites::pieces(&preview).is_some();
    if entities.is_empty() && !sprites {
        // A single text node, so an overlong preview ends in an ellipsis.
        let preview = quill::display_text::filtered(&preview);
        let text = match icon {
            Some(glyph) => format!("{glyph} {preview}"),
            None => preview.into_owned(),
        };
        let mut highlights = Vec::new();
        let text = match &prefix {
            Some(prefix) => {
                highlights.push((
                    0..prefix.len(),
                    HighlightStyle {
                        color: Some(accent().into()),
                        ..Default::default()
                    },
                ));
                format!("{prefix}{text}")
            }
            None => text,
        };
        return div()
            .text_xs()
            .truncate()
            .text_color(muted)
            .child(super::bidi_line::one_line(text, highlights, Vec::new()))
            .into_any_element();
    }
    // Right-to-left text without pictures to place: one bidi-correct line (typing-order
    // elision, right-aligned like Telegram Desktop's one-line text).
    if quill::text::has_rtl_text(&preview) {
        let runs = shown_runs(&preview, entities);
        let pictures = runs.iter().any(|run| {
            run.custom_emoji_id
                .is_some_and(|id| layered.contains_key(&id) || emoji.contains_key(&id))
        });
        if !pictures {
            let mut text = String::new();
            let mut highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = Vec::new();
            let mut mono: Vec<(std::ops::Range<usize>, SharedString)> = Vec::new();
            if let Some(prefix) = &prefix {
                text.push_str(prefix);
                highlights.push((
                    0..prefix.len(),
                    HighlightStyle {
                        color: Some(accent().into()),
                        ..Default::default()
                    },
                ));
            }
            if let Some(glyph) = icon {
                text.push_str(glyph);
                text.push(' ');
            }
            for run in &runs {
                let start = text.len();
                text.push_str(&run.text);
                let range = start..text.len();
                let style = &run.style;
                let mut highlight = HighlightStyle::default();
                if style.bold {
                    highlight.font_weight = Some(FontWeight::BOLD);
                }
                if style.italic {
                    highlight.font_style = Some(FontStyle::Italic);
                }
                if style.underline {
                    highlight.underline = Some(UnderlineStyle {
                        thickness: px(1.),
                        ..Default::default()
                    });
                }
                if style.strikethrough {
                    highlight.strikethrough = Some(StrikethroughStyle {
                        thickness: px(1.),
                        ..Default::default()
                    });
                }
                if style.code || style.pre {
                    mono.push((range.clone(), MONO_FONT.into()));
                }
                if style.spoiler {
                    highlight.fade_out = Some(1.);
                    highlight.background_color = Some(fill_muted().into());
                }
                if highlight != HighlightStyle::default() {
                    highlights.push((range, highlight));
                }
            }
            return div()
                .text_xs()
                .truncate()
                .text_color(muted)
                .child(super::bidi_line::one_line(text, highlights, mono))
                .into_any_element();
        }
    }
    let mut parts: Vec<(bool, Div)> = Vec::new();
    if let Some(prefix) = prefix {
        parts.push((false, div().text_color(accent()).child(prefix)));
    }
    if let Some(glyph) = icon {
        parts.push((false, div().child(format!("{glyph} "))));
    }
    let key = line_key(&preview);
    for (index, run) in shown_runs(&preview, entities).into_iter().enumerate() {
        if run.text.is_empty() {
            continue;
        }
        if let Some(clip) = run.custom_emoji_id.and_then(|id| layered.get(&id)) {
            let frames = super::anim_layer::frames(clip.clone());
            parts.push((false, div().child(frames.size(px(14.)).flex_none())));
            continue;
        }
        if let Some(source) = run.custom_emoji_id.and_then(|id| emoji.get(&id)) {
            parts.push((
                false,
                div().child(
                    img(source.clone())
                        .id(SharedString::from(format!("preview-emoji-{key}-{index}")))
                        .size(px(14.))
                        .aspect_square()
                        .object_fit(ObjectFit::Contain),
                ),
            ));
            continue;
        }
        let style = &run.style;
        let segments: Vec<(String, Option<std::sync::Arc<RenderImage>>)> =
            match (!style.spoiler && !style.code && !style.pre)
                .then(|| super::emoji_sprites::pieces(&run.text))
                .flatten()
            {
                Some(pieces) => pieces
                    .into_iter()
                    .map(|p| {
                        (
                            run.text.get(p.range).unwrap_or_default().to_string(),
                            p.image,
                        )
                    })
                    .collect(),
                None => vec![(run.text, None)],
            };
        for (segment, sprite) in segments {
            if let Some(image) = sprite {
                parts.push((
                    false,
                    div().child(
                        img(ImageSource::Render(image))
                            .size(px(14.))
                            .object_fit(ObjectFit::Contain),
                    ),
                ));
                continue;
            }
            let mut el = div().whitespace_nowrap().child(segment);
            if style.bold {
                el = el.font_weight(FontWeight::BOLD);
            }
            if style.italic {
                el = el.italic();
            }
            if style.underline {
                el = el.underline();
            }
            if style.strikethrough {
                el = el.line_through();
            }
            if style.code || style.pre {
                el = el.font_family(MONO_FONT);
            }
            if style.spoiler {
                el = el.bg(fill_muted()).text_color(fill_muted()).rounded_sm();
            }
            parts.push((true, el));
        }
    }
    // The last text run shrinks and ends in the ellipsis; everything
    // before it keeps its width.
    let last_text = parts.iter().rposition(|(is_text, _)| *is_text);
    let mut line = div()
        .text_xs()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_color(muted)
        .flex()
        .flex_row()
        .items_center()
        // A right-to-left line rests against the end edge (Telegram Desktop).
        .when(quill::text::has_rtl_text(&preview), |line| {
            line.justify_end()
        });
    for (position, (_, part)) in parts.into_iter().enumerate() {
        line = line.child(if Some(position) == last_text {
            part.min_w_0().flex_shrink(1.).truncate()
        } else {
            part.flex_none()
        });
    }
    line.into_any_element()
}

/// A stable per-preview key for element ids (GPUI animates an image
/// only when it has an id).
fn line_key(preview: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    preview.hash(&mut hasher);
    hasher.finish()
}

/// The preview's runs without what message text drops before shaping
/// (`quill::display_text`), counted across runs as a paragraph counts them.
fn shown_runs(preview: &str, entities: &[TextEntity]) -> Vec<quill::text::TextRun> {
    let mut state = quill::display_text::FilterState::default();
    let mut map = quill::display_text::SourceMap::default();
    let mut runs = styled_runs(preview, entities);
    for run in &mut runs {
        let mut shown = String::with_capacity(run.text.len());
        quill::display_text::push_filtered(&mut shown, &run.text, &mut state, &mut map);
        run.text = shown;
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::shown_runs;

    #[test]
    fn a_formatted_preview_loses_zalgo_marks_counted_across_its_runs() {
        let bold = quill::text::TextEntity {
            utf8_start: 0,
            utf8_end: 5,
            kind: quill::text::TextEntityKind::Bold,
        };
        let text = format!("e\u{0301}\u{0301}{}x\u{E0069}", "\u{0301}".repeat(30));
        let shown: String = shown_runs(&text, &[bold])
            .into_iter()
            .map(|run| run.text)
            .collect();
        assert_eq!(shown, "e\u{0301}\u{0301}x");
    }
}
