//! The folders TDLib writes to, each read on the volume it really lives
//! on: an `accounts`, `tdlib` or `files` folder can be a symlink to another
//! disk, and the tightest volume is the one that fills first.
#![deny(clippy::indexing_slicing)]

use super::DataUse;
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

/// The least free space over `folders`, read by `read`.
pub(super) fn tightest(folders: &[PathBuf], read: impl Fn(&Path) -> Option<u64>) -> Option<u64> {
    folders.iter().filter_map(|folder| read(folder)).min()
}

/// The accounts under `root` that have a database.
pub(super) fn data_use(root: &Path) -> DataUse {
    let accounts = std::fs::read_dir(root.join("accounts"))
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| has_database(&entry.path().join("tdlib")))
                .count()
        })
        .unwrap_or(0);
    DataUse {
        accounts: u64::try_from(accounts).unwrap_or(u64::MAX),
    }
}

fn has_database(tdlib: &Path) -> bool {
    tdlib.join("db.sqlite").exists() || tdlib.join("td.binlog").exists()
}
