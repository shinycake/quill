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

use crate::composer::{MarkupScanner, Seam, parse_format_markup, scan_work};

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

/// Largest char-boundary byte index `b` with `units(rest[..b]) <= max`.
fn largest_fitting_prefix(rest: &str, seams: &[Seam], max: i32) -> usize {
    let first_over = seams.partition_point(|seam| seam.utf16_units <= max);
    let fits = first_over
        .checked_sub(1)
        .and_then(|last| seams.get(last))
        .map_or(0, |seam| seam.byte);
    let over = seams.get(first_over).map_or(rest.len(), |seam| seam.byte);
    let max_units = usize::try_from(max).unwrap_or(0);
    last_fitting_boundary(rest, fits, over, |bound| {
        markup_text_units(rest.get(..bound).unwrap_or_default()) <= max_units
    })
}

fn last_fitting_boundary(
    rest: &str,
    from: usize,
    to: usize,
    fits: impl Fn(usize) -> bool,
) -> usize {
    scan_work::add(to.saturating_sub(from));
    let bounds: Vec<usize> = rest
        .get(from..to)
        .unwrap_or_default()
        .char_indices()
        .map(|(i, _)| from + i)
        .collect();
    let (mut lo, mut hi) = (0usize, bounds.len().saturating_sub(1));
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let Some(&bound) = bounds.get(mid) else {
            break;
        };
        if fits(bound) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    bounds.get(lo).copied().unwrap_or(from)
}

fn fitting_hard_cut(rest: &str, hard: usize, limit: i32) -> usize {
    if head_fits(rest, hard, limit) {
        return hard;
    }
    match last_fitting_boundary(rest, 0, hard, |at| head_fits(rest, at, limit)) {
        0 => hard,
        at => at,
    }
}

fn break_candidates(rest: &str, half: usize, hard: usize) -> Vec<(u8, usize)> {
    scan_work::add(hard.saturating_sub(half));
    let mut candidates: Vec<(u8, usize)> = Vec::new();
    let mut prev = None;
    let mut chars = rest
        .get(half..)
        .unwrap_or_default()
        .char_indices()
        .peekable();
    while let Some((rel, ch)) = chars.next() {
        let at = half + rel;
        if at > hard {
            break;
        }
        let next = chars.peek().map(|&(_, c)| c);
        if at > half
            && let Some(level) = break_level(prev, ch, next)
        {
            candidates.push((level, at));
        }
        prev = Some(ch);
    }
    candidates.sort_by(|a, b| b.cmp(a));
    candidates
}

fn head_fits(rest: &str, at: usize, limit: i32) -> bool {
    let head = rest.get(..at).unwrap_or_default().trim_end();
    i32::try_from(markup_text_units(head)).is_ok_and(|units| units <= limit)
}

fn cut_preserves_formatting(seams: &[Seam], at: usize) -> bool {
    seams
        .binary_search_by_key(&at, |seam| seam.byte)
        .is_ok_and(|ix| seams.get(ix).is_some_and(|seam| seam.outside_spans))
}

/// Split composer markup into messages of at most `limit` units each,
/// cutting at the best line / sentence / word break in the second half of
/// the window (tdesktop `CutPart`) and never through a formatting span
/// when a clean cut exists. Whitespace at the seams is trimmed. Falls back
/// to a hard cut at the limit for text without any usable break.
///
/// Returns the input unchanged (one element) when it already fits, and an
/// empty vector for blank input or a non-positive `limit`.
pub fn split_markup_text(markup: &str, limit: i32) -> Vec<String> {
    if limit <= 0 {
        return Vec::new();
    }
    let mut parts = Vec::new();
    let mut scanner = MarkupScanner::new(markup.trim());
    loop {
        let rest = scanner.rest();
        if rest.is_empty() {
            break;
        }
        let Some(seams) = scanner.scan(limit) else {
            parts.push(rest.to_string());
            break;
        };
        let hard = largest_fitting_prefix(rest, seams, limit);
        if hard == 0 {
            // A single scalar wider than the limit: cannot ever fit.
            break;
        }
        let half = largest_fitting_prefix(rest, seams, limit / 2);
        let candidates = break_candidates(rest, half, hard);
        let best = || candidates.iter().take(48).map(|&(_, at)| at);
        let fits = |at: usize| head_fits(rest, at, limit);
        let cut = best()
            .find(|&at| cut_preserves_formatting(seams, at) && fits(at))
            .or_else(|| best().find(|&at| fits(at)))
            .unwrap_or_else(|| fitting_hard_cut(rest, hard, limit));
        let head = rest.get(..cut).unwrap_or_default().trim_end();
        let tail = rest.get(cut..).unwrap_or_default().trim_start();
        if head.is_empty() || tail.len() >= rest.len() {
            // Defensive: guarantee progress.
            parts.push(rest.get(..hard).unwrap_or_default().to_string());
            let tail = rest.get(hard..).unwrap_or_default().trim_start();
            scanner.advance(rest.len() - tail.len());
            continue;
        }
        parts.push(head.to_string());
        scanner.advance(rest.len() - tail.len());
    }
    parts
}

#[cfg(test)]
#[path = "text_split_scale_tests.rs"]
mod scale_tests;

#[cfg(test)]
mod tests {
    use super::{markup_text_units, split_markup_text, units_over_limit, utf16_len};

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
        assert_eq!(squashed(&parts.concat()), squashed(&text));
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
