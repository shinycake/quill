//! The folders TDLib writes to, each read on the volume it really lives
//! on: an `accounts`, `tdlib` or `files` folder can be a symlink to another
//! disk, and the tightest volume is the one that fills first.
#![deny(clippy::indexing_slicing)]

use super::probe::Volume;
use super::{DataReading, DataUse};
use std::path::{Path, PathBuf};

/// The data root, `accounts`, and each account folder with its `tdlib` and
/// `files` folders, symlinks resolved and each real folder listed once.
pub(super) fn data_folders(root: &Path) -> Vec<PathBuf> {
    let accounts = root.join("accounts");
    let mut folders = vec![root.to_path_buf(), accounts.clone()];
    if let Ok(entries) = std::fs::read_dir(&accounts) {
        for entry in entries.flatten() {
            let account = entry.path();
            folders.push(account.join("tdlib"));
            folders.push(account.join("files"));
            folders.push(account);
        }
    }
    let mut real: Vec<PathBuf> = folders
        .iter()
        .filter_map(|folder| std::fs::canonicalize(folder).ok())
        .collect();
    real.sort();
    real.dedup();
    if real.is_empty() {
        real.push(root.to_path_buf());
    }
    real
}

/// `folders` read by `read`: the least free space over them, and whether
/// any one cannot be written.
pub(super) fn tightest(folders: &[PathBuf], read: impl Fn(&Path) -> Volume) -> DataReading {
    let volumes: Vec<Volume> = folders.iter().map(|folder| read(folder)).collect();
    DataReading {
        free: volumes.iter().filter_map(|volume| volume.free()).min(),
        unwritable: volumes.contains(&Volume::Unwritable),
    }
}

/// The device the data root lives on, while it exists.
#[cfg(unix)]
pub(super) fn root_device(root: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt as _;
    std::fs::metadata(root).ok().map(|root| root.dev())
}

/// Windows needs no device: a pulled drive takes every parent with it, so
/// its read fails.
#[cfg(not(unix))]
pub(super) fn root_device(_root: &Path) -> Option<u64> {
    None
}

/// The data root is gone and its nearest parent is on another device than
/// the root was: the volume it lived on went away (a pulled drive), not the
/// folder (deleted to free space).
pub(super) fn volume_gone(root: &Path, device: Option<u64>) -> bool {
    let Some(device) = device else {
        return false;
    };
    !root.exists() && root_device(super::probe::nearest_existing(root)) != Some(device)
}

/// The accounts under `root` that have a database, and the largest one.
pub(super) fn data_use(root: &Path) -> DataUse {
    let databases: Vec<u64> = std::fs::read_dir(root.join("accounts"))
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path().join("tdlib"))
                .filter(|tdlib| has_database(tdlib))
                .map(|tdlib| database_size(&tdlib))
                .collect()
        })
        .unwrap_or_default();
    DataUse {
        accounts: u64::try_from(databases.len()).unwrap_or(u64::MAX),
        largest_database: databases.into_iter().max().unwrap_or(0),
    }
}

fn has_database(tdlib: &Path) -> bool {
    tdlib.join("db.sqlite").exists() || tdlib.join("td.binlog").exists()
}

fn database_size(tdlib: &Path) -> u64 {
    ["db.sqlite", "db.sqlite-wal"]
        .into_iter()
        .filter_map(|name| std::fs::metadata(tdlib.join(name)).ok())
        .map(|file| file.len())
        .fold(0, u64::saturating_add)
}
