//! Fuzzy matching for settings search: the query's letters in order, scored
//! for word starts and runs, with one typo per word forgiven as a fallback.
//!
//! Text is folded first (lowercase, combining marks removed), so "Português"
//! matches "portugues". Positions in a [`Match`] are char indices into the
//! original text, for highlighting.

use icu_normalizer::DecomposingNormalizerBorrowed;

/// One folded char and the index of the original char it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Folded {
    ch: char,
    source: usize,
}

/// A successful match of a query against one text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// Higher is better. Only comparable between matches of the same query.
    pub score: i32,
    /// Char indices of the matched letters in the original text, ascending.
    /// Empty for a typo match, which has no letter-by-letter alignment.
    pub positions: Vec<usize>,
}

const MATCHED_CHAR: i32 = 10;
const WORD_START: i32 = 80;
const RUN: i32 = 50;
const GAP_STEP: i32 = 6;
const GAP_MAX: i32 = 80;
const FIRST_CHAR: i32 = 30;
/// The query covers every letter of the text: "sign out" is the keyword, not
/// the start of "sign out all".
const WHOLE: i32 = 60;
/// A typo match scores below any letter-by-letter match of the same length.
const TYPO_PER_WORD: i32 = 40;

fn fold(text: &str) -> Vec<Folded> {
    let normalizer = DecomposingNormalizerBorrowed::new_nfd();
    let mut out = Vec::with_capacity(text.len());
    for (source, ch) in text.chars().enumerate() {
        let mut buf = [0u8; 4];
        for decomposed in normalizer.normalize(ch.encode_utf8(&mut buf)).chars() {
            if matches!(decomposed as u32, 0x0300..=0x036F) {
                continue;
            }
            for lower in decomposed.to_lowercase() {
                out.push(Folded { ch: lower, source });
            }
        }
    }
    out
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric()
}

/// The query as folded letters, separators dropped.
fn query_letters(query: &str) -> Vec<char> {
    fold(query)
        .into_iter()
        .map(|f| f.ch)
        .filter(|c| is_word_char(*c))
        .collect()
}

/// Folded words of `text` (letters and digits only).
pub fn words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for f in fold(text) {
        if is_word_char(f.ch) {
            current.push(f.ch);
        } else if !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// Best in-order alignment of `query` in `text`. `None` when a letter is
/// missing or the letters are too scattered to be a plausible match.
pub fn subsequence(query: &str, text: &str) -> Option<Match> {
    let q = query_letters(query);
    if q.is_empty() {
        return None;
    }
    let t = fold(text);
    let n = t.len();
    if q.len() > n {
        return None;
    }
    let word_start: Vec<bool> = (0..n)
        .map(|j| {
            let ch = t.get(j).map_or(' ', |f| f.ch);
            let prev = j.checked_sub(1).and_then(|p| t.get(p)).map(|f| f.ch);
            is_word_char(ch) && prev.is_none_or(|p| !is_word_char(p))
        })
        .collect();

    // best[i][j]: best score with query letter i matched at text char j.
    let mut best: Vec<Vec<Option<(i32, usize)>>> = vec![vec![None; n]; q.len()];
    for (i, qc) in q.iter().enumerate() {
        for j in 0..n {
            if t.get(j).map(|f| f.ch) != Some(*qc) {
                continue;
            }
            let start = word_start.get(j).copied().unwrap_or(false);
            let gain = MATCHED_CHAR + if start { WORD_START } else { 0 };
            let cell = if i == 0 {
                let lead = if j == 0 { FIRST_CHAR } else { 0 };
                Some((gain + lead, usize::MAX))
            } else {
                let prev_row = best.get(i - 1)?;
                (0..j)
                    .filter_map(|p| {
                        let (score, _) = (*prev_row.get(p)?)?;
                        let gap = (j - p - 1) as i32;
                        let bonus = if gap == 0 { RUN } else { 0 };
                        let penalty = if start {
                            0
                        } else {
                            (gap * GAP_STEP).min(GAP_MAX)
                        };
                        Some((score + gain + bonus - penalty, p))
                    })
                    .max_by_key(|(score, _)| *score)
            };
            if let Some(row) = best.get_mut(i)
                && let Some(slot) = row.get_mut(j)
            {
                *slot = cell;
            }
        }
    }

    let last = best.last()?;
    let (end, (score, _)) = last
        .iter()
        .enumerate()
        .filter_map(|(j, cell)| cell.map(|c| (j, c)))
        .max_by_key(|(_, (score, _))| *score)?;

    // Too scattered: on average every letter needs a word start or a run.
    if score < q.len() as i32 * (MATCHED_CHAR + RUN / 2) {
        return None;
    }

    let letters = t.iter().filter(|f| is_word_char(f.ch)).count();
    let score = if letters == q.len() {
        score + WHOLE
    } else {
        score
    };

    let mut positions = Vec::with_capacity(q.len());
    let mut j = end;
    for i in (0..q.len()).rev() {
        positions.push(t.get(j)?.source);
        let (_, prev) = (*best.get(i)?.get(j)?)?;
        if prev == usize::MAX {
            break;
        }
        j = prev;
    }
    positions.reverse();
    positions.dedup();
    Some(Match { score, positions })
}

/// Edit distance between `a` and `b` is at most one (insert, delete,
/// substitute or swap two neighbours).
fn within_one_edit(a: &[char], b: &[char]) -> bool {
    if a == b {
        return true;
    }
    let (la, lb) = (a.len(), b.len());
    if la.abs_diff(lb) > 1 {
        return false;
    }
    let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let rest_a = a.get(prefix..).unwrap_or_default();
    let rest_b = b.get(prefix..).unwrap_or_default();
    match la.cmp(&lb) {
        std::cmp::Ordering::Equal => {
            rest_a.get(1..) == rest_b.get(1..)
                || (rest_a.len() >= 2
                    && rest_a.first() == rest_b.get(1)
                    && rest_a.get(1) == rest_b.first()
                    && rest_a.get(2..) == rest_b.get(2..))
        }
        std::cmp::Ordering::Greater => rest_a.get(1..) == Some(rest_b),
        std::cmp::Ordering::Less => Some(rest_a) == rest_b.get(1..),
    }
}

/// Every query word of four or more letters is one edit from a word of
/// `text` (or from the start of one). A three-letter word may miss one letter
/// of a word's start ("drk" for "dark"); shorter words must be prefixes.
pub fn typo(query: &str, text: &str) -> Option<Match> {
    let wanted = words(query);
    if wanted.is_empty() || wanted.iter().all(|w| w.chars().count() < 4) {
        return None;
    }
    let have: Vec<Vec<char>> = words(text).iter().map(|w| w.chars().collect()).collect();
    let mut score = 0;
    for word in &wanted {
        let w: Vec<char> = word.chars().collect();
        let hit = have.iter().any(|h| {
            if w.len() < 3 {
                return h.starts_with(&w);
            }
            if w.len() == 3 {
                return h.starts_with(&w) || h.get(..4).is_some_and(|p| within_one_edit(&w, p));
            }
            within_one_edit(&w, h)
                || h.get(..w.len().min(h.len()))
                    .is_some_and(|p| within_one_edit(&w, p))
                || h.get(..(w.len() + 1).min(h.len()))
                    .is_some_and(|p| within_one_edit(&w, p))
        });
        if !hit {
            return None;
        }
        score += TYPO_PER_WORD + w.len() as i32 * MATCHED_CHAR;
    }
    Some(Match {
        score,
        positions: Vec::new(),
    })
}

/// The best of a letter-by-letter match and a typo match.
pub fn best(query: &str, text: &str) -> Option<Match> {
    subsequence(query, text).or_else(|| typo(query, text))
}
