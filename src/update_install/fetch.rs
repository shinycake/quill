//! A limit on the whole download fails a slow link at the same point every
//! time, so only a stall ends an attempt, and what arrived stays for the next
//! one (tdesktop `update_checker.cpp` asks for `Range: bytes=<already>-`).
#![deny(clippy::indexing_slicing)]

use super::{swap, trusted_file};
use crate::updater::DownloadProgress;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

const STALL_LIMIT: Duration = Duration::from_secs(60);
const CONNECT_LIMIT: Duration = Duration::from_secs(30);

const PARTIAL_PREFIX: &str = ".quill-partial-";

const FAILED: &str = "Download failed. Check your connection and retry.";
const INTERRUPTED: &str = "Download interrupted or disk full. Retry the update.";
const NOT_SAVED: &str = "Could not save the update. Check disk space and retry.";
const NO_FILE: &str =
    "Could not stage update beside the executable. Check folder permissions and retry.";
pub(super) const MISMATCH: &str =
    "Update checksum or size mismatch. Nothing was installed. Retry the release check.";

pub(super) fn partial_path(stage_root: &Path, sha256: &str) -> PathBuf {
    stage_root.join(format!(
        "{PARTIAL_PREFIX}{}.archive",
        sha256.to_ascii_lowercase()
    ))
}

pub(super) fn clear_other_partials(stage_root: &Path, keep: &Path) {
    let Ok(entries) = fs::read_dir(stage_root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with(PARTIAL_PREFIX) && entry.path() != keep {
            let _ = swap::remove_any(&entry.path());
        }
    }
}

pub(super) fn agent(https_only: bool, redirects: u32) -> ureq::Agent {
    ureq::Agent::config_builder()
        .https_only(https_only)
        .max_redirects(redirects)
        .timeout_resolve(Some(CONNECT_LIMIT))
        .timeout_connect(Some(CONNECT_LIMIT))
        .timeout_per_read(Some(STALL_LIMIT))
        .build()
        .new_agent()
}

enum Answer {
    Continues,
    Whole,
    WrongRange,
}

fn read_answer(status: u16, content_range: Option<&str>, have: u64) -> Answer {
    if status != 206 || have == 0 {
        return Answer::Whole;
    }
    let start = content_range
        .and_then(|range| range.strip_prefix("bytes "))
        .and_then(|range| range.split('-').next())
        .and_then(|start| start.parse::<u64>().ok());
    if start == Some(have) {
        Answer::Continues
    } else {
        Answer::WrongRange
    }
}

fn kept_bytes(partial: &Path, size: u64) -> u64 {
    let Ok(meta) = fs::symlink_metadata(partial) else {
        return 0;
    };
    if trusted_file(partial) && meta.len() <= size {
        return meta.len();
    }
    let _ = swap::remove_any(partial);
    0
}

fn open_partial(path: &Path, append: bool) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).write(true);
    if append {
        options.append(true);
    } else {
        options.truncate(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    options.open(path)
}

pub(super) fn fetch(
    agent: &ureq::Agent,
    url: &str,
    partial: &Path,
    size: u64,
    progress: &DownloadProgress,
) -> Result<(), &'static str> {
    let mut have = kept_bytes(partial, size);
    progress.set(have);
    if have == size {
        return Ok(());
    }
    let mut response = loop {
        let mut request = agent.get(url).header("User-Agent", "Quill-updater");
        if have > 0 {
            request = request.header("Range", format!("bytes={have}-"));
        }
        let response = match request.call() {
            Ok(response) => response,
            Err(ureq::Error::StatusCode(416)) if have > 0 => {
                have = 0;
                continue;
            }
            Err(_) => return Err(FAILED),
        };
        let range = response.headers().get("content-range");
        let range = range.and_then(|value| value.to_str().ok());
        match read_answer(response.status().as_u16(), range, have) {
            Answer::Continues => break response,
            Answer::Whole => {
                have = 0;
                break response;
            }
            Answer::WrongRange => have = 0,
        }
    };
    progress.set(have);
    let mut file = open_partial(partial, have > 0).map_err(|_| NO_FILE)?;
    let mut reader = Counted {
        inner: response
            .body_mut()
            .as_reader()
            .take(size.saturating_sub(have).saturating_add(1)),
        progress,
    };
    let copied = io::copy(&mut reader, &mut file);
    let saved = file.sync_all();
    let copied = copied.map_err(|_| INTERRUPTED)?;
    saved.map_err(|_| NOT_SAVED)?;
    match (have + copied).cmp(&size) {
        std::cmp::Ordering::Equal => Ok(()),
        std::cmp::Ordering::Less => Err(INTERRUPTED),
        std::cmp::Ordering::Greater => {
            let _ = fs::remove_file(partial);
            Err(MISMATCH)
        }
    }
}

struct Counted<'a, R> {
    inner: R,
    progress: &'a DownloadProgress,
}

impl<R: Read> Read for Counted<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let read = self.inner.read(buf)?;
        self.progress.add(read as u64);
        Ok(read)
    }
}

#[cfg(test)]
#[path = "fetch_tests.rs"]
mod tests;
