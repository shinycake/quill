//! Chat backgrounds ("wallpapers"): `background`, `backgrounds`,
//! `updateDefaultBackground` (`schema/td_api.tl:739`, `:742`, `:11317`) and
//! the fills and types they carry (`:8771`-`:8801`).

use super::message_media::{ParsedFile, parse_file};
use serde_json::Value;

/// `BackgroundFill`. Colors are 0xRRGGBB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackgroundFill {
    Solid(u32),
    /// Two colors blended along `angle` degrees (0-359).
    Gradient {
        top: u32,
        bottom: u32,
        angle: i32,
    },
    /// Up to four colors blended freely.
    Freeform(Vec<u32>),
}

/// `BackgroundType`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackgroundType {
    /// A photo (`is_blurred`: Telegram's "Blur" option).
    Wallpaper {
        blurred: bool,
        moving: bool,
    },
    /// A tiled pattern drawn over a fill; `intensity` is 0-100 (negative
    /// for dark patterns on a light fill).
    Pattern {
        fill: BackgroundFill,
        intensity: i32,
        inverted: bool,
        moving: bool,
    },
    Fill(BackgroundFill),
    /// A chat theme's background (not installable on its own).
    ChatTheme {
        name: String,
    },
}

/// `background`: one installed or default wallpaper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Background {
    pub id: i64,
    pub is_default: bool,
    pub is_dark: bool,
    pub name: String,
    /// The image or pattern file; `None` for plain fills.
    pub file: Option<ParsedFile>,
    pub kind: BackgroundType,
}

impl BackgroundFill {
    /// The fill's colors, first to last.
    pub fn colors(&self) -> Vec<u32> {
        match self {
            BackgroundFill::Solid(c) => vec![*c],
            BackgroundFill::Gradient { top, bottom, .. } => vec![*top, *bottom],
            BackgroundFill::Freeform(colors) => colors.clone(),
        }
    }

    /// A single color standing for the whole fill (the mean), for places
    /// that cannot paint a gradient.
    pub fn average(&self) -> u32 {
        let colors = self.colors();
        if colors.is_empty() {
            return 0;
        }
        let n = colors.len() as u32;
        let channel = |shift: u32| colors.iter().map(|c| (c >> shift) & 0xff).sum::<u32>() / n;
        (channel(16) << 16) | (channel(8) << 8) | channel(0)
    }
}

impl Background {
    /// The fill behind (or instead of) the image.
    pub fn fill(&self) -> Option<&BackgroundFill> {
        match &self.kind {
            BackgroundType::Fill(fill) | BackgroundType::Pattern { fill, .. } => Some(fill),
            _ => None,
        }
    }

    /// Whether painting needs the downloaded photo.
    pub fn needs_image(&self) -> bool {
        matches!(self.kind, BackgroundType::Wallpaper { .. })
    }

    /// Whether painting needs the downloaded file: the photo, or a
    /// pattern's PNG / TGV.
    pub fn needs_file(&self) -> bool {
        matches!(
            self.kind,
            BackgroundType::Wallpaper { .. } | BackgroundType::Pattern { .. }
        )
    }
}

fn color(value: Option<&Value>) -> u32 {
    value.and_then(Value::as_i64).unwrap_or(0) as u32 & 0x00ff_ffff
}

pub(crate) fn parse_background_fill(value: Option<&Value>) -> Option<BackgroundFill> {
    let value = value?;
    match value.get("@type")?.as_str()? {
        "backgroundFillSolid" => Some(BackgroundFill::Solid(color(value.get("color")))),
        "backgroundFillGradient" => Some(BackgroundFill::Gradient {
            top: color(value.get("top_color")),
            bottom: color(value.get("bottom_color")),
            angle: value
                .get("rotation_angle")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
        }),
        "backgroundFillFreeformGradient" => Some(BackgroundFill::Freeform(
            value
                .get("colors")
                .and_then(Value::as_array)
                .map(|arr| arr.iter().map(|c| color(Some(c))).collect())
                .unwrap_or_default(),
        )),
        _ => None,
    }
}

pub(crate) fn parse_background_type(value: Option<&Value>) -> Option<BackgroundType> {
    let value = value?;
    let flag = |key: &str| value.get(key).and_then(Value::as_bool).unwrap_or(false);
    match value.get("@type")?.as_str()? {
        "backgroundTypeWallpaper" => Some(BackgroundType::Wallpaper {
            blurred: flag("is_blurred"),
            moving: flag("is_moving"),
        }),
        "backgroundTypePattern" => Some(BackgroundType::Pattern {
            fill: parse_background_fill(value.get("fill"))?,
            intensity: value.get("intensity").and_then(Value::as_i64).unwrap_or(0) as i32,
            inverted: flag("is_inverted"),
            moving: flag("is_moving"),
        }),
        "backgroundTypeFill" => Some(BackgroundType::Fill(parse_background_fill(
            value.get("fill"),
        )?)),
        "backgroundTypeChatTheme" => Some(BackgroundType::ChatTheme {
            name: value
                .get("theme_name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        }),
        _ => None,
    }
}

pub(crate) fn parse_background(value: &Value) -> Option<Background> {
    if value.get("@type").and_then(Value::as_str) != Some("background") {
        return None;
    }
    Some(Background {
        id: value.get("id").and_then(Value::as_i64)?,
        is_default: value
            .get("is_default")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_dark: value
            .get("is_dark")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        name: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        file: parse_file(value.get("document").and_then(|d| d.get("document"))).ok(),
        kind: parse_background_type(value.get("type"))?,
    })
}

pub(crate) fn parse_backgrounds(value: &Value) -> Option<Vec<Background>> {
    if value.get("@type").and_then(Value::as_str) != Some("backgrounds") {
        return None;
    }
    Some(
        value
            .get("backgrounds")
            .and_then(Value::as_array)
            .map(|arr| arr.iter().filter_map(parse_background).collect())
            .unwrap_or_default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_each_background_type() {
        let solid = parse_background(&json!({
            "@type":"background","id":1,"is_default":true,"is_dark":false,"name":"s",
            "document":{"@type":"document","document":{"@type":"file","id":0}},
            "type":{"@type":"backgroundTypeFill","fill":{"@type":"backgroundFillSolid","color":16711680}}
        }))
        .unwrap();
        assert_eq!(
            solid.kind,
            BackgroundType::Fill(BackgroundFill::Solid(0xff0000))
        );
        assert!(solid.is_default && !solid.needs_image());

        let gradient = parse_background(&json!({
            "@type":"background","id":2,"is_default":false,"is_dark":true,"name":"g",
            "type":{"@type":"backgroundTypeFill","fill":{"@type":"backgroundFillGradient","top_color":255,"bottom_color":65280,"rotation_angle":45}}
        }))
        .unwrap();
        assert_eq!(
            gradient.fill(),
            Some(&BackgroundFill::Gradient {
                top: 0x0000ff,
                bottom: 0x00ff00,
                angle: 45
            })
        );
        assert!(gradient.file.is_none());

        let photo = parse_background(&json!({
            "@type":"background","id":3,"is_default":false,"is_dark":false,"name":"p",
            "document":{"@type":"document","document":{"@type":"file","id":7,"size":10,"local":{"path":"/x/p.jpg","is_downloading_completed":true}}},
            "type":{"@type":"backgroundTypeWallpaper","is_blurred":true,"is_moving":false}
        }))
        .unwrap();
        assert!(photo.needs_image());
        assert_eq!(photo.file.as_ref().unwrap().usable_path(), Some("/x/p.jpg"));
        assert_eq!(
            photo.kind,
            BackgroundType::Wallpaper {
                blurred: true,
                moving: false
            }
        );

        let pattern = parse_background(&json!({
            "@type":"background","id":4,"is_default":false,"is_dark":false,"name":"pt",
            "type":{"@type":"backgroundTypePattern","fill":{"@type":"backgroundFillFreeformGradient","colors":[16711680,65280]},"intensity":40,"is_inverted":false,"is_moving":true}
        }))
        .unwrap();
        match pattern.kind {
            BackgroundType::Pattern {
                fill, intensity, ..
            } => {
                assert_eq!(fill, BackgroundFill::Freeform(vec![0xff0000, 0x00ff00]));
                assert_eq!(intensity, 40);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn fill_average_is_the_channel_mean() {
        assert_eq!(BackgroundFill::Solid(0x102030).average(), 0x102030);
        let g = BackgroundFill::Gradient {
            top: 0x000000,
            bottom: 0xfefefe,
            angle: 0,
        };
        assert_eq!(g.average(), 0x7f7f7f);
        assert_eq!(BackgroundFill::Freeform(vec![]).average(), 0);
    }

    #[test]
    fn lists_skip_unknown_entries() {
        let list = parse_backgrounds(&json!({
            "@type":"backgrounds","backgrounds":[
                {"@type":"background","id":1,"name":"a","type":{"@type":"backgroundTypeChatTheme","theme_name":"x"}},
                {"@type":"background","id":2,"name":"b","type":{"@type":"backgroundTypeNew"}}
            ]
        }))
        .unwrap();
        assert_eq!(list.len(), 1);
    }
}
