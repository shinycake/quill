//! Custom themes on disk: one `.tdesktop-theme` file per theme in the
//! app's `themes` folder, named after the theme. Importing copies the file
//! there, the editor saves there, and the appearance prefs refer to a
//! theme by its name.

use super::file::{ThemeFile, ThemeFileError};
use std::path::{Path, PathBuf};

pub const EXTENSION: &str = "tdesktop-theme";
const NAME_LIMIT: usize = 64;

/// The folder custom themes live in, under the app's settings root.
pub fn themes_dir(settings_root: &Path) -> PathBuf {
    settings_root.join("themes")
}

/// A theme name safe as a file name on every platform: letters, digits,
/// spaces, `-`, `_`, `(`, `)` and `.` (not leading), at most 64
/// characters, trimmed. Empty names become "Custom theme".
pub fn clean_name(name: &str) -> String {
    let kept: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '(' | ')' | '.'))
        .take(NAME_LIMIT)
        .collect();
    let trimmed = kept.trim().trim_start_matches('.').trim();
    if trimmed.is_empty() {
        "Custom theme".to_string()
    } else {
        trimmed.to_string()
    }
}

/// The theme name for an imported file: its stem, cleaned.
pub fn name_from_path(path: &Path) -> String {
    clean_name(
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default(),
    )
}

pub fn theme_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{}.{EXTENSION}", clean_name(name)))
}

/// `name`, or "name 2", "name 3"… so it does not replace a theme in
/// `taken` (compared without case, as on case-insensitive file systems).
pub fn unique_name(name: &str, taken: &[String]) -> String {
    let base = clean_name(name);
    let free = |candidate: &str| !taken.iter().any(|t| t.eq_ignore_ascii_case(candidate));
    if free(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base} {n}"))
        .find(|c| free(c))
        .unwrap_or(base)
}

/// The custom theme names in `dir`, sorted without case.
pub fn list(dir: &Path) -> Vec<String> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = read
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some(EXTENSION))
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str()).map(String::from))
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names
}

/// Why a custom theme could not be read or written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreError {
    Io(String),
    File(ThemeFileError),
}

impl StoreError {
    pub fn note(&self) -> String {
        match self {
            StoreError::Io(why) => format!("Couldn't access the theme file: {why}"),
            StoreError::File(err) => err.note(),
        }
    }
}

impl From<std::io::Error> for StoreError {
    fn from(err: std::io::Error) -> Self {
        StoreError::Io(err.to_string())
    }
}

/// Read and parse a theme file anywhere on disk.
pub fn read(path: &Path) -> Result<ThemeFile, StoreError> {
    let len = std::fs::metadata(path)?.len();
    if len > super::file::THEME_FILE_LIMIT as u64 {
        return Err(StoreError::File(ThemeFileError::TooLarge));
    }
    ThemeFile::parse(&std::fs::read(path)?).map_err(StoreError::File)
}

/// Load the custom theme `name`.
pub fn load(dir: &Path, name: &str) -> Result<ThemeFile, StoreError> {
    read(&theme_path(dir, name))
}

/// Write `file` as the custom theme `name`, replacing one of that name.
/// The file is written next to its final path and renamed over it, so a
/// crash never leaves half a theme.
pub fn save(dir: &Path, name: &str, file: &ThemeFile) -> Result<PathBuf, StoreError> {
    std::fs::create_dir_all(dir)?;
    let path = theme_path(dir, name);
    let temp = path.with_extension(format!("{EXTENSION}.part"));
    std::fs::write(&temp, file.to_zip())?;
    crate::fs_retry::rename_retrying(&temp, &path, crate::fs_retry::SHORT)?;
    Ok(path)
}

pub fn delete(dir: &Path, name: &str) -> Result<(), StoreError> {
    std::fs::remove_file(theme_path(dir, name))?;
    Ok(())
}

/// Import the theme file at `source` as a custom theme named after it
/// (made unique among `taken`). Returns the name it was saved under.
pub fn import(
    dir: &Path,
    source: &Path,
    taken: &[String],
) -> Result<(String, ThemeFile), StoreError> {
    let file = read(source)?;
    if !file.sets_known_keys() {
        return Err(StoreError::File(ThemeFileError::NothingToApply));
    }
    let name = unique_name(&name_from_path(source), taken);
    save(dir, &name, &file)?;
    Ok((name, file))
}
