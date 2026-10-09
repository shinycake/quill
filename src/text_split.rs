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

use crate::composer::parse_format_markup;

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
fn largest_fitting_prefix(rest: &str, max: usize) -> usize {
    let bounds: Vec<usize> = rest
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(rest.len()))
        .collect();
    let (mut lo, mut hi) = (0usize, bounds.len() - 1);
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if markup_text_units(&rest[..bounds[mid]]) <= max {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    bounds[lo]
}

/// A cut at `at` keeps formatting intact when parsing the two halves
/// separately yields the same text and the same number of entities as
/// parsing the whole (no span straddles the cut, no marker was split).
fn cut_preserves_formatting(rest: &str, at: usize) -> bool {
    let (whole_text, whole_entities) = parse_format_markup(rest);
    let (head_text, head_entities) = parse_format_markup(&rest[..at]);
    let (tail_text, tail_entities) = parse_format_markup(&rest[at..]);
    head_text.len() + tail_text.len() == whole_text.len()
        && head_entities.len() + tail_entities.len() == whole_entities.len()
        && format!("{head_text}{tail_text}") == whole_text
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
    let Some(limit) = usize::try_from(limit).ok().filter(|l| *l > 0) else {
        return Vec::new();
    };
    let mut parts = Vec::new();
    let mut rest = markup.trim().to_string();
    while !rest.is_empty() {
        if markup_text_units(&rest) <= limit {
            parts.push(rest);
            break;
        }
        let hard = largest_fitting_prefix(&rest, limit);
        if hard == 0 {
            // A single scalar wider than the limit: cannot ever fit.
            break;
        }
        let half = largest_fitting_prefix(&rest, limit / 2);
        // Candidate seams in the second half of the window, best level
        // first, latest first within a level.
        let chars: Vec<(usize, char)> = rest.char_indices().collect();
        let mut candidates: Vec<(u8, usize)> = Vec::new();
        for (n, &(at, ch)) in chars.iter().enumerate() {
            if at <= half || at > hard {
                continue;
            }
            let prev = n.checked_sub(1).map(|p| chars[p].1);
            let next = chars.get(n + 1).map(|&(_, c)| c);
            if let Some(level) = break_level(prev, ch, next) {
                candidates.push((level, at));
            }
        }
        candidates.sort_by(|a, b| b.cmp(a));
        let cut = candidates
            .iter()
            .take(48)
            .map(|&(_, at)| at)
            .find(|&at| cut_preserves_formatting(&rest, at))
            .or_else(|| candidates.first().map(|&(_, at)| at))
            .unwrap_or(hard);
        let head = rest[..cut].trim_end();
        let tail = rest[cut..].trim_start();
        if head.is_empty() || tail.len() >= rest.len() {
            // Defensive: guarantee progress.
            parts.push(rest[..hard].to_string());
            rest = rest[hard..].trim_start().to_string();
            continue;
        }
        parts.push(head.to_string());
        rest = tail.to_string();
    }
    parts
}

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
    fn blank_or_zero_limit_yields_nothing() {
        assert!(split_markup_text("   ", 10).is_empty());
        assert!(split_markup_text("abc", 0).is_empty());
    }
}
