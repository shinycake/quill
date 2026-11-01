//! The registry's per-file download index agrees with its pending map.
use super::*;
use std::time::{Duration, Instant};

const FILES: i32 = 8;

fn scan(registry: &RequestRegistry, file_id: i32) -> bool {
    registry
        .pending
        .values()
        .any(|p| p.purpose == RequestPurpose::DownloadFile && p.file_id == Some(file_id))
}

fn assert_matches_scan(registry: &RequestRegistry) {
    for file_id in 0..FILES {
        assert_eq!(
            registry.has_download(FileId(file_id)),
            scan(registry, file_id),
            "file {file_id}"
        );
    }
}

#[test]
fn download_index_follows_register_take_sweep_and_settle_all() {
    let generation = AccountGeneration(1);
    let mut registry = RequestRegistry::default();
    let first = registry.register_download(generation, FileId(3));
    let second = registry.register_download(generation, FileId(3));
    registry.register(generation, RequestPurpose::GetHistory, None, None);
    assert!(registry.has_download(FileId(3)));
    registry.take(first);
    assert!(registry.has_download(FileId(3)), "one request is still out");
    registry.take(second);
    assert!(!registry.has_download(FileId(3)));
    registry.register_download(generation, FileId(5));
    let swept = registry.sweep_stale(
        Instant::now() + Duration::from_secs(3600),
        Duration::ZERO,
        |_| false,
    );
    assert_eq!(swept.len(), 1, "only the history lookup is swept");
    assert!(registry.has_download(FileId(5)));
    registry.drain_all();
    assert!(!registry.has_download(FileId(5)));
    assert_matches_scan(&registry);
}

#[test]
fn download_index_matches_a_scan_over_a_seeded_random_sequence() {
    let generation = AccountGeneration(1);
    let mut registry = RequestRegistry::default();
    let mut ids: Vec<RequestId> = Vec::new();
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for _ in 0..5_000 {
        let roll = next();
        match roll % 10 {
            0..=3 => ids.push(
                registry.register_download(generation, FileId((next() % FILES as u64) as i32)),
            ),
            4 => ids.push(registry.register(generation, RequestPurpose::GetHistory, None, None)),
            5 => {
                ids.push(registry.register(generation, RequestPurpose::SetPhoneNumber, None, None))
            }
            6..=7 if !ids.is_empty() => {
                let id = ids.swap_remove((next() % ids.len() as u64) as usize);
                registry.take(id);
            }
            8 => {
                registry.take_purpose(RequestPurpose::DownloadFile);
            }
            9 if roll % 7 == 0 => {
                registry.sweep_stale(
                    Instant::now() + Duration::from_secs(3600),
                    Duration::ZERO,
                    |_| false,
                );
            }
            9 if roll % 11 == 0 => registry.invalidate_auth(),
            9 if roll % 13 == 0 => {
                registry.drain_all();
            }
            _ => {}
        }
        assert_matches_scan(&registry);
    }
}
