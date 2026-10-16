//! Safe extraction of the release package (Linux tar.gz, Windows zip) into a
//! staging directory. The archive is already checksum-verified; this still
//! refuses anything that could write outside the staging root.
#![deny(clippy::indexing_slicing)]
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

const MAX_ENTRIES: usize = 20_000;
const MAX_UNPACKED_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ArchiveKind {
    TarGz,
    Zip,
}

fn bad(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_string())
}

/// `quill-linux-x86_64/lib/x` becomes `lib/x`; every archive entry must share
/// the one top-level directory, and only plain names are accepted (no `..`,
/// roots, drive prefixes). Returns `None` for the top-level directory itself.
fn strip_root(path: &Path, root: &mut Option<OsString>) -> io::Result<Option<PathBuf>> {
    let mut parts = path.components().peekable();
    while matches!(parts.peek(), Some(Component::CurDir)) {
        parts.next();
    }
    let first = match parts.next() {
        Some(Component::Normal(name)) => name.to_os_string(),
        _ => return Err(bad("Unsafe path in update package")),
    };
    match root {
        Some(existing) if *existing != first => {
            return Err(bad("Update package has more than one top-level entry"));
        }
        Some(_) => (),
        None => *root = Some(first),
    }
    let mut out = PathBuf::new();
    for part in parts {
        match part {
            Component::Normal(name) => out.push(name),
            Component::CurDir => (),
            _ => return Err(bad("Unsafe path in update package")),
        }
    }
    Ok((!out.as_os_str().is_empty()).then_some(out))
}

struct Budget {
    entries: usize,
    bytes: u64,
}

impl Budget {
    fn entry(&mut self) -> io::Result<()> {
        self.entries += 1;
        if self.entries > MAX_ENTRIES {
            return Err(bad("Update package has too many files"));
        }
        Ok(())
    }
}

fn write_file(
    dest: &Path,
    reader: &mut dyn Read,
    mode: u32,
    budget: &mut Budget,
) -> io::Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Owner-writable always; executable only if the package said so.
        options.mode((mode & 0o755) | 0o600);
    }
    #[cfg(not(unix))]
    let _ = mode;
    let mut file = options.open(dest)?;
    let allowed = MAX_UNPACKED_BYTES.saturating_sub(budget.bytes);
    let copied = io::copy(&mut reader.take(allowed.saturating_add(1)), &mut file)?;
    budget.bytes = budget.bytes.saturating_add(copied);
    if budget.bytes > MAX_UNPACKED_BYTES {
        return Err(bad("Update package is larger than allowed"));
    }
    file.sync_all()
}

/// Extracts `archive` into `dest` (created; must not exist) with the single
/// top-level directory stripped.
pub fn extract(archive: &Path, kind: ArchiveKind, dest: &Path) -> io::Result<()> {
    fs::create_dir(dest)?;
    let mut root = None;
    let mut budget = Budget {
        entries: 0,
        bytes: 0,
    };
    match kind {
        ArchiveKind::TarGz => extract_tar(archive, dest, &mut root, &mut budget)?,
        ArchiveKind::Zip => extract_zip(archive, dest, &mut root, &mut budget)?,
    }
    if root.is_none() {
        return Err(bad("Update package is empty"));
    }
    Ok(())
}

fn extract_tar(
    archive: &Path,
    dest: &Path,
    root: &mut Option<OsString>,
    budget: &mut Budget,
) -> io::Result<()> {
    let decoder = flate2::read::GzDecoder::new(File::open(archive)?);
    let mut tar = tar::Archive::new(decoder);
    for entry in tar.entries()? {
        let mut entry = entry?;
        budget.entry()?;
        let path = entry.path()?.into_owned();
        let kind = entry.header().entry_type();
        let Some(relative) = strip_root(&path, root)? else {
            continue;
        };
        let target = dest.join(&relative);
        if kind.is_dir() {
            fs::create_dir_all(&target)?;
        } else if kind.is_file() {
            let mode = entry.header().mode().unwrap_or(0o644);
            write_file(&target, &mut entry, mode, budget)?;
        } else {
            return Err(bad("Update package contains a link or special file"));
        }
    }
    Ok(())
}

fn extract_zip(
    archive: &Path,
    dest: &Path,
    root: &mut Option<OsString>,
    budget: &mut Budget,
) -> io::Result<()> {
    let mut zip = zip::ZipArchive::new(File::open(archive)?).map_err(io::Error::other)?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(io::Error::other)?;
        budget.entry()?;
        if entry.is_symlink() {
            return Err(bad("Update package contains a link or special file"));
        }
        // Compress-Archive may write backslash separators.
        let name = entry.name().map_err(io::Error::other)?.replace('\\', "/");
        let Some(relative) = strip_root(Path::new(&name), root)? else {
            continue;
        };
        let target = dest.join(&relative);
        if entry.is_dir() {
            fs::create_dir_all(&target)?;
        } else {
            let mode = entry.unix_mode().unwrap_or(0o644);
            write_file(&target, &mut entry, mode, budget)?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::{ArchiveKind, extract};
    use std::fs;
    use std::io::Write;
    use std::path::{Path, PathBuf};

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("quill-bundle-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn tar_gz(path: &Path, files: &[(&str, &[u8])]) {
        let encoder = flate2::write::GzEncoder::new(
            fs::File::create(path).unwrap(),
            flate2::Compression::fast(),
        );
        let mut builder = tar::Builder::new(encoder);
        for (name, data) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, name, *data).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap();
    }

    #[test]
    fn tar_extracts_without_the_top_level_directory() {
        let root = scratch("tar");
        let archive = root.join("p.tar.gz");
        tar_gz(
            &archive,
            &[("pkg/quill", b"exe"), ("pkg/lib/libtdjson.so", b"td")],
        );
        let out = root.join("out");
        extract(&archive, ArchiveKind::TarGz, &out).unwrap();
        assert_eq!(fs::read(out.join("quill")).unwrap(), b"exe");
        assert_eq!(fs::read(out.join("lib/libtdjson.so")).unwrap(), b"td");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(out.join("quill"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o111, 0o111);
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn tar_rejects_escapes_and_mixed_roots() {
        let root = scratch("tar-bad");
        // The tar writer refuses `..`, so craft the header bytes by hand.
        let archive = root.join("p.tar.gz");
        let mut raw = vec![0u8; 512];
        raw[..8].copy_from_slice(b"pkg/../x");
        raw[100..107].copy_from_slice(b"0000644");
        raw[124..135].copy_from_slice(b"00000000000");
        raw[156] = b'0';
        raw[257..262].copy_from_slice(b"ustar");
        raw[148..156].copy_from_slice(b"        ");
        let sum: u32 = raw.iter().map(|b| u32::from(*b)).sum();
        raw[148..155].copy_from_slice(format!("{sum:06o}\0").as_bytes());
        raw.extend(vec![0u8; 1024]);
        let mut gz = flate2::write::GzEncoder::new(
            fs::File::create(&archive).unwrap(),
            flate2::Compression::fast(),
        );
        gz.write_all(&raw).unwrap();
        gz.finish().unwrap();
        assert!(extract(&archive, ArchiveKind::TarGz, &root.join("o1")).is_err());
        assert!(!root.join("x").exists());

        let mixed = root.join("mixed.tar.gz");
        tar_gz(&mixed, &[("a/one", b"1"), ("b/two", b"2")]);
        assert!(extract(&mixed, ArchiveKind::TarGz, &root.join("o2")).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn zip_extracts_deflated_entries_and_rejects_escapes() {
        let root = scratch("zip");
        let write_zip = |path: &Path, names: &[&str]| {
            let mut writer = zip::ZipWriter::new(fs::File::create(path).unwrap());
            for name in names {
                writer
                    .start_file(
                        *name,
                        zip::write::SimpleFileOptions::default()
                            .compression_method(zip::CompressionMethod::Deflated),
                    )
                    .unwrap();
                writer.write_all(b"data").unwrap();
            }
            writer.finish().unwrap();
        };
        let good = root.join("g.zip");
        write_zip(
            &good,
            &["pkg/quill.exe", "pkg\\tdjson.dll", "pkg/licenses/a.txt"],
        );
        let out = root.join("out");
        extract(&good, ArchiveKind::Zip, &out).unwrap();
        assert!(out.join("quill.exe").is_file());
        assert!(out.join("tdjson.dll").is_file());
        assert!(out.join("licenses/a.txt").is_file());
        let evil = root.join("b.zip");
        write_zip(&evil, &["pkg/quill.exe", "pkg/../../evil"]);
        assert!(extract(&evil, ArchiveKind::Zip, &root.join("o3")).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
