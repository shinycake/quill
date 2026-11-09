//! macOS checks a library for malware the first time it is mapped after the
//! system has forgotten the file (a restart, or enough other file activity),
//! and the mapping waits for the whole file to be read: 7 to 9 ms per MB,
//! about 0.4 s for TDLib. Inside `dlopen` that wait holds the dynamic
//! loader's lock, which the main thread needs while it builds the window.
//!
//! [`settle`] asks for the same check before `dlopen` and outside that lock,
//! so the load that follows is quick. Nothing is skipped: the system runs the
//! check it would have run, on the thread that was going to wait for it.
//! Measurements: docs/decisions/launch-first-load-check.md.
#![deny(clippy::indexing_slicing)]

use std::path::Path;

/// Have the system finish its first-load check of `library` and of the
/// libraries beside it that it links to. Call it right before loading
/// `library`, on the thread that loads it. Does nothing off macOS.
pub fn settle(library: &Path) {
    #[cfg(target_os = "macos")]
    macos::settle_tree(library, &mut Vec::new());
    #[cfg(not(target_os = "macos"))]
    let _ = library;
}

#[cfg(target_os = "macos")]
mod macos {
    use object::Endianness;
    use object::macho::MachHeader64;
    use object::read::macho::MachHeader;
    use std::ffi::OsStr;
    use std::fs::File;
    use std::io::Read;
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    use std::path::{Path, PathBuf};

    /// The header and load commands of every library Quill bundles fit in
    /// this many bytes (TDLib's take 2 KB).
    const HEAD: u64 = 64 * 1024;
    /// More libraries than any bundle has: a link cycle ends at `done`, and
    /// this bounds a directory that is not Quill's.
    const MOST: usize = 64;

    /// `library` after the libraries it links to, each once.
    pub(super) fn settle_tree(library: &Path, done: &mut Vec<PathBuf>) {
        if done.len() >= MOST || done.iter().any(|path| path == library) {
            return;
        }
        done.push(library.to_path_buf());
        let Ok(file) = File::open(library) else {
            return;
        };
        let mut head = Vec::new();
        if (&file).take(HEAD).read_to_end(&mut head).is_ok()
            && let Some(directory) = library.parent()
        {
            for name in linked_names(&head) {
                let beside = directory.join(OsStr::from_bytes(name));
                if beside.is_file() {
                    settle_tree(&beside, done);
                }
            }
        }
        request_check(&file);
    }

    /// The file names of the libraries a thin 64-bit Mach-O links to, from
    /// the start of the file. Empty for anything else: its own check still
    /// runs, and the loader checks what it links to as before.
    pub(super) fn linked_names(head: &[u8]) -> Vec<&[u8]> {
        let mut names = Vec::new();
        let Ok(header) = MachHeader64::<Endianness>::parse(head, 0) else {
            return names;
        };
        let Ok(endian) = header.endian() else {
            return names;
        };
        let Ok(mut commands) = header.load_commands(endian, head, 0) else {
            return names;
        };
        while let Ok(Some(command)) = commands.next() {
            let Ok(Some(dylib)) = command.dylib() else {
                continue;
            };
            let Ok(install_name) = command.string(endian, dylib.dylib.name) else {
                continue;
            };
            if let Some(name) = install_name.rsplit(|byte| *byte == b'/').next()
                && !name.is_empty()
            {
                names.push(name);
            }
        }
        names
    }

    /// Map the file as code and drop the mapping. The system checks a file
    /// when it is first mapped executable, before it decides whether the
    /// mapping is allowed, and remembers the verdict for the file.
    fn request_check(file: &File) {
        let Some(length) = file
            .metadata()
            .ok()
            .and_then(|meta| usize::try_from(meta.len()).ok())
            .filter(|length| *length > 0)
        else {
            return;
        };
        // SAFETY: a private mapping of an open regular file at an address
        // the kernel picks. Nothing reads or runs it; a refusal (the kernel
        // wants the signature attached first) returns MAP_FAILED.
        let mapped = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                length,
                libc::PROT_READ | libc::PROT_EXEC,
                libc::MAP_PRIVATE,
                file.as_raw_fd(),
                0,
            )
        };
        if mapped != libc::MAP_FAILED {
            // SAFETY: `mapped` is the mapping of `length` bytes made above,
            // and no reference into it exists.
            unsafe { libc::munmap(mapped, length) };
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn names_the_libraries_a_mach_o_links_to() {
            let exe = std::env::current_exe().expect("test executable");
            let mut head = Vec::new();
            File::open(exe)
                .expect("open")
                .take(HEAD)
                .read_to_end(&mut head)
                .expect("read");
            let names = linked_names(&head);
            assert!(
                names.contains(&&b"libSystem.B.dylib"[..]),
                "{:?}",
                names
                    .iter()
                    .map(|name| String::from_utf8_lossy(name))
                    .collect::<Vec<_>>()
            );
        }

        #[test]
        fn a_file_that_is_not_a_library_links_to_nothing() {
            assert!(linked_names(b"not a Mach-O file").is_empty());
            assert!(linked_names(&[]).is_empty());
        }

        #[test]
        fn settling_a_missing_or_plain_file_is_harmless() {
            let folder =
                std::env::temp_dir().join(format!("quill-first-load-{}", std::process::id()));
            std::fs::create_dir_all(&folder).expect("folder");
            settle_tree(&folder.join("missing.dylib"), &mut Vec::new());
            let plain = folder.join("plain.dylib");
            std::fs::write(&plain, b"text").expect("write");
            let mut done = Vec::new();
            settle_tree(&plain, &mut done);
            assert_eq!(done, vec![plain]);
            let _ = std::fs::remove_dir_all(&folder);
        }
    }
}
