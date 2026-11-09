//! What one widget keeps between runs: its `localStorage` and the last
//! height it had, so it opens at that height next time. One small JSON file
//! per message in the account's widget folder
//! ([`crate::account_data::widget_dir`]), dropped with the account.
//! `sessionStorage` lives only as long as the app run and is never written.

use super::{MAX_HEIGHT, MIN_HEIGHT, WidgetKey};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The most one storage area holds, keys and values together, in bytes
/// (widget-sandbox's and Telegram Desktop's `STORAGE_QUOTA_BYTES`).
pub const QUOTA_BYTES: usize = 4096;

/// One storage area: key to value, as the widget wrote them.
pub type Area = BTreeMap<String, String>;

/// What a widget left on disk.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Stored {
    #[serde(default)]
    pub local: Area,
    /// The height the widget last reported, CSS px.
    #[serde(default)]
    pub height: Option<f32>,
}

/// The bytes an area takes against [`QUOTA_BYTES`].
pub fn area_bytes(area: &Area) -> usize {
    area.iter().map(|(key, value)| key.len() + value.len()).sum()
}

impl Stored {
    /// The same, held to the bounds a widget may keep: an area over the
    /// quota is dropped whole (the sandbox never commits one, so it is a
    /// damaged or foreign file), a height out of bounds is forgotten.
    pub fn bounded(mut self) -> Self {
        if area_bytes(&self.local) > QUOTA_BYTES {
            self.local.clear();
        }
        self.height = self
            .height
            .filter(|height| height.is_finite() && (MIN_HEIGHT..=MAX_HEIGHT).contains(height));
        self
    }
}

/// The file of `key`'s widget in `dir`.
pub fn file(dir: &Path, key: WidgetKey) -> PathBuf {
    dir.join(format!("{}_{}.json", key.chat_id.0, key.message_id.0))
}

/// What `key`'s widget left, or nothing for a new one or an unreadable file.
pub fn load(dir: &Path, key: WidgetKey) -> Stored {
    std::fs::read(file(dir, key))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Stored>(&bytes).ok())
        .map(Stored::bounded)
        .unwrap_or_default()
}

/// Keep `stored` for `key`'s widget; an empty one removes the file.
pub fn save(dir: &Path, key: WidgetKey, stored: &Stored) -> std::io::Result<()> {
    let path = file(dir, key);
    let stored = stored.clone().bounded();
    if stored == Stored::default() {
        return match std::fs::remove_file(&path) {
            Err(err) if err.kind() != std::io::ErrorKind::NotFound => Err(err),
            _ => Ok(()),
        };
    }
    std::fs::create_dir_all(dir)?;
    crate::settings::write_json_atomic(&path, &stored)
}

/// Drop what `key`'s widget kept (its message was deleted).
pub fn remove(dir: &Path, key: WidgetKey) {
    let _ = std::fs::remove_file(file(dir, key));
}
