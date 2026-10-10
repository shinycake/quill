//! The colors a mini app gets: TDLib's `themeParameters` (int32 RGB, sent
//! with every open call) and the bridge's `theme_params` (`#rrggbb`
//! strings). Telegram Desktop fills both from its palette
//! (`Window::Theme::WebViewParams`); Quill fills them from the kit theme in
//! `ui/web_app_ui.rs`.

use quill_webview_protocol::ThemeColors;
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }

    /// TDLib's `int32` RGB (`0xRRGGBB`).
    pub fn int(self) -> i64 {
        (i64::from(self.0) << 16) | (i64::from(self.1) << 8) | i64::from(self.2)
    }
}

/// `themeParameters` (schema 1.8.68, line 1112), in its field order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeParams {
    pub background: Rgb,
    pub secondary_background: Rgb,
    pub header_background: Rgb,
    pub bottom_bar_background: Rgb,
    pub section_background: Rgb,
    pub section_separator: Rgb,
    pub text: Rgb,
    pub accent_text: Rgb,
    pub section_header_text: Rgb,
    pub subtitle_text: Rgb,
    pub destructive_text: Rgb,
    pub hint: Rgb,
    pub link: Rgb,
    pub button: Rgb,
    pub button_text: Rgb,
    pub dark: bool,
}

impl ThemeParams {
    /// The `theme` object of `webAppOpenParameters`.
    pub fn td_json(&self) -> Value {
        json!({
            "@type": "themeParameters",
            "background_color": self.background.int(),
            "secondary_background_color": self.secondary_background.int(),
            "header_background_color": self.header_background.int(),
            "bottom_bar_background_color": self.bottom_bar_background.int(),
            "section_background_color": self.section_background.int(),
            "section_separator_color": self.section_separator.int(),
            "text_color": self.text.int(),
            "accent_text_color": self.accent_text.int(),
            "section_header_text_color": self.section_header_text.int(),
            "subtitle_text_color": self.subtitle_text.int(),
            "destructive_text_color": self.destructive_text.int(),
            "hint_color": self.hint.int(),
            "link_color": self.link.int(),
            "button_color": self.button.int(),
            "button_text_color": self.button_text.int(),
        })
    }

    /// The colors for the helper's shell and the bridge's `themeParams`.
    pub fn bridge_colors(&self) -> ThemeColors {
        ThemeColors {
            bg_color: self.background.hex(),
            secondary_bg_color: self.secondary_background.hex(),
            header_bg_color: self.header_background.hex(),
            bottom_bar_bg_color: self.bottom_bar_background.hex(),
            section_bg_color: self.section_background.hex(),
            section_separator_color: self.section_separator.hex(),
            text_color: self.text.hex(),
            accent_text_color: self.accent_text.hex(),
            section_header_text_color: self.section_header_text.hex(),
            subtitle_text_color: self.subtitle_text.hex(),
            destructive_text_color: self.destructive_text.hex(),
            hint_color: self.hint.hex(),
            link_color: self.link.hex(),
            button_color: self.button.hex(),
            button_text_color: self.button_text.hex(),
            color_scheme: if self.dark { "dark" } else { "light" }.to_string(),
        }
    }

    /// The `theme_params` object of the bridge's `theme_changed`.
    pub fn bridge_json(&self) -> Value {
        let colors = self.bridge_colors();
        json!({
            "bg_color": colors.bg_color,
            "secondary_bg_color": colors.secondary_bg_color,
            "header_bg_color": colors.header_bg_color,
            "bottom_bar_bg_color": colors.bottom_bar_bg_color,
            "section_bg_color": colors.section_bg_color,
            "section_separator_color": colors.section_separator_color,
            "text_color": colors.text_color,
            "accent_text_color": colors.accent_text_color,
            "section_header_text_color": colors.section_header_text_color,
            "subtitle_text_color": colors.subtitle_text_color,
            "destructive_text_color": colors.destructive_text_color,
            "hint_color": colors.hint_color,
            "link_color": colors.link_color,
            "button_color": colors.button_color,
            "button_text_color": colors.button_text_color,
        })
    }

    /// The color a `color_key` names (`web_app_set_header_color` and
    /// friends).
    pub fn key_color(&self, key: super::bridge::ColorKey) -> Rgb {
        use super::bridge::ColorKey;
        match key {
            ColorKey::BgColor => self.background,
            ColorKey::SecondaryBgColor => self.secondary_background,
            ColorKey::BottomBarBgColor => self.bottom_bar_background,
        }
    }

    /// A neutral light theme, for tests and demos.
    pub fn light_default() -> Self {
        Self {
            background: Rgb(255, 255, 255),
            secondary_background: Rgb(241, 243, 245),
            header_background: Rgb(255, 255, 255),
            bottom_bar_background: Rgb(255, 255, 255),
            section_background: Rgb(255, 255, 255),
            section_separator: Rgb(225, 228, 232),
            text: Rgb(15, 20, 25),
            accent_text: Rgb(37, 99, 235),
            section_header_text: Rgb(107, 114, 128),
            subtitle_text: Rgb(107, 114, 128),
            destructive_text: Rgb(220, 38, 38),
            hint: Rgb(107, 114, 128),
            link: Rgb(37, 99, 235),
            button: Rgb(37, 99, 235),
            button_text: Rgb(255, 255, 255),
            dark: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_formats_both_ways() {
        assert_eq!(Rgb(0x31, 0xb5, 0x45).hex(), "#31b545");
        assert_eq!(Rgb(0x31, 0xb5, 0x45).int(), 0x31b545);
        assert_eq!(Rgb(255, 255, 255).int(), 0xffffff);
    }

    #[test]
    fn td_json_is_the_schema_object() {
        let theme = ThemeParams::light_default().td_json();
        assert_eq!(theme["@type"], "themeParameters");
        assert_eq!(theme["button_color"], 0x2563eb);
        assert_eq!(theme["secondary_background_color"], 0xf1f3f5);
        assert_eq!(theme.as_object().unwrap().len(), 16);
    }

    #[test]
    fn bridge_json_uses_the_web_app_keys() {
        let mut params = ThemeParams::light_default();
        params.dark = true;
        let colors = params.bridge_colors();
        assert_eq!(colors.color_scheme, "dark");
        assert_eq!(colors.bg_color, "#ffffff");
        let json = params.bridge_json();
        assert_eq!(json["link_color"], "#2563eb");
        assert_eq!(json["destructive_text_color"], "#dc2626");
        assert_eq!(json.as_object().unwrap().len(), 15);
    }
}
