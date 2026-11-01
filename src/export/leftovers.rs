//! Downloads an export made only for itself, listed on disk next to the
//! TDLib database until their cache copy is deleted or something else keeps
//! it. A delete that a quit, a crash or a held delete cut off then goes out
//! on the next start.
//!
//! TDLib file ids hold for one run only, so an entry keeps the file's
//! remote id. Each entry is a file named `<run>-<file id>`: workers of one
//! run never share a name, and an entry left by an earlier run is told
//! apart by its run.
//!
//! The `parts` folder lists the `.part` files copies into an export folder
//! are writing (`export::copy`), so one a kill or a power loss cut off is
//! removed on the next start.
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Leftovers {
    dir: PathBuf,
    run: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub remote_id: String,
    /// The downloaded copy, once the download ended.
    pub path: Option<PathBuf>,
}

/// An entry an earlier run left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Earlier {
    pub name: String,
    pub entry: Entry,
}

impl Leftovers {
    pub fn new(dir: PathBuf, run: u64) -> Self {
        Self { dir, run }
    }

    pub fn name(&self, file_id: i32) -> String {
        format!("{}-{file_id}", self.run)
    }

    pub fn record(&self, file_id: i32, entry: &Entry) {
        let written = fs::create_dir_all(&self.dir).and_then(|()| {
            let text = serde_json::to_vec(entry)?;
            fs::write(self.dir.join(self.name(file_id)), text)
        });
        if let Err(err) = written {
            log::warn!("export: could not list file {file_id} for cleanup: {err}");
        }
    }

    pub fn forget(&self, name: &str) {
        let _ = fs::remove_file(self.dir.join(name));
    }

    /// Lists `part` while a copy writes it; the entry goes on drop.
    pub(crate) fn copying(&self, part: &Path) -> Copying {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let entry = self.dir.join(PARTS).join(format!("{}-{serial}", self.run));
        let written = fs::create_dir_all(self.dir.join(PARTS))
            .and_then(|()| fs::write(&entry, part.as_os_str().as_encoded_bytes()));
        if let Err(err) = written {
            log::warn!("export: could not list {} for cleanup: {err}", part.display());
        }
        Copying(entry)
    }

    /// Removes the `.part` files earlier runs left, and their entries.
    pub fn sweep_parts(&self) {
        let Ok(names) = fs::read_dir(self.dir.join(PARTS)) else {
            return;
        };
        let mine = format!("{}-", self.run);
        for item in names.flatten() {
            if item.file_name().to_string_lossy().starts_with(&mine) {
                continue;
            }
            if let Some(part) = fs::read(item.path()).ok().and_then(part_path) {
                let _ = fs::remove_file(part);
            }
            let _ = fs::remove_file(item.path());
        }
    }

    /// Entries from earlier runs. One that cannot be read is dropped.
    pub fn earlier(&self) -> Vec<Earlier> {
        let Ok(names) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mine = format!("{}-", self.run);
        names
            .flatten()
            .filter(|item| item.file_type().is_ok_and(|kind| kind.is_file()))
            .filter_map(|item| item.file_name().into_string().ok())
            .filter(|name| !name.starts_with(&mine))
            .filter_map(|name| match read(&self.dir.join(&name)) {
                Some(entry) => Some(Earlier { name, entry }),
                None => {
                    self.forget(&name);
                    None
                }
            })
            .collect()
    }
}

const PARTS: &str = "parts";

/// A listed `.part` file; the entry goes when the copy is done with it.
pub(crate) struct Copying(PathBuf);

impl Drop for Copying {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// The path a parts entry names, when it is a `.part` file: nothing else
/// is removed on the word of a file on disk.
fn part_path(bytes: Vec<u8>) -> Option<PathBuf> {
    let path = PathBuf::from(String::from_utf8(bytes).ok()?);
    (path.is_absolute() && path.extension().is_some_and(|ext| ext == "part")).then_some(path)
}

fn read(path: &Path) -> Option<Entry> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_outlives_its_run_until_forgotten() {
        let dir = crate::scratch::unique_dir("quill-export-leftovers");
        let first = Leftovers::new(dir.clone(), 1);
        let entry = Entry {
            remote_id: "AgAD".into(),
            path: Some(dir.join("photo.jpg")),
        };
        first.record(5, &entry);
        first.record(6, &entry);
        first.forget(&first.name(6));
        std::fs::write(dir.join("torn"), b"{\"remote").unwrap();

        let next = Leftovers::new(dir.clone(), 2);
        next.record(5, &entry);
        let earlier = next.earlier();

        assert_eq!(
            earlier,
            [Earlier {
                name: "1-5".into(),
                entry
            }]
        );
        assert!(!dir.join("torn").exists());
        next.forget(&earlier[0].name);
        assert!(next.earlier().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Quill is killed while it copies a video into the export folder: the
    /// next start removes the half-written `.part`, and leaves the part a
    /// copy of this run is writing.
    #[test]
    fn a_part_file_a_crash_left_in_the_export_folder_goes_on_the_next_start() {
        let dir = crate::scratch::unique_dir("quill-export-parts");
        let journal = dir.join("export-leftovers");
        let folder = dir.join("ChatExport");
        std::fs::create_dir_all(&folder).unwrap();
        let cut_off = folder.join("video.mp4.part");
        std::fs::write(&cut_off, b"half").unwrap();
        std::mem::forget(Leftovers::new(journal.clone(), 1).copying(&cut_off));

        let next = Leftovers::new(journal.clone(), 2);
        let writing = folder.join("other.mp4.part");
        std::fs::write(&writing, b"now").unwrap();
        let copying = next.copying(&writing);
        next.sweep_parts();

        assert!(!cut_off.exists());
        assert!(writing.exists(), "this run's copy is still writing");
        assert!(next.earlier().is_empty(), "the parts folder is no entry");
        drop(copying);
        assert_eq!(std::fs::read_dir(journal.join(PARTS)).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
