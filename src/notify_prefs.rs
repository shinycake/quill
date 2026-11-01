//! App-wide desktop notification preferences shared by every account:
//! the notification sound volume (tdesktop `Core::Settings::
//! notificationsVolume`, 0-100, default 100, applied linearly to the sound
//! `Track::playWithLooping`). Stored in `notify_prefs.json` under the app
//! data root, next to `file_prefs.json`.
//!
//! The other "Desktop notifications" options (use native notifications,
//! screen corner, count, display, respect Focus mode) drive Quill's own
//! notification windows where those are used (`crate::notify::custom`,
//! `docs/decisions/notification-windows-system-proxy.md`).
#![deny(clippy::indexing_slicing)]

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// Highest slider value (percent).
pub const MAX_VOLUME: u8 = 100;

/// App-wide notification preferences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NotifyPrefs {
    /// Notification sound volume in percent (0 = silent).
    pub volume: u8,
    /// "Use native notifications" / "Use Windows notifications", once
    /// set (tdesktop `_nativeNotifications`, unset means the platform's
    /// default).
    pub native: Option<bool>,
    /// "Location on the screen" (tdesktop `notificationsCorner`).
    pub corner: crate::notify::custom::ScreenCorner,
    /// "Notifications count", 1-5 (tdesktop `notificationsCount`).
    pub count: u8,
    /// "Display for notifications": the display's checksum, 0 = Default
    /// (tdesktop `notificationsDisplayChecksum`).
    pub display: u64,
    /// Windows "Respect system Focus mode" (tdesktop `skipToastsInFocus`).
    pub skip_in_focus: bool,
}

impl Default for NotifyPrefs {
    fn default() -> Self {
        Self {
            volume: MAX_VOLUME,
            native: None,
            corner: crate::notify::custom::ScreenCorner::default(),
            count: crate::notify::custom::DEFAULT_COUNT,
            display: 0,
            skip_in_focus: false,
        }
    }
}

impl NotifyPrefs {
    /// The saved volume clamped to 0-100, so a hand-edited file can't
    /// amplify.
    pub fn volume_percent(self) -> u8 {
        self.volume.min(MAX_VOLUME)
    }

    /// Linear playback gain for the player (0.0 - 1.0).
    pub fn gain(self) -> f32 {
        f32::from(self.volume_percent()) / f32::from(MAX_VOLUME)
    }
}

static PREFS: RwLock<Option<NotifyPrefs>> = RwLock::new(None);
static PERSIST: AtomicBool = AtomicBool::new(true);

/// Turn disk writes off (tests must never touch the real file).
pub fn set_persistence(enabled: bool) {
    PERSIST.store(enabled, Ordering::SeqCst);
}

fn prefs_path() -> Option<PathBuf> {
    crate::settings::safe_app_root().map(|root| root.join("notify_prefs.json"))
}

fn load_from_disk() -> NotifyPrefs {
    prefs_path()
        .and_then(|path| crate::settings::read_with_pending(&path).ok())
        .and_then(|bytes| serde_json::from_slice::<NotifyPrefs>(&bytes).ok())
        .unwrap_or_default()
}

/// The current preferences (loaded from disk on first use).
pub fn current() -> NotifyPrefs {
    if let Ok(guard) = PREFS.read()
        && let Some(prefs) = *guard
    {
        return prefs;
    }
    let loaded = load_from_disk();
    match PREFS.write() {
        Ok(mut guard) => *guard.get_or_insert(loaded),
        Err(_) => loaded,
    }
}

/// Replace the preferences and persist them. A failed write keeps the
/// in-memory value for this run.
pub fn store(prefs: NotifyPrefs) {
    if PERSIST.load(Ordering::SeqCst)
        && let Some(path) = prefs_path()
    {
        crate::settings::write_json_deferred(&path, &prefs);
    }
    if let Ok(mut guard) = PREFS.write() {
        *guard = Some(prefs);
    }
}

/// Save a new volume percent (clamped to 0-100).
pub fn set_volume(percent: u8) {
    update(|prefs| prefs.volume = percent.min(MAX_VOLUME));
}

/// Change the volume in memory only (slider drags); [`persist`] saves it.
pub fn set_volume_memory(percent: u8) {
    let mut prefs = current();
    prefs.volume = percent.min(MAX_VOLUME);
    if let Ok(mut guard) = PREFS.write() {
        *guard = Some(prefs);
    }
}

/// Save the in-memory preferences through the background settings writer
/// (the write syncs the disk).
pub fn persist() {
    if !PERSIST.load(Ordering::SeqCst) {
        return;
    }
    if let Some(path) = prefs_path() {
        crate::settings::write_json_deferred(&path, &current());
    }
}

/// Change some preferences and save them.
pub fn update(change: impl FnOnce(&mut NotifyPrefs)) {
    let mut prefs = current();
    change(&mut prefs);
    store(prefs);
}

#[cfg(test)]
mod tests {
    use super::{NotifyPrefs, current, set_persistence, set_volume, store};

    #[test]
    fn default_is_full_volume() {
        assert_eq!(NotifyPrefs::default().volume, 100);
        assert!((NotifyPrefs::default().gain() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn gain_is_linear_and_clamped() {
        assert_eq!(
            NotifyPrefs {
                volume: 0,
                ..NotifyPrefs::default()
            }
            .gain(),
            0.0
        );
        assert!(
            (NotifyPrefs {
                volume: 50,
                ..NotifyPrefs::default()
            }
            .gain()
                - 0.5)
                .abs()
                < f32::EPSILON
        );
        assert!(
            (NotifyPrefs {
                volume: 250,
                ..NotifyPrefs::default()
            }
            .gain()
                - 1.0)
                .abs()
                < f32::EPSILON
        );
        assert_eq!(
            NotifyPrefs {
                volume: 250,
                ..NotifyPrefs::default()
            }
            .volume_percent(),
            100
        );
    }

    #[test]
    fn json_round_trips_and_tolerates_missing_fields() {
        let prefs = NotifyPrefs {
            volume: 35,
            count: 5,
            native: Some(false),
            ..NotifyPrefs::default()
        };
        let text = serde_json::to_string(&prefs).unwrap();
        assert_eq!(serde_json::from_str::<NotifyPrefs>(&text).unwrap(), prefs);
        assert_eq!(
            serde_json::from_str::<NotifyPrefs>("{}").unwrap(),
            NotifyPrefs::default()
        );
    }

    #[test]
    fn store_keeps_the_value_in_memory() {
        set_persistence(false);
        set_volume(40);
        assert_eq!(current().volume, 40);
        set_volume(200);
        assert_eq!(current().volume, 100);
        store(NotifyPrefs::default());
    }
}
