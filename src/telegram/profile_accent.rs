//! Profile accent colors (parity:auth-profile-accent).
//!
//! One responsibility: the `profileAccentColor` palette types from
//! `updateProfileAccentColors` (TDLib 1.8.67, `schema/td_api.tl:10963`).
//! Parsing lives here so the waived `envelope.rs` only grows a match arm
//! and an enum variant (type-coherent extensions).

use serde_json::Value;

/// `profileAccentColor` (schema 1.8.67, `schema/td_api.tl:2260`): one
/// settable accent color. Colors are `0xRRGGBB` RGB ints; the first
/// `palette_colors` entry is the primary swatch color.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileAccentColor {
    pub id: i32,
    pub light_colors: Vec<u32>,
    pub dark_colors: Vec<u32>,
}

impl ProfileAccentColor {
    /// Primary swatch color for the picker (light-theme first palette
    /// color; Telegram blue as the never-panic fallback).
    pub fn swatch_rgb(&self) -> u32 {
        self.light_colors.first().copied().unwrap_or(0x229ED9)
    }
}

fn rgb_list(value: Option<&Value>) -> Vec<u32> {
    value
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_i64())
                .map(|n| (n as u32) & 0xFFFFFF)
                .collect()
        })
        .unwrap_or_default()
}

/// Parse one `profileAccentColor` object. `None` on a malformed entry —
/// a hostile palette can never crash the parse; the entry is skipped.
pub fn parse_profile_accent_color(value: &Value) -> Option<ProfileAccentColor> {
    let id = i32::try_from(value.get("id")?.as_i64()?).ok()?;
    let light = value.get("light_theme_colors");
    let dark = value.get("dark_theme_colors");
    Some(ProfileAccentColor {
        id,
        light_colors: rgb_list(light.and_then(|c| c.get("palette_colors"))),
        dark_colors: rgb_list(dark.and_then(|c| c.get("palette_colors"))),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_palette_entry() {
        let v = json!({
            "id": 3,
            "light_theme_colors": {
                "palette_colors": [0xFF5A5A, 0xFF8A8A],
                "background_colors": [],
                "story_colors": []
            },
            "dark_theme_colors": {
                "palette_colors": [0xAA3333],
                "background_colors": [],
                "story_colors": []
            },
            "min_supergroup_chat_boost_level": 0,
            "min_channel_chat_boost_level": 0,
        });
        let c = parse_profile_accent_color(&v).unwrap();
        assert_eq!(c.id, 3);
        assert_eq!(c.light_colors, vec![0xFF5A5A, 0xFF8A8A]);
        assert_eq!(c.swatch_rgb(), 0xFF5A5A);
    }

    #[test]
    fn malformed_entry_is_none() {
        assert!(parse_profile_accent_color(&json!({"id": "x"})).is_none());
        assert!(parse_profile_accent_color(&json!({})).is_none());
    }

    #[test]
    fn empty_palette_falls_back_to_telegram_blue() {
        let v = json!({"id": 7});
        let c = parse_profile_accent_color(&v).unwrap();
        assert_eq!(c.swatch_rgb(), 0x229ED9);
    }
}
