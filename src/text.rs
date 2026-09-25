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

/// Link-bearing text entities Quill paints in private chats.
///
/// TDLib `textEntity` offsets are UTF-16. Callers convert them to UTF-8 byte
/// indices before storing a span. Bold/italic and the other entity types stay
/// unstyled in this slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextEntityKind {
    /// `textEntityTypeUrl` — the substring is the HTTP URL.
    Url,
    /// `textEntityTypeTextUrl` — visible label, `url` is opened on click.
    TextUrl { url: String },
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
        };
        openable_http_url(raw).then_some(raw.trim())
    }
}

/// One painted slice of message text. `href` is set for clickable links.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextRun {
    pub text: String,
    pub href: Option<String>,
}

/// Split `text` on non-overlapping link entities, in source order.
///
/// Nested or overlapping spans are skipped once a earlier link covers them
/// (schema: Url and TextUrl must not contain each other).
pub fn link_runs(text: &str, entities: &[TextEntity]) -> Vec<TextRun> {
    let mut links: Vec<&TextEntity> = entities
        .iter()
        .filter(|entity| {
            matches!(
                entity.kind,
                TextEntityKind::Url | TextEntityKind::TextUrl { .. }
            )
        })
        .collect();
    links.sort_by_key(|entity| (entity.utf8_start, entity.utf8_end));
    let mut runs = Vec::new();
    let mut cursor = 0usize;
    for entity in links {
        if entity.utf8_start < cursor
            || entity.utf8_end > text.len()
            || entity.utf8_start >= entity.utf8_end
            || !text.is_char_boundary(entity.utf8_start)
            || !text.is_char_boundary(entity.utf8_end)
        {
            continue;
        }
        if entity.utf8_start > cursor {
            runs.push(TextRun {
                text: text[cursor..entity.utf8_start].to_string(),
                href: None,
            });
        }
        let slice = text[entity.utf8_start..entity.utf8_end].to_string();
        let href = entity.open_href(text).map(str::to_string);
        runs.push(TextRun { text: slice, href });
        cursor = entity.utf8_end;
    }
    if cursor < text.len() {
        runs.push(TextRun {
            text: text[cursor..].to_string(),
            href: None,
        });
    }
    if runs.is_empty() && !text.is_empty() {
        runs.push(TextRun {
            text: text.to_string(),
            href: None,
        });
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
            TextEntity {
                utf8_start: start,
                utf8_end: end,
                kind: TextEntityKind::Url,
            },
            TextEntity {
                utf8_start: text.find("now").unwrap(),
                utf8_end: text.len(),
                kind: TextEntityKind::TextUrl {
                    url: "https://example.com/notes".into(),
                },
            },
        ];
        let runs = link_runs(text, &entities);
        assert_eq!(runs[0].text, "see ");
        assert!(runs[0].href.is_none());
        assert_eq!(runs[1].href.as_deref(), Some("https://example.com"));
        assert_eq!(runs[2].text, " ");
        assert_eq!(runs[3].text, "now");
        assert_eq!(runs[3].href.as_deref(), Some("https://example.com/notes"));
    }

    #[test]
    fn emoji_prefix_does_not_shift_link_bytes() {
        let text = "👋 https://example.com";
        let start = text.find("https").unwrap();
        let entity = TextEntity {
            utf8_start: start,
            utf8_end: text.len(),
            kind: TextEntityKind::Url,
        };
        let runs = link_runs(text, &[entity]);
        assert_eq!(runs[0].text, "👋 ");
        assert_eq!(runs[1].href.as_deref(), Some("https://example.com"));
    }

    #[test]
    fn rejects_non_http_and_overlapping_links() {
        let text = "javascript:alert(1) hi";
        let bad = TextEntity {
            utf8_start: 0,
            utf8_end: "javascript:alert(1)".len(),
            kind: TextEntityKind::Url,
        };
        let runs = link_runs(text, &[bad]);
        assert!(runs.iter().all(|run| run.href.is_none()));
        assert!(!openable_http_url("javascript:alert(1)"));
        assert!(!openable_http_url("https://evil.com/a b"));
        assert!(openable_http_url("https://example.com/a"));

        let overlap = [
            TextEntity {
                utf8_start: 0,
                utf8_end: 4,
                kind: TextEntityKind::TextUrl {
                    url: "https://a.example".into(),
                },
            },
            TextEntity {
                utf8_start: 2,
                utf8_end: 6,
                kind: TextEntityKind::TextUrl {
                    url: "https://b.example".into(),
                },
            },
        ];
        let runs = link_runs("abcdef", &overlap);
        assert_eq!(runs[0].href.as_deref(), Some("https://a.example"));
        assert!(runs.iter().skip(1).all(|run| run.href.is_none()));
    }
}
