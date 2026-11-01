//! Downloads an export made only for itself, listed on disk next to the
//! TDLib database until their cache copy is deleted or something else keeps
//! it. A delete that a quit, a crash or a held delete cut off then goes out
//! on the next start.
//!
//! TDLib file ids hold for one run only, so an entry keeps the file's
//! remote id. Each entry is a file named `<run>-<file id>`: workers of one
//! run never share a name, and an entry left by an earlier run is told
//! apart by its run.
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

    /// Entries from earlier runs. One that cannot be read is dropped.
    pub fn earlier(&self) -> Vec<Earlier> {
        let Ok(names) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mine = format!("{}-", self.run);
        names
            .flatten()
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
}
