//! A paragraph's text as `inline_paragraph` draws it, shared with the
//! measurement that lays captions out before they draw (`measure`).

use super::*;

/// One paragraph's string, with resolved emoji as placeholder glyphs, its
/// styling ranges, and what its clicks, spoilers and pictures act on.
pub(super) struct ParagraphText {
    pub(super) text: String,
    pub(super) highlights: Vec<(std::ops::Range<usize>, HighlightStyle)>,
    pub(super) mono: Vec<(std::ops::Range<usize>, SharedString)>,
    pub(super) click_ranges: Vec<std::ops::Range<usize>>,
    pub(super) actions: Vec<InlineAction>,
    pub(super) link_flags: Vec<bool>,
    pub(super) spoilers: Vec<(std::ops::Range<usize>, f32)>,
    pub(super) inline_emoji: Vec<super::super::selectable_text::InlineEmoji>,
}

pub(super) fn paragraph_text(
    runs: &[TextRun],
    first_index: usize,
    msg_key: (i64, u64),
    is_caption: bool,
    revealed: &std::collections::HashSet<(i64, u64, u64, bool)>,
    emoji_paths: &HashMap<i64, ImageSource>,
    layered: &HashMap<i64, StickerView>,
) -> ParagraphText {
    let mut text = String::new();
    let mut highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = Vec::new();
    let mut mono: Vec<(std::ops::Range<usize>, SharedString)> = Vec::new();
    let mut click_ranges = Vec::new();
    let mut actions = Vec::new();
    // Per click range: whether it is a link (underlined on hover).
    let mut link_flags: Vec<bool> = Vec::new();
    let mut spoilers: Vec<(std::ops::Range<usize>, f32)> = Vec::new();
    let mut inline_emoji: Vec<super::super::selectable_text::InlineEmoji> = Vec::new();
    let mut filter = quill::display_text::FilterState::default();
    for (offset, run) in runs.iter().enumerate() {
        if run.text.is_empty() {
            continue;
        }
        let style = &run.style;
        let key = (
            msg_key.0,
            msg_key.1,
            (first_index + offset) as u64,
            is_caption,
        );
        let hidden = style.spoiler && !revealed.contains(&key);
        // A resolved custom emoji (animated first) becomes a placeholder
        // glyph; unresolved ones and hidden spoilers keep their text.
        let visual = run.custom_emoji_id.filter(|_| !hidden).and_then(|id| {
            layered
                .get(&id)
                .map(|clip| super::super::selectable_text::InlineEmojiVisual::Clip(clip.clone()))
                .or_else(|| {
                    emoji_paths.get(&id).map(|source| {
                        super::super::selectable_text::InlineEmojiVisual::Image(source.clone())
                    })
                })
        });
        if let Some(visual) = visual {
            let at = text.len();
            text.push(super::super::selectable_text::EMOJI_PLACEHOLDER);
            let range = at..text.len();
            // The picture carries no text styling; a link around it keeps
            // its click target.
            if let Some(link) = &run.link {
                click_ranges.push(range.clone());
                actions.push(InlineAction::Link(link.clone()));
                link_flags.push(true);
            }
            inline_emoji.push(super::super::selectable_text::InlineEmoji {
                range,
                visual,
                fallback: run.text.clone(),
            });
            continue;
        }
        // The active emoji set draws plain emoji as pictures, placed like
        // custom emoji (`emoji_sprites`); hidden spoilers and code keep
        // their characters.
        let sprites = (!hidden && !style.code && !style.pre)
            .then(|| super::super::emoji_sprites::pieces(&run.text))
            .flatten();
        let segments: Vec<(std::ops::Range<usize>, Option<std::sync::Arc<RenderImage>>)> =
            match sprites {
                Some(pieces) => pieces.into_iter().map(|p| (p.range, p.image)).collect(),
                None => vec![(0..run.text.len(), None)],
            };
        for (segment, sprite) in segments {
            let piece = run.text.get(segment).unwrap_or_default();
            let at = text.len();
            if let Some(image) = sprite {
                text.push(super::super::selectable_text::EMOJI_PLACEHOLDER);
                let range = at..text.len();
                let range_for_specks = range.clone();
                if let Some(link) = &run.link {
                    click_ranges.push(range.clone());
                    actions.push(InlineAction::Link(link.clone()));
                    link_flags.push(true);
                }
                inline_emoji.push(super::super::selectable_text::InlineEmoji {
                    range,
                    visual: super::super::selectable_text::InlineEmojiVisual::Image(
                        ImageSource::Render(image),
                    ),
                    fallback: piece.to_string(),
                });
                // A revealed spoiler's specks fade out over the picture too.
                if style.spoiler
                    && let Some(fade) = super::super::spoiler_fx::reveal_fade(key)
                {
                    spoilers.push((range_for_specks, fade));
                }
                continue;
            }
            // Ranges are taken from what was pushed, so dropped characters
            // never shift a style or a click target.
            text.push_str(&quill::display_text::filter_display_text(
                piece,
                &mut filter,
            ));
            let range = at..text.len();
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
            if style.code {
                highlight.background_color = Some(fill_muted().into());
                mono.push((range.clone(), MONO_FONT.into()));
            }
            // tdesktop hides spoiler text under drifting specks of its color;
            // a revealed run's specks fade out over the text.
            if hidden {
                spoilers.push((range.clone(), 1.));
            } else if style.spoiler
                && let Some(fade) = super::super::spoiler_fx::reveal_fade(key)
            {
                spoilers.push((range.clone(), fade));
            }
            if hidden {
                // GPUI blends a highlight color over the text's, so a clear
                // color changes nothing: fade the glyphs out instead.
                highlight.color = None;
                highlight.fade_out = Some(1.);
                highlight.background_color = None;
                click_ranges.push(range.clone());
                actions.push(InlineAction::RevealSpoiler(key));
                link_flags.push(false);
            } else if let Some(link) = &run.link {
                highlight.color = Some(accent_info().into());
                click_ranges.push(range.clone());
                actions.push(InlineAction::Link(link.clone()));
                link_flags.push(true);
            } else if style.code && !style.pre {
                click_ranges.push(range.clone());
                actions.push(InlineAction::CopyCode(run.text.clone()));
                link_flags.push(false);
            }
            if highlight != HighlightStyle::default() {
                highlights.push((range, highlight));
            }
        }
    }
    ParagraphText {
        text,
        highlights,
        mono,
        click_ranges,
        actions,
        link_flags,
        spoilers,
        inline_emoji,
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "paragraph_tests.rs"]
mod tests;
