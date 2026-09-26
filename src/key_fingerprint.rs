//! Phase B2: secret-chat key fingerprint visualization.
//!
//! TDLib 1.8.67, `schema/td_api.tl:2812` (the `secretChat.key_hash`
//! comment):
//! > "Hash of the currently used key for comparison with the hash of the
//! > chat partner's key. This is a string of 36 little-endian bytes, which
//! > must be split into groups of 2 bits, each denoting a pixel of one of
//! > 4 colors FFFFFF, D5E6F3, 2D5775, and 2F99C9."
//!
//! `schema/td_api.tl:2813` adds:
//! > "The pixels must be used to make a 12x12 square image filled from
//! > left to right, top to bottom."
//!
//! This module is the pure, deterministic byte→pixel mapping: 36 bytes →
//! 144 two-bit pixel indices (byte `i` contributes pixels `4i..4i+4`, group
//! `j` = `(byte >> (2*j)) & 3`, i.e. the least-significant 2 bits first —
//! the little-endian split). Color index 0 → FFFFFF, 1 → D5E6F3,
//! 2 → 2D5775, 3 → 2F99C9.
//!
//! Security: this module never sees anything but the byte slice it is
//! handed; it holds no state and implements no `Debug`/`serde` on any key
//! record (there is nothing to derive on here at all). Raw `key_hash`
//! bytes stay in `Session.secret_chat_states` — only the pixel indices
//! cross into the UI for rendering.

/// `secretChat.key_hash` is exactly 36 bytes (`schema/td_api.tl:2812`).
pub const KEY_HASH_LEN: usize = 36;

/// The key image is a 12×12 square (`schema/td_api.tl:2813`).
pub const KEY_GRID_SIZE: usize = 12;

/// 12×12 = 144 pixels, 4 two-bit groups per byte × 36 bytes.
pub const KEY_PIXEL_COUNT: usize = KEY_GRID_SIZE * KEY_GRID_SIZE;

/// The four key-pixel colors as 0xRRGGBB, in schema-comment order
/// (`schema/td_api.tl:2812`): FFFFFF, D5E6F3, 2D5775, 2F99C9.
pub const KEY_PIXEL_COLORS: [u32; 4] = [0xFF_FF_FF, 0xD5_E6_F3, 0x2D_57_75, 0x2F_99_C9];

/// Map a `secretChat.key_hash` to 144 pixel indices (row-major, left to
/// right, top to bottom).
///
/// Returns `None` unless the slice is exactly [`KEY_HASH_LEN`] bytes — a
/// short/empty hash (e.g. a chat whose `getSecretChat` answer hasn't
/// arrived yet) is not renderable and the UI shows a still-loading note
/// instead.
pub fn key_hash_pixels(key_hash: &[u8]) -> Option<[u8; KEY_PIXEL_COUNT]> {
    if key_hash.len() != KEY_HASH_LEN {
        return None;
    }
    let mut pixels = [0u8; KEY_PIXEL_COUNT];
    for (i, byte) in key_hash.iter().enumerate() {
        // Little-endian 2-bit groups: group j = bits (2j+1, 2j), the
        // least-significant pair first.
        for j in 0..4 {
            pixels[i * 4 + j] = (byte >> (2 * j)) & 0b11;
        }
    }
    Some(pixels)
}

/// The 0xRRGGBB color for a pixel index from [`key_hash_pixels`].
pub fn key_pixel_color(pixel: u8) -> u32 {
    KEY_PIXEL_COLORS[(pixel & 0b11) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_hash_is_all_white() {
        let pixels = key_hash_pixels(&[0u8; 36]).expect("36 zero bytes");
        assert_eq!(pixels.len(), KEY_PIXEL_COUNT);
        assert!(pixels.iter().all(|&p| p == 0));
        assert_eq!(key_pixel_color(0), 0xFF_FF_FF);
    }

    #[test]
    fn little_endian_group_order() {
        // 0xE4 = 0b11_10_01_00: group 0 (least-significant) = 0,
        // then 1, 2, 3.
        let mut hash = [0u8; 36];
        hash[0] = 0xE4;
        let pixels = key_hash_pixels(&hash).expect("36 bytes");
        assert_eq!(&pixels[0..4], &[0, 1, 2, 3]);
        // Every other byte is zero, so the rest of the grid is white.
        assert!(pixels[4..].iter().all(|&p| p == 0));
    }

    #[test]
    fn wrong_lengths_are_not_renderable() {
        assert!(key_hash_pixels(&[]).is_none());
        assert!(key_hash_pixels(&[0u8; 35]).is_none());
        assert!(key_hash_pixels(&[0u8; 37]).is_none());
    }

    #[test]
    fn color_table_matches_schema_comment_order() {
        assert_eq!(
            KEY_PIXEL_COLORS,
            [0xFF_FF_FF, 0xD5_E6_F3, 0x2D_57_75, 0x2F_99_C9]
        );
        assert_eq!(key_pixel_color(1), 0xD5_E6_F3);
        assert_eq!(key_pixel_color(2), 0x2D_57_75);
        assert_eq!(key_pixel_color(3), 0x2F_99_C9);
    }

    #[test]
    fn full_36_byte_fixture_maps_deterministically() {
        // Same base64 fixture the B1 envelope test decodes: bytes
        // 0x00, 0x01, …, 0x23.
        let hash: Vec<u8> = (0u8..36).collect();
        let pixels = key_hash_pixels(&hash).expect("36 bytes");
        assert_eq!(pixels.len(), 144);
        // Byte 0x00 → four white pixels; byte 0x01 → [1,0,0,0].
        assert_eq!(&pixels[0..8], &[0, 0, 0, 0, 1, 0, 0, 0]);
        // Byte 0x23 = 0b00_10_00_11 → [3, 0, 2, 0] at the tail.
        assert_eq!(&pixels[140..144], &[3, 0, 2, 0]);
        // Row-major placement: pixel 13 (byte 3, group 1) sits at
        // row 1, col 1.
        assert_eq!(pixels[12], 3); // byte 0x03 group 0
        assert_eq!(pixels[13], 0); // byte 0x03 group 1
    }
}
