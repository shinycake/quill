//! Copying a downloaded file into the export folder so that a stop lands
//! within one chunk and nothing half written sits under the final name.
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

const CHUNK: usize = 1 << 20;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Copied {
    Done,
    Stopped,
}

/// Copy `from` to `to`. A volume that clones (APFS, btrfs) shares the
/// blocks at once. Anywhere else the bytes go through `<to>.part`, `stopped`
/// is asked between chunks, and the part takes the final name only once
/// complete.
pub(crate) fn copy_stoppable(
    from: &Path,
    to: &Path,
    stopped: &dyn Fn() -> bool,
) -> io::Result<Copied> {
    if stopped() {
        return Ok(Copied::Stopped);
    }
    let part = part_path(to);
    let copied = if clone(from, &part) {
        Ok(Copied::Done)
    } else {
        copy_chunks(from, &part, stopped)
    };
    settle(&part, to, copied)
}

/// Give a complete part its final name, and remove any other.
fn settle(part: &Path, to: &Path, copied: io::Result<Copied>) -> io::Result<Copied> {
    match copied {
        Ok(Copied::Done) => fs::rename(part, to)
            .inspect_err(|_| {
                let _ = fs::remove_file(part);
            })
            .map(|()| Copied::Done),
        other => {
            let _ = fs::remove_file(part);
            other
        }
    }
}

fn part_path(to: &Path) -> PathBuf {
    let mut name = to.as_os_str().to_os_string();
    name.push(".part");
    PathBuf::from(name)
}

fn copy_chunks(from: &Path, to: &Path, stopped: &dyn Fn() -> bool) -> io::Result<Copied> {
    let mut source = File::open(from)?;
    let mut target = File::create(to)?;
    let mut buffer = vec![0; CHUNK];
    loop {
        let read = source.read(&mut buffer)?;
        if read == 0 {
            return Ok(Copied::Done);
        }
        target.write_all(buffer.get(..read).unwrap_or_default())?;
        if stopped() {
            return Ok(Copied::Stopped);
        }
    }
}

#[cfg(target_os = "macos")]
fn clone(from: &Path, to: &Path) -> bool {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let _ = fs::remove_file(to);
    let (Ok(from), Ok(to)) = (
        CString::new(from.as_os_str().as_bytes()),
        CString::new(to.as_os_str().as_bytes()),
    ) else {
        return false;
    };
    // SAFETY: both are NUL-terminated paths that outlive the call.
    unsafe { libc::clonefile(from.as_ptr(), to.as_ptr(), 0) == 0 }
}

#[cfg(target_os = "linux")]
fn clone(from: &Path, to: &Path) -> bool {
    use std::os::fd::AsRawFd;
    let (Ok(source), Ok(target)) = (File::open(from), File::create(to)) else {
        return false;
    };
    // SAFETY: FICLONE reads the source descriptor; both files stay open
    // across the call.
    unsafe { libc::ioctl(target.as_raw_fd(), libc::FICLONE, source.as_raw_fd()) == 0 }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn clone(_: &Path, _: &Path) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stop_mid_copy_lands_within_one_chunk_and_leaves_no_part() {
        let dir = crate::scratch::unique_dir("quill-export-copy");
        let from = dir.join("video.mp4");
        fs::write(&from, vec![7u8; 3 * CHUNK + 1]).unwrap();
        let part = dir.join("copy.mp4.part");

        let copied = copy_chunks(&from, &part, &|| true);

        assert_eq!(fs::metadata(&part).unwrap().len(), CHUNK as u64);
        let settled = settle(&part, &dir.join("copy.mp4"), copied).unwrap();
        assert_eq!(settled, Copied::Stopped);
        assert!(!part.exists());
        assert!(!dir.join("copy.mp4").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_copy_that_runs_to_the_end_takes_the_final_name() {
        let dir = crate::scratch::unique_dir("quill-export-copy-done");
        let from = dir.join("video.mp4");
        let bytes: Vec<u8> = (0..2 * CHUNK + 5).map(|at| (at % 251) as u8).collect();
        fs::write(&from, &bytes).unwrap();
        let to = dir.join("copy.mp4");

        assert_eq!(copy_stoppable(&from, &to, &|| false).unwrap(), Copied::Done);

        assert_eq!(fs::read(&to).unwrap(), bytes);
        assert!(!dir.join("copy.mp4.part").exists());
        fs::write(&to, b"edited").unwrap();
        assert_eq!(fs::read(&from).unwrap(), bytes, "the copy stands apart");
        let _ = fs::remove_dir_all(&dir);
    }
}
