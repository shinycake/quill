//! `upgradedGiftColors` (TDLib 1.8.68, `schema/td_api.tl:1578`): the color
//! scheme a collectible gift lends to its wearer's name, replies and
//! profile (`setUpgradedGiftColors`).

use serde_json::Value;

/// One color scheme. Colors are `0xRRGGBB`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpgradedGiftColors {
    pub id: i64,
    /// The reply icon the scheme brings (`model_custom_emoji_id`).
    pub model_emoji_id: i64,
    /// The profile icon the scheme brings (`symbol_custom_emoji_id`).
    pub symbol_emoji_id: i64,
    pub light_accent: u32,
    pub light_colors: Vec<u32>,
    pub dark_accent: u32,
    pub dark_colors: Vec<u32>,
}

impl UpgradedGiftColors {
    /// The accent color for a theme.
    pub fn accent(&self, dark: bool) -> u32 {
        if dark {
            self.dark_accent
        } else {
            self.light_accent
        }
    }
}

fn int64(value: Option<&Value>) -> i64 {
    match value {
        Some(Value::String(s)) => s.parse().unwrap_or(0),
        Some(Value::Number(n)) => n.as_i64().unwrap_or(0),
        _ => 0,
    }
}

fn rgb(value: Option<&Value>) -> u32 {
    (int64(value) as u32) & 0x00FF_FFFF
}

fn rgb_list(value: Option<&Value>) -> Vec<u32> {
    value
        .and_then(Value::as_array)
        .map(|rows| rows.iter().map(|v| rgb(Some(v))).collect())
        .unwrap_or_default()
}

/// Parse an `upgradedGiftColors`; `None` when absent or without an id.
pub fn parse_upgraded_gift_colors(value: &Value) -> Option<UpgradedGiftColors> {
    let id = int64(value.get("id"));
    if id == 0 {
        return None;
    }
    Some(UpgradedGiftColors {
        id,
        model_emoji_id: int64(value.get("model_custom_emoji_id")),
        symbol_emoji_id: int64(value.get("symbol_custom_emoji_id")),
        light_accent: rgb(value.get("light_theme_accent_color")),
        light_colors: rgb_list(value.get("light_theme_colors")),
        dark_accent: rgb(value.get("dark_theme_accent_color")),
        dark_colors: rgb_list(value.get("dark_theme_colors")),
    })
}

#[cfg(test)]
mod tests {
    use super::parse_upgraded_gift_colors;
    use serde_json::json;

    #[test]
    fn parses_ids_sent_as_strings_and_masks_colors() {
        let colors = parse_upgraded_gift_colors(&json!({
            "@type": "upgradedGiftColors",
            "id": "5368324170671202286",
            "model_custom_emoji_id": "11",
            "symbol_custom_emoji_id": "12",
            "light_theme_accent_color": 0x112233,
            "light_theme_colors": [0x112233, 0x445566],
            "dark_theme_accent_color": 0x778899,
            "dark_theme_colors": [],
        }))
        .unwrap();
        assert_eq!(colors.id, 5_368_324_170_671_202_286);
        assert_eq!((colors.model_emoji_id, colors.symbol_emoji_id), (11, 12));
        assert_eq!(colors.accent(false), 0x112233);
        assert_eq!(colors.accent(true), 0x778899);
        assert_eq!(colors.light_colors, vec![0x112233, 0x445566]);
        assert!(colors.dark_colors.is_empty());
    }

    #[test]
    fn missing_or_zero_id_is_none() {
        assert!(parse_upgraded_gift_colors(&json!({"id": 0})).is_none());
        assert!(parse_upgraded_gift_colors(&json!({})).is_none());
    }
}
