//! A paragraph's text as `inline_paragraph` draws it, shared with the
//! measurement that lays captions out before they draw (`measure`).

use super::*;

/// One paragraph's string, with resolved emoji as placeholder glyphs, its
/// styling ranges, and what its clicks, spoilers and pictures act on.
pub(super) struct ParagraphText {
    pub(super) text: String,
    pub(super) highlights: Vec<(std::ops::Range<usize>, HighlightStyle)>,
    pub(super) mono: Vec<(std::ops::Range<usize>, SharedString)>,
    pub(super) targets: Vec<ClickTarget>,
    pub(super) spoilers: Vec<(std::ops::Range<usize>, f32)>,
    pub(super) inline_emoji: Vec<super::super::selectable_text::InlineEmoji>,
    /// What `text` dropped from the message, for copy and quote.
    pub(super) source: quill::display_text::SourceMap,
}

/// One clickable range of the paragraph and what a click on it does.
pub(super) struct ClickTarget {
    pub(super) range: std::ops::Range<usize>,
    pub(super) action: InlineAction,
}

impl ClickTarget {
    /// Links underline on hover; spoilers and code do not.
    pub(super) fn is_link(&self) -> bool {
        matches!(self.action, InlineAction::Link(_))
    }
}

pub(super) fn paragraph_text(
    runs: &[TextRun],
    first_index: usize,
    msg_key: (i64, u64),
    is_caption: bool,
    revealed: &super::revealed::Revealed,
    emoji_paths: &HashMap<i64, ImageSource>,
    layered: &HashMap<i64, StickerView>,
) -> ParagraphText {
    let mut text = String::new();
    let mut highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = Vec::new();
    let mut mono: Vec<(std::ops::Range<usize>, SharedString)> = Vec::new();
    let mut targets: Vec<ClickTarget> = Vec::new();
    let mut spoilers: Vec<(std::ops::Range<usize>, f32)> = Vec::new();
    let mut inline_emoji: Vec<super::super::selectable_text::InlineEmoji> = Vec::new();
    let mut filter = quill::display_text::FilterState::default();
    let mut source = quill::display_text::SourceMap::default();
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
        let hidden = style.spoiler && !revealed.spoiler_shown(&key);
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
                targets.push(ClickTarget {
                    range: range.clone(),
                    action: InlineAction::Link(link.clone()),
                });
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
                    targets.push(ClickTarget {
                        range: range.clone(),
                        action: InlineAction::Link(link.clone()),
                    });
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
            quill::display_text::push_filtered(&mut text, piece, &mut filter, &mut source);
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
                targets.push(ClickTarget {
                    range: range.clone(),
                    action: InlineAction::RevealSpoiler(key),
                });
            } else if let Some(link) = &run.link {
                highlight.color = Some(accent_info().into());
                targets.push(ClickTarget {
                    range: range.clone(),
                    action: InlineAction::Link(link.clone()),
                });
            } else if style.code && !style.pre {
                targets.push(ClickTarget {
                    range: range.clone(),
                    action: InlineAction::CopyCode(run.text.clone()),
                });
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
        targets,
        spoilers,
        inline_emoji,
        source,
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "paragraph_tests.rs"]
mod tests;
