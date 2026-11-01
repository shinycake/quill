//! The files one export already saved, by TDLib file id, as tdesktop's
//! `ApiWrap::LoadedFileCache`: a file that occurs again (a sticker sent 200
//! times, a forwarded video) links to the first copy instead of being
//! downloaded and copied again.
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};

/// Bounds the map on a huge export; an evicted file is only copied again.
const LIMIT: usize = 100_000;

#[derive(Debug, Default)]
pub struct LoadedFiles(RefCell<Cache>);

#[derive(Debug, Default)]
struct Cache {
    paths: HashMap<i32, String>,
    order: VecDeque<i32>,
}

impl LoadedFiles {
    /// Where the file was saved, relative to the export root.
    pub fn find(&self, file_id: i32) -> Option<String> {
        self.0.borrow().paths.get(&file_id).cloned()
    }

    pub fn save(&self, file_id: i32, root_relative: String) {
        let mut cache = self.0.borrow_mut();
        if cache.paths.insert(file_id, root_relative).is_none() {
            cache.order.push_back(file_id);
        }
        while cache.order.len() > LIMIT {
            if let Some(old) = cache.order.pop_front() {
                cache.paths.remove(&old);
            }
        }
    }
}
