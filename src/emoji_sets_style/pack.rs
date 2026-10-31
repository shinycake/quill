//! A downloaded sprite pack: `config.json` (the emoji → cell index) and
//! `emoji_<n>.webp` sheets, unpacked from the release zip (tdesktop
//! `UnpackBlob` + `lib_ui/ui/emoji_config.cpp` `LoadSprites`).
#![deny(clippy::indexing_slicing)]

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

/// The pack format this build reads.
pub const PACK_VERSION: u32 = 1;

/// Upper bound for one file inside a pack.
const MAX_ENTRY_BYTES: u64 = 16 * 1024 * 1024;

/// Where an emoji sits: sheet number and cell within the sheet (row-major).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub sheet: u16,
    pub index: u16,
}

#[derive(serde::Deserialize)]
struct Config {
    version: u32,
    cell: u32,
    columns: u32,
    rows: u32,
    sheets: u32,
    index: HashMap<String, (u16, u16)>,
}

/// A parsed `config.json`.
#[derive(Debug)]
pub struct SpritePack {
    /// Edge of one cell in pixels.
    pub cell: u32,
    pub columns: u32,
    pub rows: u32,
    pub sheets: u16,
    /// Keyed by the emoji without U+FE0F.
    index: HashMap<Box<str>, Cell>,
}

impl SpritePack {
    /// Parse and vet a `config.json`. Entries pointing outside the sheets
    /// are dropped rather than failing the pack.
    pub fn parse(json: &[u8]) -> Option<Self> {
        let config: Config = serde_json::from_slice(json).ok()?;
        if config.version != PACK_VERSION
            || !(16..=256).contains(&config.cell)
            || !(1..=64).contains(&config.columns)
            || !(1..=64).contains(&config.rows)
            || !(1..=64).contains(&config.sheets)
        {
            return None;
        }
        let per_sheet = config.columns * config.rows;
        let sheets = u16::try_from(config.sheets).ok()?;
        let index = config
            .index
            .into_iter()
            .filter(|(key, (sheet, cell))| {
                !key.is_empty() && *sheet < sheets && u32::from(*cell) < per_sheet
            })
            .map(|(key, (sheet, index))| (key.into_boxed_str(), Cell { sheet, index }))
            .collect();
        Some(Self {
            cell: config.cell,
            columns: config.columns,
            rows: config.rows,
            sheets,
            index,
        })
    }

    /// The cell of `key`, an emoji with U+FE0F already removed.
    pub fn get(&self, key: &str) -> Option<Cell> {
        self.index.get(key).copied()
    }

    /// The cell of an emoji as typed, with or without U+FE0F.
    pub fn lookup(&self, emoji: &str) -> Option<Cell> {
        if emoji.contains('\u{fe0f}') {
            self.get(&emoji.replace('\u{fe0f}', ""))
        } else {
            self.get(emoji)
        }
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// Pixel origin of cell `index` in its sheet.
    pub fn cell_origin(&self, index: u16) -> (u32, u32) {
        let index = u32::from(index);
        let columns = self.columns.max(1);
        ((index % columns) * self.cell, (index / columns) * self.cell)
    }

    /// Load the pack in `dir`, if it is complete.
    pub fn load(dir: &Path) -> Option<Self> {
        let pack = Self::parse(&std::fs::read(dir.join("config.json")).ok()?)?;
        (0..pack.sheets)
            .all(|n| sheet_path(dir, n).is_file())
            .then_some(pack)
    }
}

pub fn sheet_path(dir: &Path, sheet: u16) -> PathBuf {
    dir.join(format!("emoji_{sheet}.webp"))
}

/// Whether `dir` holds a complete pack (tdesktop `SetIsReady`).
pub fn is_ready(dir: &Path) -> bool {
    SpritePack::load(dir).is_some()
}

/// Only these names are taken out of a pack (tdesktop `GoodSetPartName`);
/// none can climb out of the target folder.
fn good_part_name(name: &str) -> bool {
    name == "config.json"
        || name == "LICENSE.txt"
        || name
            .strip_prefix("emoji_")
            .and_then(|rest| rest.strip_suffix(".webp"))
            .is_some_and(|n| !n.is_empty() && n.len() <= 3 && n.bytes().all(|b| b.is_ascii_digit()))
}

#[derive(Debug, thiserror::Error)]
pub enum UnpackError {
    #[error("The downloaded emoji set is damaged.")]
    Invalid,
    #[error("Couldn't save the emoji set: {0}")]
    Io(#[from] std::io::Error),
}

/// Unpack `zip` into `dir`, replacing what was there. The files go to a
/// sibling temp folder first, which is renamed into place only once the
/// pack in it loads, so `dir` never holds half a pack.
pub fn unpack(zip: &[u8], dir: &Path) -> Result<(), UnpackError> {
    let parent = dir.parent().ok_or(UnpackError::Invalid)?;
    let name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or(UnpackError::Invalid)?;
    std::fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(".{name}.{}.part", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp)?;
    let result = extract(zip, &tmp).and_then(|()| {
        if !is_ready(&tmp) {
            return Err(UnpackError::Invalid);
        }
        // Move the old pack aside rather than deleting it first, so a
        // failed rename puts it back.
        let old = parent.join(format!(".{name}.old"));
        let _ = std::fs::remove_dir_all(&old);
        let had_old = std::fs::rename(dir, &old).is_ok();
        if let Err(err) = crate::fs_retry::rename_retrying(&tmp, dir, crate::fs_retry::SHORT) {
            if had_old {
                let _ = std::fs::rename(&old, dir);
            }
            return Err(err.into());
        }
        let _ = std::fs::remove_dir_all(&old);
        Ok(())
    });
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&tmp);
    }
    result
}

fn extract(zip: &[u8], dir: &Path) -> Result<(), UnpackError> {
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(zip)).map_err(|_| UnpackError::Invalid)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|_| UnpackError::Invalid)?;
        if entry.is_dir() {
            continue;
        }
        let Ok(name) = entry.name().map(|n| n.to_string()) else {
            continue;
        };
        if !good_part_name(&name) {
            continue;
        }
        if entry.size() > MAX_ENTRY_BYTES {
            return Err(UnpackError::Invalid);
        }
        let mut bytes = Vec::new();
        (&mut entry)
            .take(MAX_ENTRY_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| UnpackError::Invalid)?;
        if bytes.len() as u64 > MAX_ENTRY_BYTES {
            return Err(UnpackError::Invalid);
        }
        std::fs::write(dir.join(&name), bytes)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    pub(crate) fn config(index: &str) -> String {
        format!(
            r#"{{"version":1,"cell":72,"columns":16,"rows":16,"sheets":2,"unicode":"17.0","index":{{{index}}}}}"#
        )
    }

    pub(crate) fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(&mut out);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, bytes) in files {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
        out.into_inner()
    }

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("quill-emoji-pack-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn parses_and_looks_up_with_or_without_fe0f() {
        let pack = SpritePack::parse(
            config(r#""😀":[0,0],"❤":[0,5],"👍🏽":[1,3],"👨‍👩‍👧":[1,4],"🇺🇸":[1,255]"#).as_bytes(),
        )
        .unwrap();
        assert_eq!(pack.len(), 5);
        assert_eq!(pack.lookup("😀"), Some(Cell { sheet: 0, index: 0 }));
        assert_eq!(pack.lookup("❤\u{fe0f}"), Some(Cell { sheet: 0, index: 5 }));
        assert_eq!(pack.lookup("❤"), Some(Cell { sheet: 0, index: 5 }));
        assert_eq!(pack.lookup("👍🏽"), Some(Cell { sheet: 1, index: 3 }));
        assert_eq!(pack.lookup("👨‍👩‍👧"), Some(Cell { sheet: 1, index: 4 }));
        assert_eq!(pack.lookup("🙂"), None);
        assert_eq!(pack.cell_origin(17), (72, 72));
    }

    #[test]
    fn rejects_other_versions_and_drops_cells_out_of_range() {
        assert!(
            SpritePack::parse(
                config("")
                    .replace("\"version\":1", "\"version\":2")
                    .as_bytes()
            )
            .is_none()
        );
        assert!(SpritePack::parse(b"not json").is_none());
        let pack =
            SpritePack::parse(config(r#""😀":[2,0],"😁":[0,256],"😂":[0,1]"#).as_bytes()).unwrap();
        assert_eq!(pack.len(), 1);
    }

    #[test]
    fn part_names_are_vetted() {
        assert!(good_part_name("config.json"));
        assert!(good_part_name("emoji_0.webp"));
        assert!(good_part_name("emoji_15.webp"));
        assert!(!good_part_name("../emoji_0.webp"));
        assert!(!good_part_name("emoji_/../x.webp"));
        assert!(!good_part_name("sub/config.json"));
        assert!(!good_part_name("emoji_.webp"));
        assert!(!good_part_name("evil.sh"));
    }

    #[test]
    fn unpack_installs_a_complete_pack_and_skips_traversal() {
        let dir = temp("ok").join("noto");
        let cfg = config(r#""😀":[0,0]"#);
        let zip = zip_of(&[
            ("config.json", cfg.as_bytes()),
            ("emoji_0.webp", b"x"),
            ("emoji_1.webp", b"y"),
            ("../escape.txt", b"z"),
        ]);
        unpack(&zip, &dir).unwrap();
        // Installing again replaces the pack in place.
        unpack(&zip, &dir).unwrap();
        assert!(is_ready(&dir));
        assert!(!dir.parent().unwrap().join("escape.txt").exists());
        let left: Vec<_> = std::fs::read_dir(dir.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(left, ["noto"]);
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    #[test]
    fn unpack_refuses_an_incomplete_pack_and_keeps_the_old_one() {
        let dir = temp("partial").join("noto");
        let cfg = config(r#""😀":[0,0]"#);
        let full = zip_of(&[
            ("config.json", cfg.as_bytes()),
            ("emoji_0.webp", b"x"),
            ("emoji_1.webp", b"y"),
        ]);
        unpack(&full, &dir).unwrap();
        let partial = zip_of(&[("config.json", cfg.as_bytes()), ("emoji_0.webp", b"x")]);
        assert!(matches!(unpack(&partial, &dir), Err(UnpackError::Invalid)));
        assert!(matches!(
            unpack(b"garbage", &dir),
            Err(UnpackError::Invalid)
        ));
        assert!(is_ready(&dir));
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }
}
