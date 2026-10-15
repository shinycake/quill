//! The emoji sets Quill offers and the state line of each row in the
//! "Choose emoji set" box (tdesktop `emoji_sets_manager.cpp` `kSets`,
//! `ComputeState`, `StateDescription`).
#![deny(clippy::indexing_slicing)]

use std::path::{Path, PathBuf};

/// Release of shinycake/quill the packs are attached to. A pinned tag keeps
/// what is downloaded reproducible; bump it together with the hashes.
pub const RELEASE_TAG: &str = "emoji-sets-v1";

/// Upper bound for one downloaded pack; a bigger answer is not a pack.
pub const MAX_PACK_BYTES: u64 = 32 * 1024 * 1024;

/// One emoji set. `System` draws emoji with the platform's emoji font
/// (tdesktop's built-in set); the others are sprite packs Quill downloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum EmojiSetId {
    #[default]
    System,
    Noto,
    Twemoji,
}

impl EmojiSetId {
    /// In the box's order (tdesktop lists the built-in set first).
    pub const ALL: [EmojiSetId; 3] = [Self::System, Self::Noto, Self::Twemoji];

    /// The id used in file names and the saved choice.
    pub fn key(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Noto => "noto",
            Self::Twemoji => "twemoji",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|id| id.key() == key.trim())
    }

    /// The row's title. The system set is named after the platform's
    /// emoji, as tdesktop names its built-in set after the art ("Mac").
    pub fn name(self) -> &'static str {
        match self {
            Self::System if cfg!(target_os = "macos") => "Apple",
            Self::System if cfg!(target_os = "windows") => "Segoe",
            Self::System => "System",
            Self::Noto => "Noto",
            Self::Twemoji => "Twemoji",
        }
    }

    /// The pack's size in bytes ("Download 4.3 MB"); 0 for the system set.
    pub fn bytes(self) -> u64 {
        match self {
            Self::System => 0,
            Self::Noto => 4_464_013,
            Self::Twemoji => 3_107_720,
        }
    }

    /// SHA-256 of the pack, lowercase hex. A download that does not match
    /// is thrown away.
    pub fn sha256(self) -> &'static str {
        match self {
            Self::System => "",
            Self::Noto => "47416bd07bf8480ce912d711ec211ad3cf41e2825929f5f9da56c24828b8269b",
            Self::Twemoji => "1ea7d7708179af53cbb480e5cd10d93e5632d8d5237c00925a07ffc32150fedd",
        }
    }

    pub fn is_system(self) -> bool {
        self == Self::System
    }

    /// Where the pack is downloaded from.
    pub fn download_url(self) -> String {
        format!(
            "https://github.com/shinycake/quill/releases/download/{RELEASE_TAG}/emoji-set-{}.zip",
            self.key()
        )
    }

    /// The art's license, shown under the list (Twemoji's CC-BY needs
    /// attribution).
    pub fn credit(self) -> Option<&'static str> {
        match self {
            Self::System => None,
            Self::Noto => Some("Noto Emoji by Google, Apache License 2.0"),
            Self::Twemoji => Some("Twemoji by Twitter, Inc. and contributors, CC-BY 4.0"),
        }
    }
}

/// Where downloaded sets live: `<app data>/emoji-sets/<key>/`.
pub fn sets_dir(app_root: &Path) -> PathBuf {
    app_root.join("emoji-sets")
}

pub fn set_dir(app_root: &Path, id: EmojiSetId) -> PathBuf {
    sets_dir(app_root).join(id.key())
}

const CURRENT_FILE: &str = "current";

/// The chosen set. App-wide, like tdesktop's (not per account). A missing
/// or unknown value is the system set.
pub fn load_current(app_root: &Path) -> EmojiSetId {
    std::fs::read_to_string(sets_dir(app_root).join(CURRENT_FILE))
        .ok()
        .and_then(|key| EmojiSetId::from_key(&key))
        .unwrap_or_default()
}

pub fn save_current(app_root: &Path, id: EmojiSetId) -> std::io::Result<()> {
    let dir = sets_dir(app_root);
    std::fs::create_dir_all(&dir)?;
    let tmp = dir.join(".current.part");
    std::fs::write(&tmp, id.key())?;
    std::fs::rename(&tmp, dir.join(CURRENT_FILE)).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// A row's state (tdesktop `SetState`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetState {
    /// The set in use.
    Active,
    /// Downloaded, not in use.
    Ready,
    /// Not downloaded; the pack's size.
    Available(u64),
    /// Downloading: bytes so far of the total.
    Loading { done: u64, total: u64 },
    /// The last download failed; the reason.
    Failed(String),
}

impl SetState {
    /// tdesktop `ComputeState` with Quill's download and failure on top.
    pub fn of(
        id: EmojiSetId,
        current: EmojiSetId,
        ready: bool,
        loading: Option<(EmojiSetId, u64)>,
        failed: Option<(EmojiSetId, &str)>,
    ) -> Self {
        if id == current {
            return Self::Active;
        }
        if let Some((loading, done)) = loading
            && loading == id
        {
            return Self::Loading {
                done,
                total: id.bytes(),
            };
        }
        if id.is_system() || ready {
            return Self::Ready;
        }
        if let Some((failed, reason)) = failed
            && failed == id
        {
            return Self::Failed(reason.to_string());
        }
        Self::Available(id.bytes())
    }

    /// The status line (tdesktop `lng_emoji_set_*`).
    pub fn description(&self) -> String {
        match self {
            Self::Active => "Current set".into(),
            Self::Ready => "Downloaded".into(),
            Self::Available(bytes) => format!("Download {}", format_size(*bytes)),
            Self::Loading { done, total } => {
                let percent = if *total == 0 {
                    0
                } else {
                    (done.saturating_mul(100) / total).min(99)
                };
                format!(
                    "{percent}%, {} / {}",
                    format_size(*done),
                    format_size(*total)
                )
            }
            Self::Failed(reason) => reason.clone(),
        }
    }

    /// Clicking the row downloads or switches (tdesktop: only Ready and
    /// Available rows react).
    pub fn clickable(&self) -> bool {
        matches!(self, Self::Ready | Self::Available(_) | Self::Failed(_))
    }

    /// Fraction downloaded, for the progress bar.
    pub fn progress(&self) -> Option<f32> {
        match self {
            Self::Loading { done, total } if *total > 0 => {
                Some((*done as f64 / *total as f64).clamp(0.0, 1.0) as f32)
            }
            Self::Loading { .. } => Some(0.0),
            _ => None,
        }
    }
}

/// tdesktop `Ui::FormatSizeText` for the row: "4.2 MB", "512 KB".
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    if bytes >= MB {
        let tenths = bytes / (MB / 10);
        format!("{}.{} MB", tenths / 10, tenths % 10)
    } else if bytes >= KB {
        let tenths = bytes * 10 / KB;
        format!("{}.{} KB", tenths / 10, tenths % 10)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip_and_unknown_is_none() {
        for id in EmojiSetId::ALL {
            assert_eq!(EmojiSetId::from_key(id.key()), Some(id));
        }
        assert_eq!(EmojiSetId::from_key("joypixels"), None);
        assert_eq!(EmojiSetId::from_key("noto\n"), Some(EmojiSetId::Noto));
    }

    #[test]
    fn packs_have_a_url_size_and_hash() {
        for id in EmojiSetId::ALL.into_iter().filter(|id| !id.is_system()) {
            assert!(
                id.download_url()
                    .starts_with("https://github.com/shinycake/quill/")
            );
            assert!(
                id.download_url()
                    .ends_with(&format!("emoji-set-{}.zip", id.key()))
            );
            assert!(id.bytes() > 0 && id.bytes() < MAX_PACK_BYTES);
            assert_eq!(id.sha256().len(), 64);
            assert!(id.credit().is_some());
        }
    }

    #[test]
    fn state_follows_tdesktop() {
        use EmojiSetId::*;
        assert_eq!(SetState::of(Noto, Noto, true, None, None), SetState::Active);
        assert_eq!(
            SetState::of(System, Noto, false, None, None),
            SetState::Ready
        );
        assert_eq!(
            SetState::of(Noto, System, true, None, None),
            SetState::Ready
        );
        assert_eq!(
            SetState::of(Twemoji, System, false, None, None),
            SetState::Available(Twemoji.bytes())
        );
        assert_eq!(
            SetState::of(Twemoji, System, false, Some((Twemoji, 10)), None),
            SetState::Loading {
                done: 10,
                total: Twemoji.bytes()
            }
        );
        assert_eq!(
            SetState::of(Twemoji, System, false, None, Some((Twemoji, "Nope."))),
            SetState::Failed("Nope.".into())
        );
        assert!(!SetState::Active.clickable());
        assert!(SetState::Ready.clickable());
    }

    #[test]
    fn descriptions_match_tdesktop_strings() {
        assert_eq!(SetState::Active.description(), "Current set");
        assert_eq!(SetState::Ready.description(), "Downloaded");
        assert_eq!(
            SetState::Available(3 * 1024 * 1024).description(),
            "Download 3.0 MB"
        );
        assert_eq!(
            SetState::Loading {
                done: 1024 * 1024,
                total: 4 * 1024 * 1024
            }
            .description(),
            "25%, 1.0 MB / 4.0 MB"
        );
    }

    #[test]
    fn current_set_persists() {
        let root = std::env::temp_dir().join(format!("quill-emoji-current-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(load_current(&root), EmojiSetId::System);
        save_current(&root, EmojiSetId::Twemoji).unwrap();
        assert_eq!(load_current(&root), EmojiSetId::Twemoji);
        std::fs::write(sets_dir(&root).join(CURRENT_FILE), "joypixels").unwrap();
        assert_eq!(load_current(&root), EmojiSetId::System);
        let _ = std::fs::remove_dir_all(&root);
    }
}
