//! Name (peer) accent colors from `updateAccentColors`.
//!
//! TDLib's `accentColor` (`td_api.tl`) maps a server palette id to a
//! built-in id (0..=6) and light/dark RGB lists; the first color of each
//! list is the one tdesktop paints names with. Ids 0..=6 are the built-in
//! colors and are not always listed, ids 7+ only exist in the update.
//!
//! The palette is the server's, so each account keeps its own table
//! (`set_palette`, by account) and the renderer reads the table of the
//! account on screen (`set_active_account`) by id, instead of threading the
//! session through every name-color call site. A background account's
//! update fills its own table and paints nothing on the account on screen.

use crate::locks::RwLockExt;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

/// Number of built-in accent ids (red, orange, violet, green, cyan, blue, pink).
pub const BUILT_IN_COUNT: i32 = 7;

const BUILT_IN_LIGHT: [u32; 7] = [
    0xcc5049, 0xd67722, 0x955cdb, 0x40a920, 0x309eba, 0x368ad1, 0xc7508b,
];
const BUILT_IN_DARK: [u32; 7] = [
    0xff8a80, 0xffab5e, 0xc29bff, 0x7ad46d, 0x5cd0e8, 0x6fb7ff, 0xff86c2,
];

/// One `accentColor` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameAccentColor {
    pub id: i32,
    /// `built_in_accent_color_id`, or -1 when the server sent none.
    pub built_in_id: i32,
    pub light_colors: Vec<u32>,
    pub dark_colors: Vec<u32>,
    /// `min_channel_chat_boost_level`: the boost level a channel needs to
    /// use this color (0 = none).
    pub min_channel_level: i32,
}

fn rgb_list(value: Option<&Value>) -> Vec<u32> {
    value
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_i64)
                .map(|n| (n as u32) & 0xFF_FFFF)
                .collect()
        })
        .unwrap_or_default()
}

/// A `min_*_chat_boost_level` field: 0 when missing or negative.
pub(crate) fn min_level(value: Option<&Value>) -> i32 {
    value
        .and_then(Value::as_i64)
        .and_then(|n| i32::try_from(n).ok())
        .unwrap_or(0)
        .max(0)
}

/// Parse one `accentColor`; `None` for a malformed entry.
pub fn parse_name_accent_color(value: &Value) -> Option<NameAccentColor> {
    let id = i32::try_from(value.get("id")?.as_i64()?).ok()?;
    let built_in_id = value
        .get("built_in_accent_color_id")
        .and_then(Value::as_i64)
        .and_then(|n| i32::try_from(n).ok())
        .unwrap_or(-1);
    Some(NameAccentColor {
        id,
        built_in_id,
        light_colors: rgb_list(value.get("light_theme_colors")),
        dark_colors: rgb_list(value.get("dark_theme_colors")),
        min_channel_level: min_level(value.get("min_channel_chat_boost_level")),
    })
}

type Palette = HashMap<i32, NameAccentColor>;

static PALETTES: LazyLock<RwLock<HashMap<String, Palette>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// The account whose palette the renderer reads (the one on screen).
static ACTIVE: LazyLock<RwLock<Option<String>>> = LazyLock::new(|| RwLock::new(None));

/// Replace `account`'s palette (the update is the full server table).
pub fn set_palette(account: &crate::ids::AccountKey, colors: &[NameAccentColor]) {
    if let Ok(mut palettes) = PALETTES.write() {
        palettes.insert(
            account.0.clone(),
            colors.iter().map(|c| (c.id, c.clone())).collect(),
        );
    }
}

/// `account` takes the screen: its palette is the one names are painted
/// with from the next frame.
pub fn set_active_account(account: &crate::ids::AccountKey) {
    *ACTIVE.write_or_recover() = Some(account.0.clone());
}

/// The name color for `id` as `0xRRGGBB`, from the server palette when it
/// lists the id, else the built-in (the entry's `built_in_id`, or `id`
/// folded onto the seven built-ins).
pub fn name_color_rgb(id: i32, dark: bool) -> u32 {
    let account = ACTIVE
        .read_or_recover()
        .clone()
        .unwrap_or_else(|| crate::ids::AccountKey::primary().0);
    name_color_rgb_of(&account, id, dark)
}

fn name_color_rgb_of(account: &str, id: i32, dark: bool) -> u32 {
    let entry = PALETTES
        .read_or_recover()
        .get(account)
        .and_then(|palette| palette.get(&id).cloned());
    resolve(id, entry.as_ref(), dark)
}

fn resolve(id: i32, entry: Option<&NameAccentColor>, dark: bool) -> u32 {
    let built_in = |n: i32| {
        let table = if dark {
            &BUILT_IN_DARK
        } else {
            &BUILT_IN_LIGHT
        };
        table
            .get(n.rem_euclid(BUILT_IN_COUNT) as usize)
            .copied()
            .unwrap_or_default()
    };
    if (0..BUILT_IN_COUNT).contains(&id) {
        // Built-ins keep Quill's tuned legibility values.
        return built_in(id);
    }
    let Some(entry) = entry else {
        return built_in(id);
    };
    let colors = if dark {
        &entry.dark_colors
    } else {
        &entry.light_colors
    };
    // A dark list may be empty: tdesktop then reuses the light colors.
    colors
        .first()
        .or_else(|| entry.light_colors.first())
        .copied()
        .unwrap_or_else(|| {
            if entry.built_in_id >= 0 {
                built_in(entry.built_in_id)
            } else {
                built_in(id)
            }
        })
}

#[cfg(test)]
mod tests {
    use super::{BUILT_IN_DARK, BUILT_IN_LIGHT, NameAccentColor, parse_name_accent_color, resolve};
    use serde_json::json;

    fn entry(id: i32, built_in_id: i32, light: &[u32], dark: &[u32]) -> NameAccentColor {
        NameAccentColor {
            id,
            built_in_id,
            light_colors: light.to_vec(),
            dark_colors: dark.to_vec(),
            min_channel_level: 0,
        }
    }

    #[test]
    fn parses_accent_color() {
        let v = json!({
            "@type": "accentColor", "id": 9, "built_in_accent_color_id": 4,
            "light_theme_colors": [0x112233, 0x445566], "dark_theme_colors": [0xAABBCC],
            "min_channel_chat_boost_level": 0
        });
        let c = parse_name_accent_color(&v).unwrap();
        assert_eq!((c.id, c.built_in_id), (9, 4));
        assert_eq!(c.light_colors, vec![0x112233, 0x445566]);
        assert_eq!(c.dark_colors, vec![0xAABBCC]);
        assert!(parse_name_accent_color(&json!({"id": "x"})).is_none());
    }

    #[test]
    fn built_ins_use_the_tuned_table() {
        assert_eq!(resolve(2, None, false), BUILT_IN_LIGHT[2]);
        assert_eq!(resolve(2, None, true), BUILT_IN_DARK[2]);
    }

    #[test]
    fn server_palette_ids_use_their_own_colors() {
        let e = entry(9, 4, &[0x112233], &[0xAABBCC]);
        assert_eq!(resolve(9, Some(&e), false), 0x112233);
        assert_eq!(resolve(9, Some(&e), true), 0xAABBCC);
    }

    #[test]
    fn missing_dark_list_reuses_light() {
        let e = entry(9, 4, &[0x112233], &[]);
        assert_eq!(resolve(9, Some(&e), true), 0x112233);
    }

    #[test]
    fn colorless_entry_falls_back_to_its_built_in_then_folds() {
        let e = entry(9, 4, &[], &[]);
        assert_eq!(resolve(9, Some(&e), false), BUILT_IN_LIGHT[4]);
        let e = entry(9, -1, &[], &[]);
        assert_eq!(resolve(9, Some(&e), false), BUILT_IN_LIGHT[2]);
        // Unknown id: folds onto the built-ins.
        assert_eq!(resolve(15, None, true), BUILT_IN_DARK[1]);
    }

    /// 06-064: two accounts keep their own tables; names are painted with
    /// the one asked for.
    #[test]
    fn each_account_paints_with_its_own_palette() {
        use crate::ids::AccountKey;
        let a = AccountKey::new("palette-a").unwrap();
        let b = AccountKey::new("palette-b").unwrap();
        super::set_palette(&a, &[entry(9001, -1, &[0x111111], &[0x222222])]);
        super::set_palette(&b, &[entry(9001, -1, &[0x333333], &[0x444444])]);
        assert_eq!(super::name_color_rgb_of(&a.0, 9001, false), 0x111111);
        assert_eq!(super::name_color_rgb_of(&b.0, 9001, false), 0x333333);
        assert_eq!(super::name_color_rgb_of(&b.0, 9001, true), 0x444444);
        // An account with no table falls back to the built-ins.
        let unknown = super::name_color_rgb_of("palette-none", 9001, false);
        assert_ne!(unknown, 0x111111);
        assert_ne!(unknown, 0x333333);
    }
}
