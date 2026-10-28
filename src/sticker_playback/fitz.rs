//! Emoji skin tones in Lottie files, ported from the parser of Telegram's
//! Lottie renderer (lottieparser.cpp: parseFitzColorReplacement,
//! applyReplacements, toColor and getValue(LottieColor&)).
//!
//! An emoji animation may carry a top-level `"fitz"` array of
//! `{"o": original, "f12": .., "f3": .., "f4": .., "f5": .., "f6": ..}`,
//! colors as `0xRRGGBB` integers. For a chosen tone, every fill (`fl`) and
//! stroke (`st`) color and every layer's solid color (`sc`) that equals an
//! `o` takes that tone's color instead. Gradients keep their stops, as there.
//! That parser applies the table while it streams the file, so only the
//! top-level entries after `"fitz"` are recoloured; so here.
#![deny(clippy::indexing_slicing)]

use serde_json::Value;

/// A skin tone for emoji animations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FitzType {
    #[default]
    None,
    /// Light (Fitzpatrick types 1 and 2 share one color set).
    Type12,
    Type3,
    Type4,
    Type5,
    Type6,
}

impl FitzType {
    /// From TDLib's `fitzpatrick_type` (0 none, 1-6 a tone; 1 and 2 both
    /// pick the light set, as Telegram X's createLottieDecoder maps them).
    pub fn from_tdlib(fitzpatrick_type: i32) -> Self {
        match fitzpatrick_type {
            1 | 2 => Self::Type12,
            3 => Self::Type3,
            4 => Self::Type4,
            5 => Self::Type5,
            6 => Self::Type6,
            _ => Self::None,
        }
    }

    fn key(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Type12 => Some("f12"),
            Self::Type3 => Some("f3"),
            Self::Type4 => Some("f4"),
            Self::Type5 => Some("f5"),
            Self::Type6 => Some("f6"),
        }
    }
}

/// Shape groups nest; deeper ones are left alone.
const MAX_GROUP_DEPTH: usize = 64;

/// `(original, replacement)` colors as `0xRRGGBB`; the first match wins.
type Replacements = Vec<(u32, u32)>;

/// Recolours `lottie` (a whole Lottie document) for `fitz`. Returns whether
/// any color changed.
pub fn apply_fitzpatrick(lottie: &mut Value, fitz: FitzType) -> bool {
    let Some(tone) = fitz.key() else {
        return false;
    };
    let Some(root) = lottie.as_object_mut() else {
        return false;
    };
    let mut replacements = Replacements::new();
    let mut changed = false;
    for (key, value) in root.iter_mut() {
        match key.as_str() {
            "fitz" => replacements.extend(table(value, tone)),
            "layers" => changed |= layers(value, &replacements),
            "assets" => {
                for asset in value.as_array_mut().into_iter().flatten() {
                    if let Some(precomp) = asset.get_mut("layers") {
                        changed |= layers(precomp, &replacements);
                    }
                }
            }
            _ => {}
        }
    }
    changed
}

fn table(fitz: &Value, tone: &str) -> Replacements {
    // Read with GetInt() into a uint32_t there; absent ones stay 0.
    let int = |entry: &Value, key: &str| {
        entry
            .get(key)
            .and_then(Value::as_i64)
            .map_or(0, |n| n as i32 as u32)
    };
    fitz.as_array()
        .into_iter()
        .flatten()
        .filter(|entry| entry.is_object())
        .map(|entry| (int(entry, "o"), int(entry, tone)))
        .collect()
}

fn layers(layers: &mut Value, replacements: &Replacements) -> bool {
    if replacements.is_empty() {
        return false;
    }
    let mut changed = false;
    for layer in layers.as_array_mut().into_iter().flatten() {
        if let Some(solid) = layer.get_mut("sc") {
            changed |= solid_color(solid, replacements);
        }
        if let Some(shapes) = layer.get_mut("shapes") {
            changed |= shape_list(shapes, replacements, 0);
        }
    }
    changed
}

fn shape_list(shapes: &mut Value, replacements: &Replacements, depth: usize) -> bool {
    if depth > MAX_GROUP_DEPTH {
        return false;
    }
    let mut changed = false;
    for shape in shapes.as_array_mut().into_iter().flatten() {
        match shape.get("ty").and_then(Value::as_str) {
            Some("gr") => {
                if let Some(items) = shape.get_mut("it") {
                    changed |= shape_list(items, replacements, depth + 1);
                }
            }
            Some("fl" | "st") => {
                if let Some(color) = shape.get_mut("c") {
                    changed |= animated_color(color, replacements);
                }
            }
            _ => {}
        }
    }
    changed
}

/// A color property: `{"k": [r, g, b, a]}` or keyframes
/// `{"k": [{"s": [..], "e": [..]}, ..]}`.
fn animated_color(property: &mut Value, replacements: &Replacements) -> bool {
    let Some(k) = property.get_mut("k").and_then(Value::as_array_mut) else {
        return false;
    };
    if k.first().is_some_and(Value::is_number) {
        return color_array(k, replacements);
    }
    let mut changed = false;
    for keyframe in k.iter_mut() {
        for end in ["s", "e"] {
            if let Some(value) = keyframe.get_mut(end).and_then(Value::as_array_mut) {
                changed |= color_array(value, replacements);
            }
        }
    }
    changed
}

/// The parser's `convert`: a channel in 0..=1 to 0..=255, read as a float.
fn channel(value: f32) -> u32 {
    (f64::from(value.clamp(0.0, 1.0)) * 255.0).round() as u32
}

/// The parser's `part`: one byte of `0xRRGGBB` as a float channel.
fn part(rgb: u32, shift: u32) -> f32 {
    ((rgb >> shift) & 0xFF) as f32 / 255.0
}

fn replacement(rgb: [f32; 3], replacements: &Replacements) -> Option<u32> {
    let [r, g, b] = rgb.map(channel);
    let converted = b | (g << 8) | (r << 16);
    replacements
        .iter()
        .find(|(original, _)| *original == converted)
        .map(|(_, to)| *to)
}

/// `[r, g, b, (a)]`: missing channels count as 0, as in the parser; alpha and
/// anything after it are kept.
fn color_array(color: &mut Vec<Value>, replacements: &Replacements) -> bool {
    let mut rgb = [0.0f32; 3];
    for (index, slot) in rgb.iter_mut().enumerate() {
        match color.get(index) {
            None => {}
            Some(value) => match value.as_f64() {
                Some(number) => *slot = number as f32,
                None => return false,
            },
        }
    }
    let Some(to) = replacement(rgb, replacements) else {
        return false;
    };
    let mut recoloured: Vec<Value> = [16, 8, 0]
        .map(|shift| Value::from(f64::from(part(to, shift))))
        .into();
    recoloured.extend(color.iter().skip(3).cloned());
    *color = recoloured;
    true
}

/// A solid layer's `"#rrggbb"`; anything else is left as it is.
fn solid_color(solid: &mut Value, replacements: &Replacements) -> bool {
    let Some(hex) = solid.as_str() else {
        return false;
    };
    if hex.len() != 7 || !hex.starts_with('#') {
        return false;
    }
    // strtol on each pair: leading hex digits, 0 when there are none.
    let pair = |at: usize| {
        let digits = hex.get(at..at + 2).unwrap_or("");
        let end = digits
            .find(|c: char| !c.is_ascii_hexdigit())
            .unwrap_or(digits.len());
        u8::from_str_radix(digits.get(..end).unwrap_or(""), 16).unwrap_or(0)
    };
    let rgb = [1, 3, 5].map(|at| f32::from(pair(at)) / 255.0);
    let Some(to) = replacement(rgb, replacements) else {
        return false;
    };
    *solid = Value::from(format!("#{:06x}", to & 0xFF_FFFF));
    true
}

#[cfg(test)]
mod tests;
