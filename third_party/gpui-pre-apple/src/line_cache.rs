//! QUILL: line layouts kept from one launch to the next.
//!
//! The first frame of a launch shapes every line it shows through Core Text
//! on the main thread. Like [`crate::glyph_cache`], the app hands this module
//! the layouts the last launch made ([`load`]) before its first frame, and
//! `layout_line` takes a layout from here instead of shaping the line again.
//! Everything shaped or taken until [`save`] goes back to the app for the
//! next launch. The layouts hold the lines' text, so the app keeps them
//! where it keeps the text itself (Quill encrypts them with the account's
//! database key).
//!
//! A line is named by its text, its font size and the length and font of
//! each of its font runs. A run's font is named by its PostScript name, its
//! OpenType features and its fallbacks. A layout is used only when the text
//! system finds every font it draws with under its PostScript name.
//! Otherwise the line is shaped as usual. The app's `stamp` names the OS build, so new
//! system fonts start over.

use crate::glyph_cache::{Reader, put_bytes};
use gpui::{FontId, GlyphId, LineLayout, Pixels, ShapedGlyph, ShapedRun, point, px};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

const MAGIC: &[u8; 8] = b"QLINES01";
/// Recording stops at about this many bytes of layouts, in case `save` never
/// comes.
const MAX_USED: usize = 2 << 20;

/// The line, as `layout_line` is asked for it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Key {
    text: String,
    font_size: u32,
    /// Each font run's length in bytes and its font.
    runs: Vec<(u32, String)>,
}

impl Key {
    pub(crate) fn new(text: &str, font_size: Pixels, runs: Vec<(u32, String)>) -> Self {
        Self {
            text: text.to_owned(),
            font_size: f32::from(font_size).to_bits(),
            runs,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Line {
    width: f32,
    ascent: f32,
    descent: f32,
    runs: Vec<Run>,
}

#[derive(Clone, Debug, PartialEq)]
struct Run {
    /// The PostScript name of the font the run draws with.
    font: String,
    glyphs: Vec<Glyph>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Glyph {
    id: u32,
    x: f32,
    y: f32,
    index: u32,
}

impl Line {
    fn from_layout(
        layout: &LineLayout,
        font_name: impl Fn(FontId) -> Option<String>,
    ) -> Option<Self> {
        let mut runs = Vec::with_capacity(layout.runs.len());
        for run in &layout.runs {
            let mut glyphs = Vec::with_capacity(run.glyphs.len());
            for glyph in &run.glyphs {
                glyphs.push(Glyph {
                    id: glyph.id.0,
                    x: glyph.position.x.into(),
                    y: glyph.position.y.into(),
                    index: u32::try_from(glyph.index).ok()?,
                });
            }
            runs.push(Run {
                font: font_name(run.font_id)?,
                glyphs,
            });
        }
        Some(Self {
            width: layout.width.into(),
            ascent: layout.ascent.into(),
            descent: layout.descent.into(),
            runs,
        })
    }

    fn to_layout(&self, key: &Key, mut font: impl FontLookup) -> Option<LineLayout> {
        let mut runs = Vec::with_capacity(self.runs.len());
        for run in &self.runs {
            let (font_id, is_emoji) = font(&run.font)?;
            runs.push(ShapedRun {
                font_id,
                glyphs: run
                    .glyphs
                    .iter()
                    .map(|glyph| ShapedGlyph {
                        id: GlyphId(glyph.id),
                        position: point(px(glyph.x), px(glyph.y)),
                        index: glyph.index as usize,
                        is_emoji,
                    })
                    .collect(),
            });
        }
        Some(LineLayout {
            font_size: px(f32::from_bits(key.font_size)),
            width: px(self.width),
            ascent: px(self.ascent),
            descent: px(self.descent),
            runs,
            len: key.text.len(),
        })
    }

    fn weight(&self, key: &Key) -> usize {
        let glyphs: usize = self.runs.iter().map(|run| run.glyphs.len()).sum();
        key.text.len() + glyphs * 16 + (self.runs.len() + key.runs.len()) * 32
    }
}

/// Finds a font by its PostScript name, and says whether it draws emoji.
pub(crate) trait FontLookup: FnMut(&str) -> Option<(FontId, bool)> {}
impl<F: FnMut(&str) -> Option<(FontId, bool)>> FontLookup for F {}

#[derive(Default)]
struct Cache {
    /// Layouts from the last launch not asked for yet.
    loaded: HashMap<Key, Line>,
    /// Layouts this launch made or took, until `save`.
    used: Vec<(Key, Line)>,
    used_bytes: usize,
    has_loaded: bool,
}

impl Cache {
    fn take(&mut self, key: &Key, font: impl FontLookup) -> Option<LineLayout> {
        let layout = self.loaded.get(key)?.to_layout(key, font)?;
        if let Some(line) = self.loaded.remove(key) {
            self.keep(key.clone(), line);
        }
        Some(layout)
    }

    fn keep(&mut self, key: Key, line: Line) {
        let weight = line.weight(&key);
        if self.used_bytes + weight > MAX_USED {
            return;
        }
        self.used_bytes += weight;
        self.used.push((key, line));
    }
}

/// Set once by [`save`]: from then on nothing is looked up or recorded.
static SAVED: AtomicBool = AtomicBool::new(false);
static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

/// Whether `layout_line` should look here and record what it shapes: until
/// [`save`].
pub(crate) fn recording() -> bool {
    !SAVED.load(Ordering::Acquire)
}

/// Hands over the layouts the last launch saved, if `data` holds them and
/// was saved with `stamp`. Layouts are recorded from the start, so a `load`
/// after the first frame still saves that frame's lines. Only the first call
/// counts.
pub fn load(data: Option<&[u8]>, stamp: &str) {
    let loaded = data
        .and_then(|data| decode(data, stamp))
        .unwrap_or_default();
    let mut cache = CACHE.lock();
    let cache = cache.get_or_insert_with(Cache::default);
    if !cache.has_loaded {
        cache.has_loaded = true;
        cache.loaded = loaded;
    }
}

/// The layouts this launch shaped or took, to keep for the next launch.
/// Recording stops and what was not used is dropped. Only the first call
/// returns them.
pub fn save(stamp: &str) -> Option<Vec<u8>> {
    let used = {
        let mut cache = CACHE.lock();
        if SAVED.swap(true, Ordering::AcqRel) {
            return None;
        }
        cache.take().map(|cache| cache.used).unwrap_or_default()
    };
    Some(encode(&used, stamp))
}

/// The last launch's layout for `key`, when `font` finds every font it
/// draws with.
pub(crate) fn take(key: &Key, font: impl FontLookup) -> Option<LineLayout> {
    if !recording() {
        return None;
    }
    CACHE.lock().as_mut()?.take(key, font)
}

/// Keeps a layout this launch shaped for the next launch (`font_name` gives
/// a font's PostScript name).
pub(crate) fn record(key: Key, layout: &LineLayout, font_name: impl Fn(FontId) -> Option<String>) {
    if !recording() {
        return;
    }
    if let Some(line) = Line::from_layout(layout, font_name) {
        let mut cache = CACHE.lock();
        if !SAVED.load(Ordering::Acquire) {
            cache.get_or_insert_with(Cache::default).keep(key, line);
        }
    }
}

fn encode(lines: &[(Key, Line)], stamp: &str) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    put_bytes(&mut out, stamp.as_bytes());
    put_u32(&mut out, lines.len());
    for (key, line) in lines {
        put_bytes(&mut out, key.text.as_bytes());
        out.extend_from_slice(&key.font_size.to_le_bytes());
        put_u32(&mut out, key.runs.len());
        for (len, font) in &key.runs {
            out.extend_from_slice(&len.to_le_bytes());
            put_bytes(&mut out, font.as_bytes());
        }
        for value in [line.width, line.ascent, line.descent] {
            out.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        put_u32(&mut out, line.runs.len());
        for run in &line.runs {
            put_bytes(&mut out, run.font.as_bytes());
            put_u32(&mut out, run.glyphs.len());
            for glyph in &run.glyphs {
                for word in [glyph.id, glyph.x.to_bits(), glyph.y.to_bits(), glyph.index] {
                    out.extend_from_slice(&word.to_le_bytes());
                }
            }
        }
    }
    out
}

fn put_u32(out: &mut Vec<u8>, count: usize) {
    out.extend_from_slice(&u32::try_from(count).unwrap_or(u32::MAX).to_le_bytes());
}

/// The lines in `data`, or `None` when it was written with another stamp or
/// is cut short or damaged anywhere.
fn decode(data: &[u8], stamp: &str) -> Option<HashMap<Key, Line>> {
    let mut reader = Reader(data);
    if reader.take(MAGIC.len())? != MAGIC || reader.bytes()? != stamp.as_bytes() {
        return None;
    }
    let mut lines = HashMap::new();
    for _ in 0..reader.u32()? {
        let text = String::from_utf8(reader.bytes()?.to_vec()).ok()?;
        let font_size = reader.u32()?;
        let mut key_runs = Vec::new();
        for _ in 0..reader.u32()? {
            let len = reader.u32()?;
            key_runs.push((len, String::from_utf8(reader.bytes()?.to_vec()).ok()?));
        }
        let mut metrics = [0f32; 3];
        for value in &mut metrics {
            *value = finite(reader.u32()?)?;
        }
        let [width, ascent, descent] = metrics;
        let mut runs = Vec::new();
        for _ in 0..reader.u32()? {
            let font = String::from_utf8(reader.bytes()?.to_vec()).ok()?;
            let mut glyphs = Vec::new();
            for _ in 0..reader.u32()? {
                let glyph = Glyph {
                    id: reader.u32()?,
                    x: finite(reader.u32()?)?,
                    y: finite(reader.u32()?)?,
                    index: reader.u32()?,
                };
                // A glyph's index is a byte offset the line's users slice the
                // text at.
                if !text.is_char_boundary(usize::try_from(glyph.index).ok()?) {
                    return None;
                }
                glyphs.push(glyph);
            }
            runs.push(Run { font, glyphs });
        }
        let key = Key {
            text,
            font_size,
            runs: key_runs,
        };
        lines.insert(
            key,
            Line {
                width,
                ascent,
                descent,
                runs,
            },
        );
    }
    Some(lines)
}

/// `layout` written out and read back, as the next launch takes it.
#[cfg(test)]
pub(crate) fn through_a_file(
    key: &Key,
    layout: &LineLayout,
    font_name: impl Fn(FontId) -> Option<String>,
    font: impl FontLookup,
) -> Option<LineLayout> {
    let line = Line::from_layout(layout, font_name)?;
    let data = encode(&[(key.clone(), line)], "test");
    let mut cache = Cache {
        loaded: decode(&data, "test")?,
        ..Cache::default()
    };
    cache.take(key, font)
}

fn finite(bits: u32) -> Option<f32> {
    Some(f32::from_bits(bits)).filter(|value| value.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(text: &str) -> Key {
        Key::new(
            text,
            px(13.0),
            vec![(text.len() as u32, "SFNS|calt=0|".into())],
        )
    }

    fn line() -> Line {
        Line {
            width: 21.5,
            ascent: 12.0,
            descent: 3.5,
            runs: vec![
                Run {
                    font: ".SFNS-Regular".into(),
                    glyphs: vec![Glyph {
                        id: 7,
                        x: 0.0,
                        y: 0.0,
                        index: 0,
                    }],
                },
                Run {
                    font: "AppleColorEmoji".into(),
                    glyphs: vec![Glyph {
                        id: 1200,
                        x: 8.25,
                        y: 0.0,
                        index: 2,
                    }],
                },
            ],
        }
    }

    fn fonts(name: &str) -> Option<(FontId, bool)> {
        match name {
            ".SFNS-Regular" => Some((FontId(3), false)),
            "AppleColorEmoji" => Some((FontId(9), true)),
            _ => None,
        }
    }

    #[test]
    fn a_saved_line_comes_back_with_this_launchs_fonts() {
        let data = encode(&[(key("hi😀"), line())], "a");
        assert!(decode(&data, "b").is_none(), "another stamp is ignored");
        let mut cache = Cache {
            loaded: decode(&data, "a").expect("read back"),
            ..Cache::default()
        };
        let layout = cache.take(&key("hi😀"), fonts).expect("the saved line");
        assert_eq!(layout.len, "hi😀".len());
        assert_eq!(layout.font_size, px(13.0));
        assert_eq!(layout.width, px(21.5));
        let runs: Vec<_> = layout
            .runs
            .iter()
            .map(|run| {
                let glyph = &run.glyphs[0];
                (
                    run.font_id,
                    glyph.id,
                    glyph.position.x,
                    glyph.index,
                    glyph.is_emoji,
                )
            })
            .collect();
        assert_eq!(
            runs,
            vec![
                (FontId(3), GlyphId(7), px(0.0), 0, false),
                (FontId(9), GlyphId(1200), px(8.25), 2, true),
            ]
        );
        assert!(cache.take(&key("hi😀"), fonts).is_none(), "taken once");
        assert_eq!(cache.used.len(), 1, "kept for the next launch");
    }

    #[test]
    fn a_line_whose_font_is_not_loaded_is_shaped_again() {
        let mut cache = Cache::default();
        cache.loaded.insert(key("hi😀"), line());
        let no_emoji = |name: &str| fonts(name).filter(|(id, _)| *id != FontId(9));
        assert!(cache.take(&key("hi😀"), no_emoji).is_none());
        assert!(cache.take(&key("hi😀"), fonts).is_some(), "still there");
    }

    #[test]
    fn another_size_or_run_split_is_another_line() {
        let mut cache = Cache::default();
        cache.loaded.insert(key("hi😀"), line());
        let bigger = Key::new("hi😀", px(14.0), key("hi😀").runs);
        let split = Key::new(
            "hi😀",
            px(13.0),
            vec![(2, "SFNS|calt=0|".into()), (4, "SFNS|calt=0|".into())],
        );
        assert!(cache.take(&bigger, fonts).is_none());
        assert!(cache.take(&split, fonts).is_none());
    }

    #[test]
    fn a_damaged_file_is_ignored() {
        let data = encode(&[(key("hi😀"), line())], "a");
        assert!(decode(&data[..data.len() - 1], "a").is_none(), "cut short");
        let mut inside_a_char = line();
        inside_a_char.runs[1].glyphs[0].index = 3;
        let data = encode(&[(key("hi😀"), inside_a_char)], "a");
        assert!(decode(&data, "a").is_none(), "an index inside a character");
    }
}
