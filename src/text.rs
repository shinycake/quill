//! UTF-16 entity offsets used by TDLib, mapped to UTF-8 byte indices.

/// Convert a UTF-8 byte index into a UTF-16 code-unit offset.
///
/// The byte index must land on a character boundary. Surrogate pairs (non-BMP
/// code points, including many emoji) count as two UTF-16 units.
pub fn utf8_to_utf16_offset(text: &str, byte_index: usize) -> Result<i32, OffsetError> {
    if byte_index > text.len() {
        return Err(OffsetError::OutOfRange);
    }
    if byte_index < text.len() && !text.is_char_boundary(byte_index) {
        return Err(OffsetError::NotCharBoundary);
    }
    let mut units = 0i32;
    for (i, ch) in text.char_indices() {
        if i == byte_index {
            return Ok(units);
        }
        if i > byte_index {
            return Err(OffsetError::NotCharBoundary);
        }
        units = units.saturating_add(ch.len_utf16() as i32);
    }
    if byte_index == text.len() {
        Ok(units)
    } else {
        Err(OffsetError::NotCharBoundary)
    }
}

/// Convert a UTF-16 code-unit offset into a UTF-8 byte index.
pub fn utf16_to_utf8_offset(text: &str, utf16_offset: i32) -> Result<usize, OffsetError> {
    if utf16_offset < 0 {
        return Err(OffsetError::OutOfRange);
    }
    let mut remaining = utf16_offset as usize;
    for (i, ch) in text.char_indices() {
        if remaining == 0 {
            return Ok(i);
        }
        let width = ch.len_utf16();
        if remaining < width {
            // Offset landed inside a surrogate pair.
            return Err(OffsetError::InsideSurrogatePair);
        }
        remaining -= width;
    }
    if remaining == 0 {
        Ok(text.len())
    } else {
        Err(OffsetError::OutOfRange)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OffsetError {
    OutOfRange,
    NotCharBoundary,
    InsideSurrogatePair,
}

/// True when `needle` appears as a contiguous substring (used by log canaries).
pub fn contains_canary(haystack: &str, needle: &str) -> bool {
    haystack.contains(needle)
}

/// Styled text entities Quill paints in message text and captions (Phase 4.1).
///
/// TDLib `textEntity` offsets are UTF-16. Callers convert them to UTF-8 byte
/// indices before storing a span. Entity type constructors are verified
/// against `schema/td_api.tl` (1.8.67, lines 5719–5785); anything not listed
/// here (mentions, hashtags, phone numbers, bank-card numbers, block quotes,
/// custom emoji, media timestamps, dates, …) stays unparsed and unstyled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextEntityKind {
    /// `textEntityTypeUrl` — the substring is the HTTP URL.
    Url,
    /// `textEntityTypeTextUrl` — visible label, `url` is opened on click.
    TextUrl { url: String },
    /// `textEntityTypeBold`
    Bold,
    /// `textEntityTypeItalic`
    Italic,
    /// `textEntityTypeUnderline`
    Underline,
    /// `textEntityTypeStrikethrough`
    Strikethrough,
    /// `textEntityTypeSpoiler` — hidden until tapped.
    Spoiler,
    /// `textEntityTypeCode` — inline monospace chip.
    Code,
    /// `textEntityTypePre` — monospace block, no language.
    Pre,
    /// `textEntityTypePreCode language:string` — monospace block with language.
    PreCode { language: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEntity {
    pub utf8_start: usize,
    pub utf8_end: usize,
    pub kind: TextEntityKind,
}

impl TextEntity {
    /// URL passed to the OS opener. Only `http` and `https` (no shell).
    pub fn open_href<'a>(&'a self, text: &'a str) -> Option<&'a str> {
        let raw = match &self.kind {
            TextEntityKind::Url => text.get(self.utf8_start..self.utf8_end)?,
            TextEntityKind::TextUrl { url } => url.as_str(),
            // Style entities carry no link target.
            _ => return None,
        };
        openable_http_url(raw).then_some(raw.trim())
    }
}

/// One painted slice of message text. `href` is set for clickable links;
/// `style` carries the Phase 4.1 entity styling for the same slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextRun {
    pub text: String,
    pub href: Option<String>,
    pub style: RunStyle,
}

/// Combined styling for one painted run (Phase 4.1).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    /// Spoiler: hidden until tapped; the UI keeps reveal state.
    pub spoiler: bool,
    /// Inline `code`: monospace chip.
    pub code: bool,
    /// `pre` / `preCode`: monospace block.
    pub pre: bool,
    /// Language from `textEntityTypePreCode`; `None` for plain `pre`.
    pub language: Option<String>,
}

impl RunStyle {
    /// True when the run carries no entity styling at all.
    pub fn is_plain(&self) -> bool {
        self == &RunStyle::default()
    }
}

/// Split `text` into painted runs honoring every entity, nested or not.
///
/// Nesting rule (Phase 4.1): the text is cut at every entity boundary, so
/// each emitted run is covered by one fixed set of entities. Styles combine
/// additively across nested / partially overlapping entities (bold inside
/// italic renders bold italic; `code` inside a spoiler renders a hidden code
/// chip; `pre` inside a spoiler renders a hidden block). Adjacent runs with
/// identical styling are merged back together.
///
/// Degradation rules, all deterministic and panic-free:
/// - `href`: the entity with the smallest `(utf8_start, utf8_end)` wins per
///   run. The schema forbids Url / TextUrl nesting, so this only matters for
///   malformed input.
/// - `pre` wins over `code` for the block look; both stay monospace.
/// - Degenerate entities (zero length, outside the text, or splitting a
///   UTF-8 char boundary) are dropped.
pub fn styled_runs(text: &str, entities: &[TextEntity]) -> Vec<TextRun> {
    let mut spans: Vec<&TextEntity> = entities
        .iter()
        .filter(|entity| {
            entity.utf8_start < entity.utf8_end
                && entity.utf8_end <= text.len()
                && text.is_char_boundary(entity.utf8_start)
                && text.is_char_boundary(entity.utf8_end)
        })
        .collect();
    spans.sort_by_key(|entity| (entity.utf8_start, entity.utf8_end));
    let mut points = vec![0usize, text.len()];
    for entity in &spans {
        points.push(entity.utf8_start);
        points.push(entity.utf8_end);
    }
    points.sort_unstable();
    points.dedup();
    let mut runs: Vec<TextRun> = Vec::new();
    for window in points.windows(2) {
        let (start, end) = (window[0], window[1]);
        if start >= end {
            continue;
        }
        let mut style = RunStyle::default();
        let mut href: Option<String> = None;
        for entity in &spans {
            if entity.utf8_start > start || entity.utf8_end < end {
                continue;
            }
            match &entity.kind {
                TextEntityKind::Url | TextEntityKind::TextUrl { .. } => {
                    if href.is_none() {
                        href = entity.open_href(text).map(str::to_string);
                    }
                }
                TextEntityKind::Bold => style.bold = true,
                TextEntityKind::Italic => style.italic = true,
                TextEntityKind::Underline => style.underline = true,
                TextEntityKind::Strikethrough => style.strikethrough = true,
                TextEntityKind::Spoiler => style.spoiler = true,
                TextEntityKind::Code => style.code = true,
                TextEntityKind::Pre => style.pre = true,
                TextEntityKind::PreCode { language } => {
                    style.pre = true;
                    if style.language.is_none() {
                        style.language = Some(language.clone());
                    }
                }
            }
        }
        let slice = text[start..end].to_string();
        if let Some(last) = runs.last_mut()
            && last.style == style
            && last.href == href
        {
            last.text.push_str(&slice);
        } else {
            runs.push(TextRun {
                text: slice,
                href,
                style,
            });
        }
    }
    runs
}

/// `http`/`https` only, no whitespace or control characters (passed to xdg-open/open).
pub fn openable_http_url(url: &str) -> bool {
    let url = url.trim();
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"));
    let Some(rest) = rest else {
        return false;
    };
    !rest.is_empty()
        && url
            .chars()
            .all(|ch| !ch.is_whitespace() && !ch.is_control())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_round_trip() {
        let text = "hello";
        assert_eq!(utf8_to_utf16_offset(text, 0).unwrap(), 0);
        assert_eq!(utf8_to_utf16_offset(text, 5).unwrap(), 5);
        assert_eq!(utf16_to_utf8_offset(text, 2).unwrap(), 2);
    }

    #[test]
    fn hebrew_and_latin() {
        let text = "שלום hello";
        let hello_start = text.find("hello").unwrap();
        let utf16 = utf8_to_utf16_offset(text, hello_start).unwrap();
        assert_eq!(utf16_to_utf8_offset(text, utf16).unwrap(), hello_start);
        assert_eq!(utf8_to_utf16_offset(text, text.len()).unwrap(), 10);
    }

    #[test]
    fn arabic_digits_and_link() {
        let text = "مرحبا https://example.com 123";
        let link = text.find("https").unwrap();
        let utf16 = utf8_to_utf16_offset(text, link).unwrap();
        assert_eq!(utf16_to_utf8_offset(text, utf16).unwrap(), link);
    }

    #[test]
    fn emoji_family_zwj_is_non_bmp() {
        // Woman+ZWJ+woman+ZWJ+girl — several non-BMP scalars.
        let text = "hello 👩‍👧‍👦 world";
        let emoji_at = text.find('👩').unwrap();
        let utf16 = utf8_to_utf16_offset(text, emoji_at).unwrap();
        assert_eq!(utf16_to_utf8_offset(text, utf16).unwrap(), emoji_at);
        let after = text.find(" world").unwrap();
        let utf16_after = utf8_to_utf16_offset(text, after).unwrap();
        assert_eq!(utf16_to_utf8_offset(text, utf16_after).unwrap(), after);
        assert!(utf16_after > utf16 + 1);
    }

    #[test]
    fn skin_tone_and_combining_accent() {
        let text = "e\u{0301} 👍🏽";
        assert_eq!(
            utf16_to_utf8_offset(text, utf8_to_utf16_offset(text, 0).unwrap()).unwrap(),
            0
        );
        let thumb = text.find('👍').unwrap();
        let utf16 = utf8_to_utf16_offset(text, thumb).unwrap();
        assert_eq!(utf16_to_utf8_offset(text, utf16).unwrap(), thumb);
    }

    #[test]
    fn cjk_offsets() {
        let text = "日本語入力";
        assert_eq!(utf8_to_utf16_offset(text, text.len()).unwrap(), 5);
        assert_eq!(utf16_to_utf8_offset(text, 2).unwrap(), "日本".len());
    }

    #[test]
    fn rejects_mid_char_byte_index() {
        let text = "é";
        assert_eq!(
            utf8_to_utf16_offset(text, 1),
            Err(OffsetError::NotCharBoundary)
        );
    }

    #[test]
    fn rejects_offset_inside_surrogate_pair() {
        let text = "𝄞"; // U+1D11E, two UTF-16 units, four UTF-8 bytes
        assert_eq!(
            utf16_to_utf8_offset(text, 1),
            Err(OffsetError::InsideSurrogatePair)
        );
    }

    #[test]
    fn url_and_text_url_runs_use_utf8_spans() {
        let text = "see https://example.com now";
        let start = text.find("https").unwrap();
        let end = start + "https://example.com".len();
        let entities = vec![
            entity(start, end, TextEntityKind::Url),
            entity(
                text.find("now").unwrap(),
                text.len(),
                TextEntityKind::TextUrl {
                    url: "https://example.com/notes".into(),
                },
            ),
        ];
        let runs = styled_runs(text, &entities);
        assert_eq!(runs[0].text, "see ");
        assert!(runs[0].href.is_none());
        assert!(runs[0].style.is_plain());
        assert_eq!(runs[1].href.as_deref(), Some("https://example.com"));
        assert_eq!(runs[2].text, " ");
        assert_eq!(runs[3].text, "now");
        assert_eq!(runs[3].href.as_deref(), Some("https://example.com/notes"));
    }

    #[test]
    fn emoji_prefix_does_not_shift_link_bytes() {
        let text = "👋 https://example.com";
        let start = text.find("https").unwrap();
        let runs = styled_runs(text, &[entity(start, text.len(), TextEntityKind::Url)]);
        assert_eq!(runs[0].text, "👋 ");
        assert_eq!(runs[1].href.as_deref(), Some("https://example.com"));
    }

    #[test]
    fn rejects_non_http_and_first_link_wins_overlap() {
        let text = "javascript:alert(1) hi";
        let bad = entity(0, "javascript:alert(1)".len(), TextEntityKind::Url);
        let runs = styled_runs(text, &[bad]);
        assert!(runs.iter().all(|run| run.href.is_none()));
        assert!(!openable_http_url("javascript:alert(1)"));
        assert!(!openable_http_url("https://evil.com/a b"));
        assert!(openable_http_url("https://example.com/a"));

        // Overlapping links (forbidden by the schema) degrade deterministically:
        // the entity with the smallest (start, end) wins each run.
        let overlap = [
            entity(
                0,
                4,
                TextEntityKind::TextUrl {
                    url: "https://a.example".into(),
                },
            ),
            entity(
                2,
                6,
                TextEntityKind::TextUrl {
                    url: "https://b.example".into(),
                },
            ),
        ];
        let runs = styled_runs("abcdef", &overlap);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].text, "abcd");
        assert_eq!(runs[0].href.as_deref(), Some("https://a.example"));
        assert_eq!(runs[1].text, "ef");
        assert_eq!(runs[1].href.as_deref(), Some("https://b.example"));
    }

    fn entity(start: usize, end: usize, kind: TextEntityKind) -> TextEntity {
        TextEntity {
            utf8_start: start,
            utf8_end: end,
            kind,
        }
    }

    #[test]
    fn styled_runs_plain_text_is_one_run() {
        let runs = styled_runs("hello", &[]);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "hello");
        assert!(runs[0].style.is_plain());
        assert!(runs[0].href.is_none());
    }

    #[test]
    fn styled_runs_single_style_segments() {
        let text = "bold and italic";
        let runs = styled_runs(
            text,
            &[
                entity(0, 4, TextEntityKind::Bold),
                entity(9, 15, TextEntityKind::Italic),
            ],
        );
        assert_eq!(runs.len(), 3);
        assert_eq!(runs[0].text, "bold");
        assert!(runs[0].style.bold);
        assert_eq!(runs[1].text, " and ");
        assert!(runs[1].style.is_plain());
        assert_eq!(runs[2].text, "italic");
        assert!(runs[2].style.italic);
    }

    #[test]
    fn styled_runs_nested_styles_combine() {
        // "abcdef": bold over 0..6, italic over 2..4, strikethrough over 3..6.
        let text = "abcdef";
        let runs = styled_runs(
            text,
            &[
                entity(0, 6, TextEntityKind::Bold),
                entity(2, 4, TextEntityKind::Italic),
                entity(3, 6, TextEntityKind::Strikethrough),
            ],
        );
        assert_eq!(runs.len(), 4);
        assert_eq!(runs[0].text, "ab");
        assert!(runs[0].style.bold && !runs[0].style.italic);
        assert_eq!(runs[1].text, "c");
        assert!(runs[1].style.bold && runs[1].style.italic);
        assert_eq!(runs[2].text, "d");
        assert!(runs[2].style.bold && runs[2].style.italic && runs[2].style.strikethrough);
        assert_eq!(runs[3].text, "ef");
        assert!(runs[3].style.bold && runs[3].style.strikethrough && !runs[3].style.italic);
    }

    #[test]
    fn styled_runs_partial_overlap_splits_and_merges() {
        // bold 0..4, italic 2..6: runs are 0..2 bold, 2..4 bold+italic,
        // 4..6 italic.
        let runs = styled_runs(
            "abcdef",
            &[
                entity(0, 4, TextEntityKind::Bold),
                entity(2, 6, TextEntityKind::Italic),
            ],
        );
        assert_eq!(runs.len(), 3);
        assert!(runs[0].style.bold && !runs[0].style.italic);
        assert!(runs[1].style.bold && runs[1].style.italic);
        assert!(!runs[2].style.bold && runs[2].style.italic);
    }

    #[test]
    fn styled_runs_adjacent_same_style_merges() {
        let runs = styled_runs(
            "abcd",
            &[
                entity(0, 2, TextEntityKind::Bold),
                entity(2, 4, TextEntityKind::Bold),
            ],
        );
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "abcd");
        assert!(runs[0].style.bold);
    }

    #[test]
    fn styled_runs_code_pre_and_language() {
        let runs = styled_runs(
            "a b c",
            &[
                entity(2, 3, TextEntityKind::Code),
                entity(4, 5, TextEntityKind::Pre),
            ],
        );
        assert_eq!(runs.len(), 4);
        assert_eq!(runs[0].text, "a ");
        assert!(runs[0].style.is_plain());
        assert_eq!(runs[1].text, "b");
        assert!(runs[1].style.code && !runs[1].style.pre);
        assert_eq!(runs[2].text, " ");
        assert!(runs[2].style.is_plain());
        assert_eq!(runs[3].text, "c");
        assert!(runs[3].style.pre && !runs[3].style.code);
        assert!(runs[3].style.language.is_none());

        let runs = styled_runs(
            "rust",
            &[entity(
                0,
                4,
                TextEntityKind::PreCode {
                    language: "rust".into(),
                },
            )],
        );
        assert_eq!(runs.len(), 1);
        assert!(runs[0].style.pre);
        assert_eq!(runs[0].style.language.as_deref(), Some("rust"));
    }

    #[test]
    fn styled_runs_spoiler_and_link_combine_with_first_link_wins() {
        let text = "secret link";
        let runs = styled_runs(
            text,
            &[
                entity(0, 11, TextEntityKind::Spoiler),
                entity(
                    7,
                    11,
                    TextEntityKind::TextUrl {
                        url: "https://example.com".into(),
                    },
                ),
            ],
        );
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].text, "secret ");
        assert!(runs[0].style.spoiler);
        assert!(runs[0].href.is_none());
        assert_eq!(runs[1].text, "link");
        assert!(runs[1].style.spoiler);
        assert_eq!(runs[1].href.as_deref(), Some("https://example.com"));
    }

    #[test]
    fn styled_runs_drops_malformed_entities() {
        let text = "ok";
        let runs = styled_runs(
            text,
            &[
                // zero length
                entity(1, 1, TextEntityKind::Bold),
                // past the end
                entity(0, 99, TextEntityKind::Italic),
                // negative-ish (wraps to huge usize start)
                TextEntity {
                    utf8_start: usize::MAX - 1,
                    utf8_end: usize::MAX,
                    kind: TextEntityKind::Underline,
                },
            ],
        );
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "ok");
        assert!(runs[0].style.is_plain());
    }

    #[test]
    fn styled_runs_multibyte_boundaries() {
        // "é" is two UTF-8 bytes; an entity over the whole char styles it,
        // one splitting the bytes is dropped.
        let text = "aé";
        let runs = styled_runs(
            text,
            &[
                entity(1, 3, TextEntityKind::Bold),
                entity(2, 3, TextEntityKind::Italic),
            ],
        );
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[1].text, "é");
        assert!(runs[1].style.bold);
        assert!(!runs[1].style.italic);
    }

    #[test]
    fn styled_runs_empty_text() {
        assert!(styled_runs("", &[entity(0, 0, TextEntityKind::Bold)]).is_empty());
    }
}
