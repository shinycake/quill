//! `media.files`: TDLib `file.id` → the latest `file` snapshot, bounded.
//!
//! Every `file` object TDLib sends (history pages, avatars, stickers,
//! thumbnails) used to stay for the whole session. The map now keeps at
//! most [`FILES_MAX`] entries: past that, the least recently used ones go
//! until [`FILES_KEEP`] are left, the way Telegram Desktop lets images it
//! has not drawn for a while go. "Used" means read through
//! [`FileMap::get`] (every row that shows a file reads it each render) or
//! written by an update. A file the caller holds (a transfer in flight,
//! a chat-list avatar, a notification sound) is never dropped. A dropped
//! file that is needed again is asked for again: TDLib answers from its
//! own cache.
//!
//! The map also counts what views derive from it, so they need not scan
//! it: [`FileMap::paths_revision`] moves when a usable local path appears,
//! changes or goes away, and [`FileMap::quiet_updates`] counts the
//! `updateFile`s that moved no path (download progress).
use crate::telegram::envelope::ParsedFile;
use std::cell::Cell;
use std::collections::HashMap;

/// Entries past which the least recently used files are dropped.
pub const FILES_MAX: usize = 8192;
/// Entries left after a drop.
pub const FILES_KEEP: usize = 6144;

#[derive(Debug, Clone)]
struct FileEntry {
    file: ParsedFile,
    /// `FileMap::clock` at the last read or write.
    used: Cell<u64>,
}

#[derive(Debug, Default, Clone)]
pub struct FileMap {
    entries: HashMap<i32, FileEntry>,
    /// Moves on every write; reads stamp the entry with it.
    clock: u64,
    paths_revision: u64,
    quiet_updates: u64,
    /// `clock` before which [`Self::evict`] does not scan again after a
    /// pass that could not free enough (held files fill the map).
    retry_at: u64,
}

/// Writes to wait after an eviction pass that fell short.
const EVICT_RETRY_AFTER: u64 = 512;

impl FileMap {
    /// The file, marking it used.
    pub fn get(&self, id: &i32) -> Option<&ParsedFile> {
        let entry = self.entries.get(id)?;
        entry.used.set(self.clock);
        Some(&entry.file)
    }

    pub fn contains_key(&self, id: &i32) -> bool {
        self.entries.contains_key(id)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every file, without marking any used.
    pub fn values(&self) -> impl Iterator<Item = &ParsedFile> {
        self.entries.values().map(|entry| &entry.file)
    }

    /// Every file with its id, without marking any used.
    pub fn iter(&self) -> impl Iterator<Item = (&i32, &ParsedFile)> {
        self.entries.iter().map(|(id, entry)| (id, &entry.file))
    }

    /// Store `file` under its id, replacing the snapshot before it.
    pub fn insert(&mut self, file: ParsedFile) {
        self.clock = self.clock.wrapping_add(1);
        let id = file.id.0;
        let before = self.entries.get(&id).map(|entry| &entry.file);
        if before.and_then(ParsedFile::usable_path) != file.usable_path() {
            self.paths_revision = self.paths_revision.wrapping_add(1);
        }
        self.entries.insert(
            id,
            FileEntry {
                file,
                used: Cell::new(self.clock),
            },
        );
    }

    /// Change the stored file `id` in place, if there is one.
    pub fn update(&mut self, id: i32, change: impl FnOnce(&mut ParsedFile)) {
        let Some(mut file) = self.entries.get(&id).map(|entry| entry.file.clone()) else {
            return;
        };
        change(&mut file);
        self.insert(file);
    }

    pub fn remove(&mut self, id: &i32) -> Option<ParsedFile> {
        self.entries.remove(id).map(|entry| entry.file)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Moves whenever a file's usable local path appears, changes or goes
    /// away by an update (not when a file is dropped from the map).
    pub fn paths_revision(&self) -> u64 {
        self.paths_revision
    }

    /// `updateFile`s that moved no usable path, so changed nothing but
    /// transfer progress.
    pub fn quiet_updates(&self) -> u64 {
        self.quiet_updates
    }

    pub(crate) fn note_quiet_update(&mut self) {
        self.quiet_updates = self.quiet_updates.wrapping_add(1);
    }

    /// Whether [`Self::evict`] would scan: past [`FILES_MAX`] and not
    /// waiting out a pass that fell short.
    pub(crate) fn evict_due(&self) -> bool {
        self.entries.len() > FILES_MAX && self.clock >= self.retry_at
    }

    /// Past [`FILES_MAX`], drop the least recently used files `held`
    /// does not keep until [`FILES_KEEP`] are left. Returns how many went.
    pub(crate) fn evict(&mut self, held: impl Fn(&ParsedFile) -> bool) -> usize {
        if !self.evict_due() {
            return 0;
        }
        let excess = self.entries.len().saturating_sub(FILES_KEEP);
        let mut candidates: Vec<(u64, i32)> = self
            .entries
            .iter()
            .filter(|(_, entry)| !held(&entry.file))
            .map(|(id, entry)| (entry.used.get(), *id))
            .collect();
        candidates.sort_unstable();
        let doomed: Vec<i32> = candidates
            .into_iter()
            .take(excess)
            .map(|(_, id)| id)
            .collect();
        for id in &doomed {
            self.entries.remove(id);
        }
        self.retry_at = if doomed.len() < excess {
            self.clock.saturating_add(EVICT_RETRY_AFTER)
        } else {
            0
        };
        doomed.len()
    }
}

impl FromIterator<ParsedFile> for FileMap {
    fn from_iter<I: IntoIterator<Item = ParsedFile>>(files: I) -> Self {
        let mut map = Self::default();
        for file in files {
            map.insert(file);
        }
        map
    }
}

#[cfg(test)]
#[path = "file_map_tests.rs"]
mod tests;
