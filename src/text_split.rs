//! Long-message handling for the composer (tdesktop `ApiWrap::sendMessage`
//! cuts text over `PremiumLimits::messageLengthCurrent()` into several
//! messages with `TextUtilities::CutPart`; editing refuses instead and
//! shows `CharactersLimitLabel`).
//!
//! The limit comes from the TDLib option `message_text_length_max`
//! (default 4096, raised for Premium). Lengths are counted in UTF-16 code
//! units of the text *after* the composer markup is parsed (the units
//! tdesktop's `QString::size()` and `TextEntity` offsets use), which is
//! never smaller than the scalar-value count, so a text that fits here
//! also fits whichever unit the server counts.
#![deny(clippy::indexing_slicing)]

use crate::composer::{parse_format_markup, scan_work};
use crate::composer_doc::{ComposerDoc, ComposerTag, DisjointRanges, DocSpan};

/// UTF-16 code units of `text`.
pub fn utf16_len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

/// Length (UTF-16 units) of `markup` once its formatting markers are
/// stripped — the text the server will count.
pub fn markup_text_units(markup: &str) -> usize {
    utf16_len(&parse_format_markup(markup).0)
}

/// How many units `markup` is over `limit` (0 when it fits). Cheap
/// early-out: parsing only removes characters, so a markup whose UTF-16
/// length fits cannot be over.
pub fn units_over_limit(markup: &str, limit: i32) -> usize {
    let limit = usize::try_from(limit.max(0)).unwrap_or(0);
    if limit == 0 || utf16_len(markup) <= limit {
        return 0;
    }
    markup_text_units(markup).saturating_sub(limit)
}

/// Break-point quality, mirroring the levels in tdesktop's `CutPart`
/// (paragraph break > line break > sentence end > clause end > space >
/// after a word separator).
fn break_level(prev: Option<char>, ch: char, next: Option<char>) -> Option<u8> {
    if ch == '\n' {
        return Some(if next == Some('\n') { 15 } else { 13 });
    }
    if ch.is_whitespace() {
        return Some(match prev {
            Some('.' | '!' | '?' | '…') => 10,
            Some(',' | ';' | ':') => 8,
            _ => 6,
        });
    }
    None
}

/// What a cut should stay out of. tdesktop `CutPart` breaks code, code blocks
/// and quotes freely and any other entity only when nothing else fits.
struct Cuts {
    spans: DisjointRanges,
    emoji: DisjointRanges,
}

impl Cuts {
    fn of(doc: &ComposerDoc) -> Self {
        let unbroken = doc
            .spans
            .iter()
            .filter(|span| {
                !matches!(
                    span.tag,
                    ComposerTag::Code | ComposerTag::Pre(_) | ComposerTag::Quote
                )
            })
            .map(|span| DocSpan::new(span.range.clone(), ComposerTag::Bold));
        let emoji = doc
            .emoji
            .iter()
            .map(|emoji| DocSpan::new(emoji.range.clone(), ComposerTag::Bold));
        Self {
            spans: DisjointRanges::of(unbroken),
            emoji: DisjointRanges::of(emoji),
        }
    }

    fn clean(&self, at: usize) -> bool {
        self.spans.around(at).is_none() && self.emoji.around(at).is_none()
    }

    /// `at`, or the start of the custom emoji it falls in when that is past
    /// `from`: an emoji cut in two would lose its picture.
    fn outside_emoji(&self, at: usize, from: usize) -> usize {
        self.emoji
            .around(at)
            .filter(|emoji| emoji.start > from)
            .map_or(at, |emoji| emoji.start)
    }
}

enum Cut {
    At(usize),
    RestFits,
    /// A scalar wider than the limit, which can never fit.
    Stuck,
}

/// Where the part of `text` from `from` ends: the best line, sentence or word
/// break in the second half of the window (tdesktop `CutPart`), or the limit.
fn next_cut(text: &str, from: usize, limit: usize, cuts: &Cuts) -> Cut {
    let rest = text.get(from..).unwrap_or_default();
    let mut candidates: Vec<(u8, usize)> = Vec::new();
    let (mut units, mut hard, mut prev) = (0, from, None);
    let mut chars = rest.char_indices().peekable();
    while let Some((rel, ch)) = chars.next() {
        let at = from + rel;
        let next = chars.peek().map(|&(_, c)| c);
        if units > limit / 2
            && let Some(level) = break_level(prev, ch, next)
        {
            candidates.push((level, at));
        }
        units += ch.len_utf16();
        if units > limit {
            scan_work::add(rel);
            candidates.sort_by(|a, b| b.cmp(a));
            let best = || candidates.iter().map(|&(_, at)| at);
            let cut = best()
                .find(|&at| cuts.clean(at))
                .or_else(|| best().find(|&at| cuts.outside_emoji(at, from) == at))
                .unwrap_or_else(|| cuts.outside_emoji(hard, from));
            return if cut > from { Cut::At(cut) } else { Cut::Stuck };
        }
        hard = at + ch.len_utf8();
        prev = Some(ch);
    }
    scan_work::add(rest.len());
    Cut::RestFits
}

/// Split composer markup into messages of at most `limit` units each,
/// cutting the parsed text as tdesktop's `CutPart` does, so formatting carries
/// over a cut and no marker is left behind. Whitespace at the seams is
/// trimmed. Falls back to a hard cut at the limit for text without any usable
/// break.
///
/// Returns the input unchanged (one element) when it already fits, and an
/// empty vector for blank input or a non-positive `limit`.
pub fn split_markup_text(markup: &str, limit: i32) -> Vec<String> {
    let Ok(limit) = usize::try_from(limit) else {
        return Vec::new();
    };
    let markup = markup.trim();
    if limit == 0 || markup.is_empty() {
        return Vec::new();
    }
    let doc = ComposerDoc::from_markup(markup);
    if utf16_len(&doc.text) <= limit {
        return vec![markup.to_string()];
    }
    let cuts = Cuts::of(&doc);
    let text = doc.text.as_str();
    let skip_space = |at: usize| {
        let rest = text.get(at..).unwrap_or_default();
        text.len() - rest.trim_start().len()
    };
    let mut ranges = Vec::new();
    let mut from = skip_space(0);
    while from < text.len() {
        match next_cut(text, from, limit, &cuts) {
            Cut::At(cut) => {
                let head = text.get(from..cut).unwrap_or_default();
                ranges.push(from..from + head.trim_end().len());
                from = skip_space(cut);
            }
            Cut::RestFits => {
                let rest = text.get(from..).unwrap_or_default();
                ranges.push(from..from + rest.trim_end().len());
                break;
            }
            Cut::Stuck => break,
        }
    }
    doc.slices(&ranges)
        .iter()
        .map(ComposerDoc::to_final_markup)
        .collect()
}

#[cfg(test)]
#[path = "text_split_scale_tests.rs"]
mod scale_tests;

#[cfg(test)]
mod tests {
    use super::{markup_text_units, split_markup_text, units_over_limit, utf16_len};
    use crate::composer::parse_format_markup;
    use crate::composer_doc::{ComposerDoc, ComposerTag};

    #[test]
    fn short_text_is_untouched() {
        assert_eq!(split_markup_text("hello there", 4096), vec!["hello there"]);
        assert_eq!(units_over_limit("hello", 4096), 0);
    }

    #[test]
    fn counts_utf16_units_after_markup() {
        assert_eq!(utf16_len("a😀"), 3);
        assert_eq!(markup_text_units("**bold** x"), 6);
        assert_eq!(units_over_limit("**bold** x", 4), 2);
    }

    #[test]
    fn splits_at_word_boundaries_within_the_limit() {
        let text = "alpha beta gamma delta epsilon zeta eta theta iota kappa";
        let parts = split_markup_text(text, 20);
        assert!(parts.len() >= 3);
        for part in &parts {
            assert!(markup_text_units(part) <= 20, "{part:?}");
        }
        assert_eq!(parts.join(" "), text);
    }

    #[test]
    fn prefers_paragraph_breaks() {
        let text = format!("{}\n\n{}", "a".repeat(30), "b ".repeat(10).trim());
        let parts = split_markup_text(&text, 40);
        assert_eq!(parts[0], "a".repeat(30));
        assert!(parts[1].starts_with('b'));
    }

    #[test]
    fn hard_cuts_text_without_breaks() {
        let parts = split_markup_text(&"x".repeat(25), 10);
        assert_eq!(
            parts.iter().map(String::len).collect::<Vec<_>>(),
            vec![10, 10, 5]
        );
    }

    #[test]
    fn never_splits_a_surrogate_pair_or_scalar() {
        let parts = split_markup_text(&"😀".repeat(9), 5);
        for part in &parts {
            assert!(utf16_len(part) <= 5);
        }
        assert_eq!(parts.concat(), "😀".repeat(9));
    }

    #[test]
    fn keeps_a_bold_span_whole_when_a_clean_seam_exists() {
        let text = "one two three **bold words here** four five six seven eight";
        let parts = split_markup_text(text, 30);
        assert!(parts.len() >= 2);
        for part in &parts {
            let opens = part.matches("**").count();
            assert!(opens % 2 == 0, "split inside a span: {part:?}");
        }
    }

    #[test]
    fn a_long_quote_stays_quoted_on_both_sides_of_a_cut() {
        let text = (0..12)
            .map(|i| format!("> quoted line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let parts = split_markup_text(&text, 40);
        assert!(parts.len() >= 2);
        for part in &parts {
            assert!(part.lines().all(|line| line.starts_with("> ")), "{part:?}");
        }
    }

    #[test]
    fn a_link_at_the_limit_stays_whole() {
        let link = "[a link](https://example.com/some/long/path)";
        let text = format!("one two three four five six {link} and then more words");
        let parts = split_markup_text(&text, 30);
        assert!(parts.iter().any(|part| part.contains(link)), "{parts:?}");
    }

    #[test]
    fn parts_fit_and_no_text_is_lost() {
        let line = "a **bold run** with `code`, a [link](https://t.me/x) and _more_ words.";
        let text = vec![line; 200].join("\n");
        let parts = split_markup_text(&text, 256);
        assert!(parts.len() > 1);
        for part in &parts {
            assert!(markup_text_units(part) <= 256, "{part:?}");
        }
        let squashed = |text: &str| text.split_whitespace().collect::<String>();
        let parsed = |text: &str| parse_format_markup(text).0;
        let joined: String = parts.iter().map(|part| parsed(part)).collect();
        assert_eq!(squashed(&joined), squashed(&parsed(&text)));
    }

    #[test]
    fn a_span_longer_than_the_limit_stays_formatted_on_both_sides() {
        let text = format!("**{}**", "word ".repeat(30).trim());
        let parts = split_markup_text(&text, 40);
        assert!(parts.len() > 1);
        for part in &parts {
            let doc = ComposerDoc::from_markup(part);
            assert!(!doc.text.contains('*'), "{part:?}");
            assert_eq!(doc.spans.len(), 1, "{part:?}");
        }
    }

    #[test]
    fn a_code_block_longer_than_the_limit_keeps_its_language() {
        let text = format!("```rust\n{}\n```", "let a = 1;\n".repeat(20).trim());
        let parts = split_markup_text(&text, 50);
        assert!(parts.len() > 1);
        for part in &parts {
            let doc = ComposerDoc::from_markup(part);
            assert!(!doc.text.contains('`'), "{part:?}");
            assert_eq!(
                doc.spans.first().map(|span| &span.tag),
                Some(&ComposerTag::Pre("rust".into()))
            );
        }
    }

    #[test]
    fn a_hard_cut_leaves_a_custom_emoji_whole() {
        let text = format!("{}![long label](tg://emoji?id=7)", "x".repeat(16));
        let parts = split_markup_text(&text, 20);
        assert_eq!(parts.len(), 2);
        assert_eq!(ComposerDoc::from_markup(&parts[1]).emoji.len(), 1);
    }

    #[test]
    fn parts_read_as_the_whole_does() {
        let mut random = crate::composer_doc::Random::new(0x2545_f491_4f6c_dd1d);
        let mut failed = Vec::new();
        for _ in 0..24_000 {
            let pieces = 1 + random.below(40);
            let markup = random.markup(pieces);
            let limit = [16, 33, 64, 100][random.below(4)];
            let parts = split_markup_text(&markup, limit);
            let whole = tagged_chars(&ComposerDoc::from_markup(markup.trim()));
            let joined: Vec<_> = parts
                .iter()
                .flat_map(|part| tagged_chars(&ComposerDoc::from_markup(part)))
                .collect();
            let too_long = parts
                .iter()
                .any(|part| markup_text_units(part) > limit as usize);
            if too_long || joined != whole {
                failed.push(format!("{limit} {markup:?}\n  {parts:?}"));
            }
        }
        let count = failed.len();
        failed.truncate(6);
        assert!(count == 0, "{count} failed\n{}", failed.join("\n"));
    }

    fn tagged_chars(doc: &ComposerDoc) -> Vec<(char, Vec<String>)> {
        doc.text
            .char_indices()
            .filter(|(_, ch)| !ch.is_whitespace())
            .map(|(at, ch)| {
                let mut tags: Vec<&ComposerTag> = doc
                    .spans
                    .iter()
                    .filter(|span| span.range.contains(&at))
                    .map(|span| &span.tag)
                    .collect();
                let block = tags.iter().find(|tag| matches!(tag, ComposerTag::Pre(_)));
                if let Some(&block) = block {
                    tags = vec![block];
                }
                let mut tags: Vec<String> = tags.iter().map(|tag| tag.to_tag()).collect();
                tags.sort();
                (ch, tags)
            })
            .collect()
    }

    #[test]
    fn a_cut_that_changes_how_a_part_reads_still_fits() {
        let texts = [
            "`a`b[label](https://t.me/a)c**d**e**f**g`h[label](https://t.me/a)i`j".repeat(6),
            "one two\n> \n> three four\n> \nfive six\n> ".repeat(6),
        ];
        for text in &texts {
            for limit in 8..=128 {
                for part in split_markup_text(text, limit) {
                    let units = markup_text_units(&part);
                    assert!(units <= limit as usize, "{limit}: {part:?}");
                }
            }
        }
    }

    #[test]
    fn blank_or_zero_limit_yields_nothing() {
        assert!(split_markup_text("   ", 10).is_empty());
        assert!(split_markup_text("abc", 0).is_empty());
    }
}
