use super::*;
use crate::ids::FileId;
use crate::telegram::envelope::LocalFileState;

fn file(id: i32, path: &str) -> ParsedFile {
    ParsedFile {
        id: FileId(id),
        size: 10,
        expected_size: 10,
        local: LocalFileState::from_parts(path, true, false, !path.is_empty(), 0),
    }
}

#[test]
fn a_path_moves_the_paths_revision_and_progress_does_not() {
    let mut files = FileMap::default();
    files.insert(file(1, ""));
    let start = files.paths_revision();
    let mut progress = file(1, "");
    progress.local = LocalFileState::from_parts("", true, true, false, 5);
    files.insert(progress);
    assert_eq!(files.paths_revision(), start, "progress moves no path");
    files.insert(file(1, "/tmp/1.jpg"));
    assert_eq!(files.paths_revision(), start + 1, "the download finished");
    files.insert(file(1, "/tmp/1.jpg"));
    assert_eq!(files.paths_revision(), start + 1);
    files.insert(file(1, ""));
    assert_eq!(files.paths_revision(), start + 2, "the file left the cache");
}

#[test]
fn past_the_cap_the_least_recently_used_unheld_files_go() {
    let mut files = FileMap::default();
    let total = FILES_MAX as i32 + 1;
    for id in 0..total {
        files.insert(file(id, "/tmp/f"));
        if id == 100 {
            // Read early on: shown, so used after files 101..=4000.
            assert!(files.get(&1).is_some());
        }
        if id == 4000 {
            assert!(files.get(&1).is_some());
        }
    }
    let gone = files.evict(|file| file.id.0 == 0);
    assert_eq!(gone, total as usize - FILES_KEEP);
    assert_eq!(files.len(), FILES_KEEP);
    assert!(files.contains_key(&0), "a held file stays");
    assert!(files.contains_key(&1), "a recently read file stays");
    assert!(!files.contains_key(&2), "an old unread file goes");
    assert!(files.contains_key(&(total - 1)), "the newest file stays");
}

#[test]
fn under_the_cap_nothing_goes() {
    let mut files: FileMap = (0..100).map(|id| file(id, "")).collect();
    assert_eq!(files.evict(|_| false), 0);
    assert_eq!(files.len(), 100);
}

#[test]
fn a_pass_that_frees_too_little_waits_for_more_writes() {
    let mut files = FileMap::default();
    for id in 0..=FILES_MAX as i32 {
        files.insert(file(id, "/tmp/f"));
    }
    assert_eq!(files.evict(|_| true), 0, "everything is held");
    // Nothing is held now, but the last pass found nothing to drop.
    assert_eq!(files.evict(|_| false), 0, "no rescan right away");
    for id in 0..super::EVICT_RETRY_AFTER as i32 {
        files.insert(file(100_000 + id, "/tmp/f"));
    }
    assert!(files.evict(|_| false) > 0, "tried again after more writes");
}

#[test]
fn a_pass_with_every_file_held_is_not_due_again_until_enough_writes() {
    let mut files = FileMap::default();
    for id in 0..=FILES_MAX as i32 {
        files.insert(file(id, "/tmp/f"));
    }
    assert!(files.evict_due());
    assert_eq!(files.evict(|_| true), 0, "everything is held");
    for id in 0..super::EVICT_RETRY_AFTER as i32 {
        assert!(!files.evict_due(), "write {id}: the caller builds nothing");
        files.insert(file(100_000 + id, "/tmp/f"));
    }
    assert!(files.evict_due());
}

#[test]
fn a_subset_holds_only_the_asked_files() {
    let mut files = FileMap::default();
    for id in 1..=4 {
        files.insert(file(id, "/tmp/f"));
    }
    let subset = files.only([2, 4, 9]);
    assert_eq!(subset.len(), 2, "9 is not in the map");
    assert!(subset.contains_key(&2) && subset.contains_key(&4));
    assert!(!subset.contains_key(&1));
}

#[test]
fn a_download_subset_keeps_the_intents_of_the_asked_files() {
    use crate::state::{DownloadIntents, DownloadOrigin};
    let mut downloads = DownloadIntents::default();
    downloads.begin(1, DownloadOrigin::User);
    downloads.begin(2, DownloadOrigin::User);
    let files = FileMap::default();
    let subset = downloads.only(&[2, 3]);
    assert!(subset.is_downloading(2, &files) || subset.is_requested(2));
    assert!(!subset.is_requested(1), "file 1 was not asked for");
    assert_eq!(subset.requested_count(), 1);
}
