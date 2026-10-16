//! `rich_text_line` laid out before it draws, for a box that sizes around
//! its text (the media viewer's caption plate). Paragraphs are shaped with
//! the runs `inline_paragraph` draws; quotes and `pre` blocks add the
//! insets their blocks draw around the text.

use super::*;

/// Rich text's laid-out height and widest line, in window points.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(in crate::ui) struct RichTextSize {
    pub(in crate::ui) height: f32,
    pub(in crate::ui) width: f32,
}

/// The text a `rich_text_line` call draws, and the style it draws in.
pub(in crate::ui) struct RichTextSource<'a> {
    pub(in crate::ui) text: &'a str,
    pub(in crate::ui) entities: &'a [TextEntity],
    pub(in crate::ui) msg_key: (i64, u64),
    pub(in crate::ui) is_caption: bool,
    pub(in crate::ui) revealed: &'a std::collections::HashSet<(i64, u64, u64, bool)>,
    pub(in crate::ui) emoji_paths: &'a HashMap<i64, ImageSource>,
}

/// `quote_block` and `pre_block`: horizontal inset (padding and the
/// quote's accent bar) and vertical room (padding and margins), in rem.
const BLOCK_INSET_X_REM: f32 = 1.0;
const QUOTE_BAR: f32 = 2.0;
const BLOCK_INSET_Y_REM: f32 = 1.0;
/// `pre_block` keeps room on the right for its copy button (`pr_6`).
const PRE_COPY_REM: f32 = 1.5;
/// The quote's "Show more" row, a medium kit button (`h_8`).
const QUOTE_TOGGLE_REM: f32 = 2.0;

/// Lay `source` out as `rich_text_line` draws it in `style` (its font size
/// and family) with lines `line_h` tall, wrapping at `wrap`.
pub(in crate::ui) fn measure_rich_text(
    source: &RichTextSource,
    style: &TextStyle,
    line_h: f32,
    wrap: f32,
    window: &mut Window,
) -> RichTextSize {
    let rem = f32::from(window.rem_size());
    let font = style.font_size.to_pixels(window.rem_size());
    let runs = styled_runs(source.text, source.entities);
    let no_clips = HashMap::new();
    let paragraph = |runs: &[TextRun], first: usize| {
        paragraph_text(
            runs,
            first,
            source.msg_key,
            source.is_caption,
            source.revealed,
            source.emoji_paths,
            &no_clips,
        )
    };
    let mut size = RichTextSize::default();
    let mut index = 0;
    while let Some(run) = runs.get(index) {
        let start = index;
        if run.style.quote {
            while runs.get(index).is_some_and(|run| run.style.quote) {
                index += 1;
            }
            let group = runs.get(start..index).unwrap_or_default();
            let quote_text: String = group.iter().map(|run| run.text.as_str()).collect();
            let key = (
                source.msg_key.0,
                source.msg_key.1,
                (start as u64) | (1 << 63),
                source.is_caption,
            );
            let collapsible = quote_collapses(&quote_text);
            let shown = if collapsible && !source.revealed.contains(&key) {
                truncate_runs(group, collapsed_quote_len(&quote_text))
            } else {
                group.to_vec()
            };
            let text = paragraph(&shown, start);
            let inner = wrap - BLOCK_INSET_X_REM * rem - QUOTE_BAR;
            let (lines, _) = shape_paragraph(&text, style, font, inner, window);
            let toggle = if collapsible {
                QUOTE_TOGGLE_REM * rem
            } else {
                0.
            };
            size.height += lines as f32 * line_h + BLOCK_INSET_Y_REM * rem + toggle;
            size.width = wrap;
            continue;
        }
        if run.style.pre {
            let code = run.text.trim_end_matches('\n');
            let mut mono = style.clone();
            mono.font_family = MONO_FONT.into();
            let inner = wrap - (BLOCK_INSET_X_REM + PRE_COPY_REM) * rem;
            let (lines, _) = shape_lines(code, font, &[mono.to_run(code.len())], inner, window);
            let language = run
                .style
                .language
                .as_ref()
                .is_some_and(|language| !language.trim().is_empty());
            let label = if language { line_h * 0.8 } else { 0. };
            size.height += lines as f32 * line_h + BLOCK_INSET_Y_REM * rem + label;
            size.width = wrap;
            index += 1;
            continue;
        }
        while runs
            .get(index)
            .is_some_and(|run| !run.style.quote && !run.style.pre)
        {
            index += 1;
        }
        let group = runs.get(start..index).unwrap_or_default();
        let text = paragraph(group, start);
        let (lines, width) = shape_paragraph(&text, style, font, wrap, window);
        size.height += lines as f32 * line_h;
        size.width = size.width.max(width);
    }
    size
}

/// The first `len` bytes of `runs`, each run keeping its style.
fn truncate_runs(runs: &[TextRun], mut len: usize) -> Vec<TextRun> {
    let mut shown = Vec::new();
    for run in runs {
        if len == 0 {
            break;
        }
        let take = len.min(run.text.len());
        len -= take;
        let mut run = run.clone();
        run.text.truncate(take);
        shown.push(run);
    }
    shown
}

/// A paragraph shaped with the runs its `StyledText` builds: the base
/// style, its highlights over it, and the monospace family on code.
fn shape_paragraph(
    paragraph: &ParagraphText,
    style: &TextStyle,
    font: Pixels,
    wrap: f32,
    window: &mut Window,
) -> (usize, f32) {
    let runs = paragraph_runs(paragraph, style);
    shape_lines(paragraph.text.as_str(), font, &runs, wrap, window)
}

fn paragraph_runs(paragraph: &ParagraphText, style: &TextStyle) -> Vec<gpui_kit::TextRun> {
    let text = paragraph.text.as_str();
    let mut runs = Vec::new();
    let mut at = 0;
    for (range, highlight) in &paragraph.highlights {
        if at < range.start {
            runs.push(style.to_run(range.start - at));
        }
        runs.push(style.clone().highlight(*highlight).to_run(range.len()));
        at = range.end;
    }
    if at < text.len() {
        runs.push(style.to_run(text.len() - at));
    }
    let mut offset = 0;
    for run in &mut runs {
        let span = offset..offset + run.len;
        if let Some((_, family)) = paragraph
            .mono
            .iter()
            .find(|(range, _)| range.start <= span.start && span.end <= range.end)
        {
            run.font.family = family.clone();
        }
        offset = span.end;
    }
    runs
}

/// Lines and widest line of `text` in `runs`, wrapped at `wrap`.
fn shape_lines(
    text: &str,
    font: Pixels,
    runs: &[gpui_kit::TextRun],
    wrap: f32,
    window: &mut Window,
) -> (usize, f32) {
    if text.is_empty() {
        return (0, 0.);
    }
    let shaped = window.text_system().shape_text(
        SharedString::from(text.to_string()),
        font,
        runs,
        Some(px(wrap.max(1.))),
        None,
    );
    let Ok(shaped) = shaped else {
        return (0, 0.);
    };
    let lines = shaped
        .iter()
        .map(|line| line.wrap_boundaries().len() + 1)
        .sum();
    let width = shaped
        .iter()
        .map(|line| f32::from(line.width()))
        .fold(0., f32::max);
    (lines, width)
}

#[cfg(test)]
mod tests {
    use super::{
        HashMap, MONO_FONT, SharedString, TextEntity, TextEntityKind, TextStyle, paragraph_runs,
        paragraph_text, styled_runs,
    };
    use gpui_kit::FontWeight;

    fn runs_of(text: &str, kind: TextEntityKind) -> Vec<gpui_kit::TextRun> {
        let entities = [TextEntity {
            utf8_start: 0,
            utf8_end: 4,
            kind,
        }];
        let runs = styled_runs(text, &entities);
        let paragraph = paragraph_text(
            &runs,
            0,
            (1, 1),
            true,
            &Default::default(),
            &HashMap::new(),
            &HashMap::new(),
        );
        paragraph_runs(&paragraph, &TextStyle::default())
    }

    #[test]
    fn a_bold_word_is_measured_bold() {
        let runs = runs_of("Bold rest", TextEntityKind::Bold);
        let first = runs.first().expect("a run");
        assert_eq!(first.len, 4);
        assert_eq!(first.font.weight, FontWeight::BOLD);
        assert_ne!(
            runs.last().map(|run| run.font.weight),
            Some(FontWeight::BOLD)
        );
        assert_eq!(runs.iter().map(|run| run.len).sum::<usize>(), 9);
    }

    #[test]
    fn a_code_word_is_measured_in_the_monospace_font() {
        let runs = runs_of("code rest", TextEntityKind::Code);
        let first = runs.first().expect("a run");
        assert_eq!(first.font.family, SharedString::from(MONO_FONT));
        assert_ne!(
            runs.last().map(|run| run.font.family.clone()),
            Some(SharedString::from(MONO_FONT))
        );
    }
}
