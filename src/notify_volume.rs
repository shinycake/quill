//! Notification sound volume per scope and per chat (tdesktop
//! `SessionSettings::ringtoneVolume`, `Data::DefaultRingtonesVolumeController`
//! and `ThreadRingtonesVolumeController`). Client-side, per account, stored
//! in `notify_volumes.json` next to `notify_prefs.json`.
//!
//! tdesktop keeps a percent from 1 to 100 per chat and per scope, where 0
//! means "not set". When a notification sound plays, the chat's value wins,
//! then the scope's, and only when neither is set does the app-wide
//! `notificationsVolume` ([`crate::notify_prefs`]) apply
//! (`window/notifications_manager.cpp` `playOnce`, `media_audio_track.cpp`
//! `volumeOverride`). A scope or chat value replaces the app-wide one, it is
//! not multiplied with it. The sliders show 100 while nothing is set.
#![deny(clippy::indexing_slicing)]

use crate::ids::ChatId;
use crate::telegram::envelope::NotificationSettingsScope;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// Highest percent (tdesktop slider end).
pub const MAX_PERCENT: u8 = 100;
/// Lowest percent the slider can pick; 0 stands for "not set".
pub const MIN_PERCENT: u8 = 1;

/// One account's volumes. A field of 0 means "not set".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AccountVolumes {
    pub private_chats: u8,
    pub group_chats: u8,
    pub channel_chats: u8,
    /// Per-chat percent, keyed by chat id.
    pub chats: BTreeMap<i64, u8>,
}

/// Percent to keep: 1-100 as is, anything else clears the value
/// (tdesktop `setRingtoneVolume`).
fn normalized(percent: u8) -> u8 {
    if (MIN_PERCENT..=MAX_PERCENT).contains(&percent) {
        percent
    } else {
        0
    }
}

impl AccountVolumes {
    /// The saved percent for a scope, 0 when not set.
    pub fn scope_percent(&self, scope: NotificationSettingsScope) -> u8 {
        match scope {
            NotificationSettingsScope::PrivateChats => self.private_chats,
            NotificationSettingsScope::GroupChats => self.group_chats,
            NotificationSettingsScope::ChannelChats => self.channel_chats,
        }
    }

    /// The saved percent for a chat, 0 when not set.
    pub fn chat_percent(&self, chat: ChatId) -> u8 {
        self.chats.get(&chat.0).copied().unwrap_or(0)
    }

    /// Save a scope percent; 0 or above 100 clears it.
    pub fn set_scope(&mut self, scope: NotificationSettingsScope, percent: u8) {
        let value = normalized(percent);
        match scope {
            NotificationSettingsScope::PrivateChats => self.private_chats = value,
            NotificationSettingsScope::GroupChats => self.group_chats = value,
            NotificationSettingsScope::ChannelChats => self.channel_chats = value,
        }
    }

    /// Save a chat percent; 0 or above 100 clears it.
    pub fn set_chat(&mut self, chat: ChatId, percent: u8) {
        match normalized(percent) {
            0 => {
                self.chats.remove(&chat.0);
            }
            value => {
                self.chats.insert(chat.0, value);
            }
        }
    }

    /// What a scope slider shows: the saved percent, 100 when not set.
    pub fn scope_slider_percent(&self, scope: NotificationSettingsScope) -> u8 {
        shown(self.scope_percent(scope))
    }

    /// What a chat slider shows: the saved percent, 100 when not set.
    pub fn chat_slider_percent(&self, chat: ChatId) -> u8 {
        shown(self.chat_percent(chat))
    }

    /// The volume override for a chat's notification sound: the chat's
    /// percent, else its scope's, `None` when neither is set.
    pub fn override_percent(&self, chat: ChatId, scope: NotificationSettingsScope) -> Option<u8> {
        resolve_percent(self.chat_percent(chat), self.scope_percent(scope))
    }
}

fn shown(saved: u8) -> u8 {
    if saved == 0 { MAX_PERCENT } else { saved }
}

/// The chat value if set, else the scope value, else `None` (0 = not set).
pub fn resolve_percent(chat: u8, scope: u8) -> Option<u8> {
    [chat, scope]
        .into_iter()
        .map(normalized)
        .find(|percent| *percent != 0)
}

/// Linear gain (0.0-1.0) for a sound: an override replaces the app-wide
/// gain.
pub fn resolve_gain(override_percent: Option<u8>, global_gain: f32) -> f32 {
    match override_percent {
        Some(percent) => f32::from(percent.min(MAX_PERCENT)) / f32::from(MAX_PERCENT),
        None => global_gain,
    }
}

/// Every account's volumes, as stored on disk.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct VolumeFile {
    accounts: BTreeMap<String, AccountVolumes>,
}

static VOLUMES: RwLock<Option<VolumeFile>> = RwLock::new(None);
static PERSIST: AtomicBool = AtomicBool::new(true);
static PERSIST_REQUESTS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static WRITE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Turn disk writes off (tests must never touch the real file).
pub fn set_persistence(enabled: bool) {
    PERSIST.store(enabled, Ordering::SeqCst);
}

fn volumes_path() -> Option<PathBuf> {
    crate::settings::safe_app_root().map(|root| root.join("notify_volumes.json"))
}

fn load_from_disk() -> VolumeFile {
    let Some(path) = volumes_path() else {
        return VolumeFile::default();
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return VolumeFile::default();
    };
    serde_json::from_slice::<VolumeFile>(&bytes).unwrap_or_else(|err| {
        eprintln!(
            "quill: {} is unreadable ({err}); notification volumes were reset",
            path.display()
        );
        VolumeFile::default()
    })
}

/// Run `read` on the cached file (read from disk once, on first use).
fn with_file<R>(read: impl FnOnce(&VolumeFile) -> R) -> R {
    if let Ok(guard) = VOLUMES.read()
        && let Some(file) = guard.as_ref()
    {
        return read(file);
    }
    let loaded = load_from_disk();
    match VOLUMES.write() {
        Ok(mut guard) => read(guard.get_or_insert(loaded)),
        Err(_) => read(&loaded),
    }
}

/// An account's saved volumes.
pub fn account(account: &str) -> AccountVolumes {
    with_file(|file| file.accounts.get(account).cloned().unwrap_or_default())
}

/// Change an account's volumes in memory only (slider drags); call
/// [`persist`] when the change is final.
pub fn update_memory(account: &str, change: impl FnOnce(&mut AccountVolumes)) {
    with_file(|_| ());
    let Ok(mut guard) = VOLUMES.write() else {
        return;
    };
    let file = guard.get_or_insert_with(VolumeFile::default);
    let mut volumes = file.accounts.remove(account).unwrap_or_default();
    change(&mut volumes);
    if volumes != AccountVolumes::default() {
        file.accounts.insert(account.to_string(), volumes);
    }
}

/// Save the in-memory volumes on a worker thread (the write syncs the
/// disk). A failed write keeps the in-memory value for this run.
pub fn persist() {
    PERSIST_REQUESTS.fetch_add(1, Ordering::SeqCst);
    if !PERSIST.load(Ordering::SeqCst) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("quill-notify-volumes".into())
        .spawn(|| {
            let _guard = WRITE_LOCK.lock();
            let Some(path) = volumes_path() else { return };
            let file = with_file(Clone::clone);
            let _ = crate::settings::write_json_atomic(&path, &file);
        });
    drop(spawned);
}

/// How many times a save was requested (tests check that drags don't).
pub fn persist_requests() -> usize {
    PERSIST_REQUESTS.load(Ordering::SeqCst)
}

/// Change an account's volumes and save them.
pub fn update(account: &str, change: impl FnOnce(&mut AccountVolumes)) {
    update_memory(account, change);
    persist();
}

/// The override for one chat's notification sound, from the saved values.
pub fn override_percent(
    account_key: &str,
    chat: ChatId,
    scope: NotificationSettingsScope,
) -> Option<u8> {
    with_file(|file| {
        file.accounts
            .get(account_key)
            .and_then(|volumes| volumes.override_percent(chat, scope))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use NotificationSettingsScope::{ChannelChats, GroupChats, PrivateChats};

    #[test]
    fn nothing_set_falls_back_to_the_app_volume() {
        let volumes = AccountVolumes::default();
        assert_eq!(volumes.override_percent(ChatId(5), PrivateChats), None);
        assert!((resolve_gain(None, 0.3) - 0.3).abs() < f32::EPSILON);
    }

    #[test]
    fn scope_value_replaces_the_app_volume() {
        let mut volumes = AccountVolumes::default();
        volumes.set_scope(GroupChats, 40);
        assert_eq!(volumes.override_percent(ChatId(5), GroupChats), Some(40));
        assert_eq!(volumes.override_percent(ChatId(5), PrivateChats), None);
        let loud_app = resolve_gain(volumes.override_percent(ChatId(5), GroupChats), 1.0);
        assert!((loud_app - 0.4).abs() < 1e-6);
        let quiet_app = resolve_gain(volumes.override_percent(ChatId(5), GroupChats), 0.1);
        assert!((quiet_app - 0.4).abs() < 1e-6, "not multiplied");
    }

    #[test]
    fn chat_value_wins_over_the_scope_value() {
        let mut volumes = AccountVolumes::default();
        volumes.set_scope(ChannelChats, 80);
        volumes.set_chat(ChatId(9), 25);
        assert_eq!(volumes.override_percent(ChatId(9), ChannelChats), Some(25));
        assert_eq!(volumes.override_percent(ChatId(10), ChannelChats), Some(80));
    }

    #[test]
    fn full_volume_is_an_explicit_override() {
        let mut volumes = AccountVolumes::default();
        volumes.set_scope(PrivateChats, 100);
        assert_eq!(volumes.override_percent(ChatId(1), PrivateChats), Some(100));
        assert!((resolve_gain(Some(100), 0.2) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn zero_and_out_of_range_clear_the_value() {
        let mut volumes = AccountVolumes::default();
        volumes.set_chat(ChatId(1), 50);
        volumes.set_chat(ChatId(1), 0);
        assert!(volumes.chats.is_empty());
        volumes.set_chat(ChatId(1), 101);
        assert!(volumes.chats.is_empty());
        volumes.set_scope(PrivateChats, 70);
        volumes.set_scope(PrivateChats, 0);
        assert_eq!(volumes.scope_percent(PrivateChats), 0);
        assert_eq!(resolve_percent(250, 0), None);
    }

    #[test]
    fn sliders_show_full_volume_while_unset() {
        let mut volumes = AccountVolumes::default();
        assert_eq!(volumes.scope_slider_percent(GroupChats), 100);
        assert_eq!(volumes.chat_slider_percent(ChatId(3)), 100);
        volumes.set_chat(ChatId(3), 12);
        assert_eq!(volumes.chat_slider_percent(ChatId(3)), 12);
    }

    #[test]
    fn json_round_trips_and_tolerates_missing_fields() {
        let mut volumes = AccountVolumes::default();
        volumes.set_scope(GroupChats, 33);
        volumes.set_chat(ChatId(-100), 7);
        let text = serde_json::to_string(&volumes).unwrap_or_default();
        assert_eq!(
            serde_json::from_str::<AccountVolumes>(&text).ok(),
            Some(volumes)
        );
        assert_eq!(
            serde_json::from_str::<AccountVolumes>("{}").ok(),
            Some(AccountVolumes::default())
        );
    }

    #[test]
    fn accounts_are_kept_apart_in_memory() {
        set_persistence(false);
        update("volume-test-a", |v| v.set_chat(ChatId(1), 20));
        assert_eq!(account("volume-test-a").chat_percent(ChatId(1)), 20);
        assert_eq!(account("volume-test-b").chat_percent(ChatId(1)), 0);
        update("volume-test-a", |v| v.set_chat(ChatId(1), 0));
        assert_eq!(account("volume-test-a"), AccountVolumes::default());
    }
}
