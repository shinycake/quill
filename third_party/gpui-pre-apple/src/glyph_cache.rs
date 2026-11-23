//! QUILL: glyph bitmaps kept from one launch to the next.
//!
//! The first frame of a launch rasterizes every glyph and emoji it shows
//! through Core Text on the main thread. The app hands this module the
//! bitmaps the last launch drew ([`load`]) before its first frame, and
//! `rasterize_glyph` takes a bitmap from here instead of drawing it again.
//! Everything rasterized or taken until [`save`] is written back for the
//! next launch. The glyphs' raster bounds are kept the same way, so the
//! first frame does not measure them again either.
//!
//! A glyph is named by its font's PostScript name and the render
//! parameters. The app passes a `stamp` (its version and the OS build), so
//! a new OS with new font files starts over; a bitmap whose size differs
//! from the glyph's raster bounds is never used.

use gpui::{Bounds, DevicePixels, RenderGlyphParams, Size, point, size};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

const MAGIC: &[u8; 8] = b"QGLYPHS2";
/// A cache file larger than this is not read.
const MAX_FILE: u64 = 16 << 20;
/// Recording stops at this many bytes of bitmaps, in case `save` never comes
/// (a signed-out launch never draws a chat list).
const MAX_USED: usize = 8 << 20;

#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct Key {
    font: String,
    glyph: u32,
    size: u32,
    scale: u32,
    subpixel: (u8, u8),
    flags: u8,
    dilation: u8,
}

impl Key {
    pub(crate) fn new(font: &str, params: &RenderGlyphParams) -> Self {
        Self {
            font: font.to_owned(),
            glyph: params.glyph_id.0,
            size: f32::from(params.font_size).to_bits(),
            scale: params.scale_factor.to_bits(),
            subpixel: (params.subpixel_variant.x, params.subpixel_variant.y),
            flags: u8::from(params.is_emoji) | (u8::from(params.subpixel_rendering) << 1),
            dilation: params.dilation,
        }
    }
}

struct Bitmap {
    width: i32,
    height: i32,
    bytes: Vec<u8>,
}

/// A glyph's raster bounds: origin x, origin y, width, height.
type RasterBounds = [i32; 4];

#[derive(Default)]
struct Cache {
    /// Bitmaps from the last launch not asked for yet.
    loaded: HashMap<Key, Bitmap>,
    /// Bitmaps this launch drew or took, until `save`.
    used: Vec<(Key, Bitmap)>,
    used_bytes: usize,
    /// Raster bounds from the last launch not asked for yet, and those this
    /// launch measured or took.
    loaded_bounds: HashMap<Key, RasterBounds>,
    used_bounds: Vec<(Key, RasterBounds)>,
    saved: bool,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

/// Reads the bitmaps `path` holds if it was written with `stamp`. Call it
/// before the first window draws; a later call does nothing.
pub fn load(path: &Path, stamp: &str) {
    let (loaded, loaded_bounds) = read(path, stamp).unwrap_or_default();
    let mut cache = CACHE.lock();
    if cache.is_none() {
        *cache = Some(Cache {
            loaded,
            loaded_bounds,
            ..Cache::default()
        });
    }
}

/// Whether the text system should look here and record what it draws:
/// between [`load`] and [`save`].
pub(crate) fn recording() -> bool {
    CACHE.lock().as_ref().is_some_and(|cache| !cache.saved)
}

/// Writes the bitmaps this launch used to `path`, then stops recording and
/// drops what was not used. Only the first call writes.
pub fn save(path: &Path, stamp: &str) -> std::io::Result<()> {
    let (used, used_bounds) = {
        let mut cache = CACHE.lock();
        let Some(cache) = cache.as_mut() else {
            return Ok(());
        };
        if cache.saved {
            return Ok(());
        }
        cache.saved = true;
        cache.loaded = HashMap::new();
        cache.loaded_bounds = HashMap::new();
        (
            std::mem::take(&mut cache.used),
            std::mem::take(&mut cache.used_bounds),
        )
    };
    let mut out = Vec::with_capacity(used.iter().map(|(_, b)| b.bytes.len() + 32).sum());
    out.extend_from_slice(MAGIC);
    put_bytes(&mut out, stamp.as_bytes());
    out.extend_from_slice(&(used.len() as u32).to_le_bytes());
    for (key, bitmap) in &used {
        put_key(&mut out, key);
        out.extend_from_slice(&bitmap.width.to_le_bytes());
        out.extend_from_slice(&bitmap.height.to_le_bytes());
        put_bytes(&mut out, &bitmap.bytes);
    }
    out.extend_from_slice(&(used_bounds.len() as u32).to_le_bytes());
    for (key, bounds) in &used_bounds {
        put_key(&mut out, key);
        for value in bounds {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let partial = path.with_extension("partial");
    let mut file = std::fs::File::create(&partial)?;
    file.write_all(&out)?;
    drop(file);
    std::fs::rename(&partial, path)
}

/// The last launch's bitmap for `key` when it has the glyph's size.
pub(crate) fn take(key: &Key, size: Size<DevicePixels>) -> Option<(Size<DevicePixels>, Vec<u8>)> {
    let mut cache = CACHE.lock();
    let cache = cache.as_mut()?;
    let bitmap = cache.loaded.remove(key)?;
    if bitmap.width != size.width.0 || bitmap.height != size.height.0 {
        return None;
    }
    let found = (size, bitmap.bytes.clone());
    cache.keep(key.clone(), bitmap);
    Some(found)
}

/// Keeps a bitmap this launch drew for the next launch.
pub(crate) fn record(key: Key, size: Size<DevicePixels>, bytes: &[u8]) {
    if let Some(cache) = CACHE.lock().as_mut() {
        cache.keep(
            key,
            Bitmap {
                width: size.width.0,
                height: size.height.0,
                bytes: bytes.to_vec(),
            },
        );
    }
}

/// The last launch's raster bounds for `key`.
pub(crate) fn take_bounds(key: &Key) -> Option<Bounds<DevicePixels>> {
    let mut cache = CACHE.lock();
    let cache = cache.as_mut()?;
    let found = cache.loaded_bounds.remove(key)?;
    cache.keep_bounds(key.clone(), found);
    let [x, y, width, height] = found;
    Some(Bounds {
        origin: point(DevicePixels(x), DevicePixels(y)),
        size: size(DevicePixels(width), DevicePixels(height)),
    })
}

/// Keeps raster bounds this launch measured for the next launch.
pub(crate) fn record_bounds(key: Key, bounds: Bounds<DevicePixels>) {
    if let Some(cache) = CACHE.lock().as_mut() {
        let found = [
            bounds.origin.x.0,
            bounds.origin.y.0,
            bounds.size.width.0,
            bounds.size.height.0,
        ];
        cache.keep_bounds(key, found);
    }
}

impl Cache {
    fn keep(&mut self, key: Key, bitmap: Bitmap) {
        if self.saved || self.used_bytes + bitmap.bytes.len() > MAX_USED {
            return;
        }
        self.used_bytes += bitmap.bytes.len();
        self.used.push((key, bitmap));
    }

    fn keep_bounds(&mut self, key: Key, bounds: RasterBounds) {
        // A key and its bounds take about 64 bytes on disk.
        if self.saved || self.used_bytes + 64 > MAX_USED {
            return;
        }
        self.used_bytes += 64;
        self.used_bounds.push((key, bounds));
    }
}

pub(crate) fn put_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(bytes);
}

fn put_key(out: &mut Vec<u8>, key: &Key) {
    put_bytes(out, key.font.as_bytes());
    for word in [key.glyph, key.size, key.scale] {
        out.extend_from_slice(&word.to_le_bytes());
    }
    out.extend_from_slice(&[key.subpixel.0, key.subpixel.1, key.flags, key.dilation]);
}

fn read_key(reader: &mut Reader) -> Option<Key> {
    let font = String::from_utf8(reader.bytes()?.to_vec()).ok()?;
    let (glyph, size, scale) = (reader.u32()?, reader.u32()?, reader.u32()?);
    let tail = reader.take(4)?;
    let (subpixel, flags, dilation) = match *tail {
        [x, y, flags, dilation] => ((x, y), flags, dilation),
        _ => return None,
    };
    Some(Key {
        font,
        glyph,
        size,
        scale,
        subpixel,
        flags,
        dilation,
    })
}

type Loaded = (HashMap<Key, Bitmap>, HashMap<Key, RasterBounds>);

fn read(path: &Path, stamp: &str) -> Option<Loaded> {
    if std::fs::metadata(path).ok()?.len() > MAX_FILE {
        return None;
    }
    let data = std::fs::read(path).ok()?;
    let mut reader = Reader(&data);
    if reader.take(MAGIC.len())? != MAGIC || reader.bytes()? != stamp.as_bytes() {
        return None;
    }
    let count = reader.u32()?;
    let mut map = HashMap::new();
    for _ in 0..count {
        let key = read_key(&mut reader)?;
        let width = i32::from_le_bytes(reader.word()?);
        let height = i32::from_le_bytes(reader.word()?);
        let bytes = reader.bytes()?.to_vec();
        let pixel = if key.flags & 1 == 1 { 4 } else { 1 };
        let expected = usize::try_from(width).ok()? * usize::try_from(height).ok()? * pixel;
        if bytes.len() != expected {
            return None;
        }
        map.insert(
            key,
            Bitmap {
                width,
                height,
                bytes,
            },
        );
    }
    let mut bounds = HashMap::new();
    for _ in 0..reader.u32()? {
        let key = read_key(&mut reader)?;
        let mut found = [0i32; 4];
        for value in &mut found {
            *value = i32::from_le_bytes(reader.word()?);
        }
        let [_, _, width, height] = found;
        if width < 0 || height < 0 {
            return None;
        }
        bounds.insert(key, found);
    }
    Some((map, bounds))
}

pub(crate) struct Reader<'a>(pub(crate) &'a [u8]);

impl<'a> Reader<'a> {
    pub(crate) fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        if len > self.0.len() {
            return None;
        }
        let (head, rest) = self.0.split_at(len);
        self.0 = rest;
        Some(head)
    }

    fn word(&mut self) -> Option<[u8; 4]> {
        self.take(4)?.try_into().ok()
    }

    pub(crate) fn u32(&mut self) -> Option<u32> {
        self.word().map(u32::from_le_bytes)
    }

    pub(crate) fn bytes(&mut self) -> Option<&'a [u8]> {
        let len = usize::try_from(self.u32()?).ok()?;
        self.take(len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{FontId, GlyphId};

    fn params(glyph: u32) -> RenderGlyphParams {
        RenderGlyphParams {
            font_id: FontId(0),
            glyph_id: GlyphId(glyph),
            font_size: gpui::px(13.0),
            subpixel_variant: point(1, 0),
            scale_factor: 2.0,
            is_emoji: false,
            subpixel_rendering: false,
            dilation: 0,
        }
    }

    #[test]
    fn round_trips_through_a_file_with_the_same_stamp() {
        let dir = std::env::temp_dir().join(format!("quill-glyphs-{}", std::process::id()));
        let path = dir.join("glyphs.bin");
        let key = Key::new("SFNS", &params(7));
        let size = Size {
            width: DevicePixels(2),
            height: DevicePixels(3),
        };
        let mut cache = Cache::default();
        cache.used.push((
            key.clone(),
            Bitmap {
                width: 2,
                height: 3,
                bytes: vec![9; 6],
            },
        ));
        let bounds = [-1, 2, 2, 3];
        cache.used_bounds.push((key.clone(), bounds));
        *CACHE.lock() = Some(cache);
        save(&path, "a").expect("written");

        assert!(read(&path, "b").is_none(), "another stamp is ignored");
        let (map, loaded_bounds) = read(&path, "a").expect("read back");
        assert_eq!(loaded_bounds.get(&key), Some(&bounds));
        *CACHE.lock() = Some(Cache {
            loaded: map,
            loaded_bounds,
            ..Cache::default()
        });
        assert_eq!(
            take_bounds(&key),
            Some(Bounds {
                origin: point(DevicePixels(-1), DevicePixels(2)),
                size,
            })
        );
        assert!(take_bounds(&key).is_none(), "taken once");
        let wrong = Size {
            width: DevicePixels(3),
            height: DevicePixels(3),
        };
        assert!(take(&key, wrong).is_none(), "a different size is not used");
        *CACHE.lock() = Some(Cache {
            loaded: read(&path, "a").expect("read back").0,
            ..Cache::default()
        });
        assert_eq!(take(&key, size).map(|(_, bytes)| bytes), Some(vec![9; 6]));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_cut_file_is_ignored() {
        let dir = std::env::temp_dir().join(format!("quill-glyphs-cut-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("glyphs.bin");
        let mut data = MAGIC.to_vec();
        put_bytes(&mut data, b"a");
        data.extend_from_slice(&5u32.to_le_bytes());
        std::fs::write(&path, data).expect("written");
        assert!(read(&path, "a").is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
}
