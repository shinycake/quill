//! `.tdesktop-theme` and `.tdesktop-palette` files (tdesktop
//! `window_theme.cpp` `LoadTheme`, `ReadPaletteValues`).
//!
//! A `.tdesktop-theme` is a zip holding `colors.tdesktop-theme` (or
//! `colors.tdesktop-palette`) and optionally a background image
//! (`background.jpg`/`.png`, or `tiled.jpg`/`.png` for a tiled one). A
//! `.tdesktop-palette` is the palette text alone. The palette is a list
//! of `name: value;` pairs with `//` and `/* */` comments, where a value is
//! `#rrggbb`, `#rrggbbaa` or the name of another key.
//!
//! Every pair is kept, in order, so a theme Quill imports and exports again
//! keeps the keys Quill does not use.

use super::keys::PaletteKey;
use super::palette::{Palette, ThemeBackground, hex_string, parse_hex};
use std::io::Read;

/// tdesktop `kThemeFileSizeLimit`.
pub const THEME_FILE_LIMIT: usize = 5 * 1024 * 1024;
/// tdesktop `kThemeSchemeSizeLimit`.
pub const SCHEME_LIMIT: usize = 1024 * 1024;
/// tdesktop `kThemeBackgroundSizeLimit`.
pub const BACKGROUND_LIMIT: usize = 4 * 1024 * 1024;

/// Why a theme file was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThemeFileError {
    TooLarge,
    /// The zip has no palette, or could not be read.
    BadArchive,
    /// The palette text is malformed (tdesktop refuses the whole theme).
    BadPalette(String),
    /// No key Quill uses is set.
    NothingToApply,
}

impl ThemeFileError {
    /// The line shown to the person importing the file.
    pub fn note(&self) -> String {
        match self {
            Self::TooLarge => "That theme file is too large (the limit is 5 MB).".into(),
            Self::BadArchive => "That theme file could not be read.".into(),
            Self::BadPalette(why) => format!("That theme file has an error: {why}"),
            Self::NothingToApply => "That theme file sets no colors Quill uses.".into(),
        }
    }
}

/// A theme's background image as stored in the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeImage {
    pub bytes: Vec<u8>,
    /// Stored as `tiled.*`.
    pub tiled: bool,
    /// PNG (else JPEG).
    pub png: bool,
}

/// A parsed theme file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThemeFile {
    /// `(name, value)` pairs in file order; a value is `#hex` or a key name.
    pub entries: Vec<(String, String)>,
    pub background: Option<ThemeImage>,
}

impl ThemeFile {
    /// Read a `.tdesktop-theme` (zip) or `.tdesktop-palette` (text).
    pub fn parse(bytes: &[u8]) -> Result<Self, ThemeFileError> {
        if bytes.len() > THEME_FILE_LIMIT {
            return Err(ThemeFileError::TooLarge);
        }
        if bytes.starts_with(b"PK\x03\x04") {
            let entries = read_zip(bytes).ok_or(ThemeFileError::BadArchive)?;
            let find = |name: &str| {
                entries
                    .iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case(name))
                    .map(|(_, data)| data.clone())
            };
            let scheme = find("colors.tdesktop-theme")
                .or_else(|| find("colors.tdesktop-palette"))
                .ok_or(ThemeFileError::BadArchive)?;
            let mut file = Self {
                entries: parse_palette(&scheme)?,
                background: None,
            };
            for (name, tiled, png) in [
                ("background.jpg", false, false),
                ("background.png", false, true),
                ("tiled.jpg", true, false),
                ("tiled.png", true, true),
            ] {
                if let Some(bytes) = find(name) {
                    file.background = Some(ThemeImage { bytes, tiled, png });
                    break;
                }
            }
            Ok(file)
        } else {
            Ok(Self {
                entries: parse_palette(bytes)?,
                background: None,
            })
        }
    }

    /// The value written for `name` (the last one, as tdesktop applies
    /// them in order).
    pub fn raw(&self, name: &str) -> Option<&str> {
        self.entries
            .iter()
            .rev()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    /// `name`'s colour, following references to other keys in the file
    /// and then to `fallback` (tdesktop resolves a reference against the
    /// palette as loaded so far, which starts from the default).
    pub fn color(&self, name: &str, fallback: &Palette) -> Option<u32> {
        let mut current = name;
        for _ in 0..32 {
            match self.raw(current) {
                Some(value) if value.starts_with('#') => return parse_hex(value),
                Some(reference) => current = reference,
                None => return PaletteKey::from_name(current).map(|k| fallback.rgba(k)),
            }
        }
        None
    }

    /// Whether the file sets at least one key Quill uses.
    pub fn sets_known_keys(&self) -> bool {
        self.entries
            .iter()
            .any(|(n, _)| PaletteKey::from_name(n).is_some())
    }

    /// The palette this file gives, keys it leaves out taken from
    /// `fallback` (tdesktop keeps the default palette's value).
    /// `background` is the theme image's colour, when known.
    pub fn palette(&self, fallback: &Palette, background: Option<u32>) -> Palette {
        let mut palette = fallback.clone();
        for &key in PaletteKey::ALL {
            if let Some(rgba) = self.color(key.name(), fallback) {
                palette.set(key, rgba);
            }
        }
        palette.background = background
            .or_else(|| {
                self.background
                    .as_ref()
                    .and_then(|i| super::png::solid_color(&i.bytes))
            })
            .map(ThemeBackground::Solid);
        palette
    }

    /// Set `key` to `rgba`, replacing what the file wrote for it.
    pub fn set_color(&mut self, key: PaletteKey, rgba: u32) {
        let value = hex_string(rgba);
        self.entries.retain(|(n, _)| n != key.name());
        self.entries.push((key.name().to_string(), value));
    }

    /// A file holding every key of `palette`, with its background as a
    /// one-pixel image (a gradient becomes its average colour).
    pub fn from_palette(palette: &Palette) -> Self {
        Self {
            entries: PaletteKey::ALL
                .iter()
                .map(|&k| (k.name().to_string(), hex_string(palette.rgba(k))))
                .collect(),
            background: palette.background.as_ref().map(|b| ThemeImage {
                bytes: super::png::solid_png(b.average()),
                tiled: false,
                png: true,
            }),
        }
    }

    /// Replace the background with one colour.
    pub fn set_background(&mut self, rgb: u32) {
        self.background = Some(ThemeImage {
            bytes: super::png::solid_png(rgb),
            tiled: false,
            png: true,
        });
    }

    /// The palette text, one `name: value;` per line.
    pub fn palette_text(&self) -> String {
        let mut out = String::new();
        for (name, value) in &self.entries {
            out.push_str(name);
            out.push_str(": ");
            out.push_str(value);
            out.push_str(";\n");
        }
        out
    }

    /// The `.tdesktop-theme` zip (tdesktop `CollectForExport`).
    pub fn to_zip(&self) -> Vec<u8> {
        let mut parts = vec![(
            "colors.tdesktop-theme".to_string(),
            self.palette_text().into_bytes(),
        )];
        if let Some(image) = &self.background {
            let name = match (image.tiled, image.png) {
                (false, false) => "background.jpg",
                (false, true) => "background.png",
                (true, false) => "tiled.jpg",
                (true, true) => "tiled.png",
            };
            parts.push((name.to_string(), image.bytes.clone()));
        }
        crate::stat_chart::xlsx::zip(&parts)
    }
}

/// tdesktop `base::parse::stripComments`: `//` to the end of the line and
/// `/* … */` removed (a comment start inside a value cannot happen: values
/// are names and hex).
fn strip_comments(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        if text[i..].starts_with(b"//") {
            while i < text.len() && text[i] != b'\n' {
                i += 1;
            }
        } else if text[i..].starts_with(b"/*") {
            i += 2;
            while i < text.len() && !text[i..].starts_with(b"*/") {
                i += 1;
            }
            i = (i + 2).min(text.len());
            out.push(b' ');
        } else {
            out.push(text[i]);
            i += 1;
        }
    }
    out
}

fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// tdesktop `ReadPaletteValues` / `readNameAndValue`: strict, any syntax
/// error refuses the palette.
pub fn parse_palette(text: &[u8]) -> Result<Vec<(String, String)>, ThemeFileError> {
    if text.len() > SCHEME_LIMIT {
        return Err(ThemeFileError::TooLarge);
    }
    let data = strip_comments(text);
    let mut at = 0;
    let skip = |at: &mut usize| {
        while *at < data.len() && data[*at].is_ascii_whitespace() {
            *at += 1;
        }
        *at < data.len()
    };
    let read_name = |at: &mut usize| {
        let start = *at;
        while *at < data.len() && is_name_byte(data[*at]) {
            *at += 1;
        }
        String::from_utf8_lossy(&data[start..*at]).into_owned()
    };
    let mut out = Vec::new();
    loop {
        if !skip(&mut at) {
            return Ok(out);
        }
        let name = read_name(&mut at);
        if name.is_empty() {
            return Err(ThemeFileError::BadPalette(
                "expected a key name".to_string(),
            ));
        }
        if !skip(&mut at) || data[at] != b':' {
            return Err(ThemeFileError::BadPalette(format!(
                "expected ':' after {name}"
            )));
        }
        at += 1;
        if !skip(&mut at) {
            return Err(ThemeFileError::BadPalette(format!("no value for {name}")));
        }
        let hash = data[at] == b'#';
        if hash {
            at += 1;
        }
        let value = read_name(&mut at);
        if value.is_empty() {
            return Err(ThemeFileError::BadPalette(format!(
                "expected a color for {name}"
            )));
        }
        if !skip(&mut at) || data[at] != b';' {
            return Err(ThemeFileError::BadPalette(format!(
                "expected ';' after {name}"
            )));
        }
        at += 1;
        let value = if hash { format!("#{value}") } else { value };
        out.push((name, value));
    }
}

fn u16_at(data: &[u8], at: usize) -> Option<usize> {
    let bytes = data.get(at..at + 2)?;
    Some(usize::from(u16::from_le_bytes([bytes[0], bytes[1]])))
}

fn u32_at(data: &[u8], at: usize) -> Option<usize> {
    let bytes = data.get(at..at + 4)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize)
}

/// The files of a small zip (stored or deflated), from its central
/// directory. Entries over the size limits are skipped.
fn read_zip(zip: &[u8]) -> Option<Vec<(String, Vec<u8>)>> {
    // The end-of-central-directory record is the last 22 bytes plus an
    // optional comment of up to 64 KB.
    let min = zip.len().checked_sub(22)?;
    let floor = min.saturating_sub(u16::MAX as usize);
    let eocd = (floor..=min)
        .rev()
        .find(|&i| zip[i..].starts_with(&[0x50, 0x4b, 0x05, 0x06]))?;
    let count = u16_at(zip, eocd + 10)?;
    let mut at = u32_at(zip, eocd + 16)?;
    let mut out = Vec::new();
    for _ in 0..count {
        if u32_at(zip, at)? != 0x0201_4b50 {
            return None;
        }
        let method = u16_at(zip, at + 10)?;
        let compressed = u32_at(zip, at + 20)?;
        let size = u32_at(zip, at + 24)?;
        let name_len = u16_at(zip, at + 28)?;
        let extra_len = u16_at(zip, at + 30)?;
        let comment_len = u16_at(zip, at + 32)?;
        let offset = u32_at(zip, at + 42)?;
        let name = String::from_utf8_lossy(zip.get(at + 46..at + 46 + name_len)?).into_owned();
        at += 46 + name_len + extra_len + comment_len;
        if size > SCHEME_LIMIT.max(BACKGROUND_LIMIT) {
            continue;
        }
        if u32_at(zip, offset)? != 0x0403_4b50 {
            return None;
        }
        let local_name = u16_at(zip, offset + 26)?;
        let local_extra = u16_at(zip, offset + 28)?;
        let start = offset + 30 + local_name + local_extra;
        let raw = zip.get(start..start.checked_add(compressed)?)?;
        let data = match method {
            0 => raw.to_vec(),
            8 => {
                let mut inflated = Vec::with_capacity(size);
                flate2::read::DeflateDecoder::new(raw)
                    .take(size as u64 + 1)
                    .read_to_end(&mut inflated)
                    .ok()?;
                if inflated.len() != size {
                    return None;
                }
                inflated
            }
            _ => continue,
        };
        out.push((name, data));
    }
    Some(out)
}
