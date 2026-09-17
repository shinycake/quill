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
}
