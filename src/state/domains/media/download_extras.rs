//! The `downloadFile` requests of each file still waiting for the file to
//! settle, both ways round: an error names a request, a finished file
//! names its id.
use std::collections::HashMap;

#[derive(Debug, Default)]
pub(crate) struct DownloadExtras {
    files: HashMap<u64, i32>,
    extras: HashMap<i32, Vec<u64>>,
}

impl DownloadExtras {
    pub(crate) fn insert(&mut self, extra: u64, file_id: i32) {
        if let Some(old) = self.files.insert(extra, file_id) {
            self.drop_extra(old, extra);
        }
        self.extras.entry(file_id).or_default().push(extra);
    }

    /// The file a request downloads.
    pub(crate) fn get(&self, extra: &u64) -> Option<&i32> {
        self.files.get(extra)
    }

    pub(crate) fn has_file(&self, file_id: i32) -> bool {
        self.extras.contains_key(&file_id)
    }

    /// Forget every request of `file_id`.
    pub(crate) fn remove_file(&mut self, file_id: i32) {
        for extra in self.extras.remove(&file_id).unwrap_or_default() {
            self.files.remove(&extra);
        }
    }

    pub(crate) fn clear(&mut self) {
        self.files.clear();
        self.extras.clear();
    }

    fn drop_extra(&mut self, file_id: i32, extra: u64) {
        if let Some(extras) = self.extras.get_mut(&file_id) {
            extras.retain(|e| *e != extra);
            if extras.is_empty() {
                self.extras.remove(&file_id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removing_a_file_forgets_its_requests_and_keeps_the_others() {
        let mut extras = DownloadExtras::default();
        extras.insert(1, 10);
        extras.insert(2, 10);
        extras.insert(3, 20);
        extras.remove_file(10);
        assert_eq!(extras.get(&1).copied(), None);
        assert_eq!(extras.get(&2).copied(), None);
        assert!(!extras.has_file(10));
        assert_eq!(extras.get(&3).copied(), Some(20));
        assert!(extras.has_file(20));
    }

    #[test]
    fn a_reused_request_id_moves_to_its_new_file() {
        let mut extras = DownloadExtras::default();
        extras.insert(1, 10);
        extras.insert(1, 20);
        assert!(!extras.has_file(10));
        assert_eq!(extras.get(&1).copied(), Some(20));
    }
}
