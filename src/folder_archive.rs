//! Zip archives for dropped folders and file sets, after Telegram Desktop's
//! `Storage::PrepareFolderArchive` / `PrepareFilesArchive`
//! (storage/storage_folder_archive.cpp).
//!
//! A dropped folder becomes `<folder>.zip` with every file and subfolder
//! under a root of the folder's name; dropped files become `<parent>.zip`
//! (or `Archive.zip`) with the files at the top level, duplicates renamed
//! "name (2).ext". Symbolic links are skipped, already-compressed formats
//! are stored, everything else is deflated, and names are UTF-8. Entries
//! stream through a data descriptor, so nothing is held in memory and the
//! written size can be checked against the upload limit as it grows.

use std::fs::File;
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use flate2::Compression;
use flate2::write::DeflateEncoder;

use crate::drop_modes::ArchiveSource;

/// Telegram's per-file upload limit for non-Premium accounts.
pub const ARCHIVE_SIZE_LIMIT: u64 = 2_000_000_000;

/// Archives left behind by a crashed run are removed after this long.
pub const STALE_AFTER: Duration = Duration::from_secs(24 * 60 * 60);

const CHUNK: usize = 1024 * 1024;
const FLAG_DATA_DESCRIPTOR: u16 = 1 << 3;
const FLAG_UTF8: u16 = 1 << 11;
const MAX_ENTRIES: usize = u16::MAX as usize;

/// Why an archive could not be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchiveError {
    /// Nothing to put in it.
    Empty,
    /// A file could not be read or the archive could not be written.
    Io(String),
    /// The archive grew past the upload limit.
    TooLarge,
    /// More entries than a zip without Zip64 can list.
    TooManyEntries,
}

impl ArchiveError {
    /// The line shown to the person who dropped the files.
    pub fn note(&self) -> String {
        match self {
            Self::Empty => "There are no files to put in an archive.".into(),
            Self::Io(reason) => format!("Could not make the archive: {reason}"),
            Self::TooLarge => "The archive is too big to send (the limit is 2 GB).".into(),
            Self::TooManyEntries => "That folder has too many files for one archive.".into(),
        }
    }
}

impl From<io::Error> for ArchiveError {
    fn from(err: io::Error) -> Self {
        Self::Io(err.to_string())
    }
}

/// One thing to write into the zip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Where the file is; `None` for a directory entry.
    pub source: Option<PathBuf>,
    /// The name inside the zip, `/`-separated, with a trailing `/` for a
    /// directory.
    pub name: String,
    pub modified: SystemTime,
}

/// The archive's file name: the folder name, the files' common parent
/// folder, or "Archive".
pub fn archive_name(source: &ArchiveSource) -> String {
    let base = match source {
        ArchiveSource::Folder(folder) => dir_name(folder),
        ArchiveSource::Files(files) => common_parent_name(files),
    };
    format!("{}.zip", base.unwrap_or_else(|| "Archive".into()))
}

fn dir_name(path: &Path) -> Option<String> {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
}

fn common_parent_name(files: &[PathBuf]) -> Option<String> {
    let mut parent: Option<&Path> = None;
    for file in files {
        let folder = file.parent()?;
        match parent {
            None => parent = Some(folder),
            Some(seen) if seen == folder => {}
            Some(_) => return None,
        }
    }
    parent.and_then(dir_name)
}

/// "name (2).ext" for the second file of a name, "name (3).ext" for the third
/// (case-insensitive, like tdesktop's `DedupedName`).
pub fn deduped_name(counts: &mut std::collections::HashMap<String, u32>, name: &str) -> String {
    let count = counts.entry(name.to_lowercase()).or_insert(0);
    *count += 1;
    if *count == 1 {
        return name.to_owned();
    }
    let number = *count;
    let (stem, ext) = match name.rfind('.') {
        Some(dot) if dot > 0 => name.split_at(dot),
        _ => (name, ""),
    };
    let mut candidate = format!("{stem} ({number}){ext}");
    // A file literally named "a (2).txt" already took that slot.
    while {
        let taken = counts.entry(candidate.to_lowercase()).or_insert(0);
        *taken += 1;
        *taken > 1
    } {
        let number = {
            let count = counts.entry(name.to_lowercase()).or_insert(0);
            *count += 1;
            *count
        };
        candidate = format!("{stem} ({number}){ext}");
    }
    candidate
}

/// Formats that are already compressed, which deflate would only slow down.
pub fn already_compressed(name: &str) -> bool {
    const STORED: &[&str] = &[
        "3gp", "7z", "aac", "apk", "avi", "avif", "bz2", "cab", "docx", "epub", "flac", "flv",
        "gif", "gz", "heic", "heif", "jar", "jpeg", "jpg", "jxl", "m4a", "m4v", "mkv", "mov",
        "mp3", "mp4", "odp", "ods", "odt", "oga", "ogg", "opus", "png", "pptx", "rar", "tgz",
        "webm", "webp", "wma", "wmv", "xlsx", "xz", "zip", "zst",
    ];
    let leaf = name.rsplit('/').next().unwrap_or(name);
    match leaf.rfind('.') {
        Some(dot) if dot > 0 => {
            let ext = leaf[dot + 1..].to_ascii_lowercase();
            STORED.contains(&ext.as_str())
        }
        _ => false,
    }
}

/// The entries for `source`, read from the file system. Folder entries come
/// parents first, sorted by name; unreadable files fail the whole archive.
pub fn plan_entries(source: &ArchiveSource) -> Result<Vec<Entry>, ArchiveError> {
    let mut entries = Vec::new();
    match source {
        ArchiveSource::Folder(folder) => {
            let root = dir_name(folder).unwrap_or_else(|| "Archive".into());
            walk(folder, &root, &mut entries)?;
        }
        ArchiveSource::Files(files) => {
            let mut counts = std::collections::HashMap::new();
            for path in files {
                let meta = std::fs::metadata(path)?;
                if meta.is_dir() {
                    continue;
                }
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "file".into());
                entries.push(Entry {
                    source: Some(path.clone()),
                    name: deduped_name(&mut counts, &name),
                    modified: meta.modified().unwrap_or(UNIX_EPOCH),
                });
            }
        }
    }
    if !entries.iter().any(|entry| entry.source.is_some()) {
        return Err(ArchiveError::Empty);
    }
    if entries.len() > MAX_ENTRIES {
        return Err(ArchiveError::TooManyEntries);
    }
    Ok(entries)
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<Entry>) -> Result<(), ArchiveError> {
    let mut children = Vec::new();
    for child in std::fs::read_dir(dir)? {
        let child = child?;
        // `file_type` does not follow links, so a link is neither dir nor file.
        let kind = child.file_type()?;
        if kind.is_dir() || kind.is_file() {
            children.push((child.file_name(), child.path(), kind.is_dir()));
        }
    }
    children.sort();
    for (name, path, is_dir) in children {
        let name = name.to_string_lossy();
        let meta = std::fs::metadata(&path)?;
        let modified = meta.modified().unwrap_or(UNIX_EPOCH);
        if is_dir {
            let inner = format!("{prefix}/{name}");
            out.push(Entry {
                source: None,
                name: format!("{inner}/"),
                modified,
            });
            walk(&path, &inner, out)?;
        } else {
            out.push(Entry {
                source: Some(path),
                name: format!("{prefix}/{name}"),
                modified,
            });
        }
        if out.len() > MAX_ENTRIES {
            return Err(ArchiveError::TooManyEntries);
        }
    }
    Ok(())
}

/// Build the archive for `source` as `<out_dir>/<unique>/<name>.zip` (the
/// file keeps its display name) and return its path.
pub fn build_archive(
    source: &ArchiveSource,
    out_dir: &Path,
    limit: u64,
) -> Result<PathBuf, ArchiveError> {
    let entries = plan_entries(source)?;
    remove_stale(out_dir, STALE_AFTER);
    let folder = out_dir.join(unique_name());
    std::fs::create_dir_all(&folder)?;
    let path = folder.join(archive_name(source));
    let result = File::create(&path)
        .map_err(ArchiveError::from)
        .and_then(|file| write_zip(BufWriter::new(file), &entries, limit));
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&folder);
        return result.map(|()| path);
    }
    Ok(path)
}

fn unique_name() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!(
        "{:x}-{:x}-{}",
        nanos,
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

/// Delete archive folders in `dir` older than `age`.
pub fn remove_stale(dir: &Path, age: Duration) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let now = SystemTime::now();
    for entry in read.flatten() {
        let old = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|elapsed| elapsed > age);
        if old && entry.path().is_dir() {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// Counts what passes through, so offsets and the total size are known.
struct Counting<W: Write> {
    inner: W,
    written: u64,
    limit: u64,
}

impl<W: Write> Write for Counting<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.written += n as u64;
        if self.written > self.limit {
            return Err(io::Error::other(TooLargeMarker));
        }
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[derive(Debug)]
struct TooLargeMarker;

impl std::fmt::Display for TooLargeMarker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("archive too large")
    }
}

impl std::error::Error for TooLargeMarker {}

fn classify_io(err: io::Error) -> ArchiveError {
    if err
        .get_ref()
        .is_some_and(|inner| inner.is::<TooLargeMarker>())
    {
        ArchiveError::TooLarge
    } else {
        ArchiveError::Io(err.to_string())
    }
}

struct Central {
    name: String,
    flags: u16,
    method: u16,
    time: u16,
    date: u16,
    crc: u32,
    csize: u32,
    usize: u32,
    dir: bool,
    offset: u32,
}

/// Write `entries` as a zip to `out`. The size limit applies to the bytes
/// written, checked as they go.
pub fn write_zip<W: Write>(out: W, entries: &[Entry], limit: u64) -> Result<(), ArchiveError> {
    write_zip_inner(out, entries, limit).map_err(classify_io)
}

fn write_zip_inner<W: Write>(out: W, entries: &[Entry], limit: u64) -> io::Result<()> {
    let mut out = Counting {
        inner: out,
        written: 0,
        limit,
    };
    let mut central = Vec::with_capacity(entries.len());
    let mut buffer = vec![0u8; CHUNK];
    for entry in entries {
        let offset = u32::try_from(out.written).map_err(|_| io::Error::other(TooLargeMarker))?;
        let (time, date) = dos_time(entry.modified);
        let dir = entry.source.is_none();
        let stored = dir || already_compressed(&entry.name);
        let method: u16 = if stored { 0 } else { 8 };
        let flags = FLAG_UTF8 | if dir { 0 } else { FLAG_DATA_DESCRIPTOR };
        write_local_header(&mut out, &entry.name, flags, method, time, date)?;
        let (mut crc, mut csize, mut size) = (0u32, 0u64, 0u64);
        if let Some(path) = &entry.source {
            let mut file = File::open(path)?;
            let mut crc_state = flate2::Crc::new();
            let start = out.written;
            if stored {
                loop {
                    let read = file.read(&mut buffer)?;
                    if read == 0 {
                        break;
                    }
                    crc_state.update(&buffer[..read]);
                    out.write_all(&buffer[..read])?;
                    size += read as u64;
                }
            } else {
                let mut encoder = DeflateEncoder::new(&mut out, Compression::default());
                loop {
                    let read = file.read(&mut buffer)?;
                    if read == 0 {
                        break;
                    }
                    crc_state.update(&buffer[..read]);
                    encoder.write_all(&buffer[..read])?;
                    size += read as u64;
                }
                encoder.finish()?;
            }
            crc = crc_state.sum();
            csize = out.written - start;
            if size > u64::from(u32::MAX) || csize > u64::from(u32::MAX) {
                return Err(io::Error::other(TooLargeMarker));
            }
            // Data descriptor: the sizes the header left at zero.
            out.write_all(&0x0807_4b50u32.to_le_bytes())?;
            out.write_all(&crc.to_le_bytes())?;
            out.write_all(&(csize as u32).to_le_bytes())?;
            out.write_all(&(size as u32).to_le_bytes())?;
        }
        central.push(Central {
            name: entry.name.clone(),
            flags,
            method,
            time,
            date,
            crc,
            csize: csize as u32,
            usize: size as u32,
            dir,
            offset,
        });
    }
    let directory_start =
        u32::try_from(out.written).map_err(|_| io::Error::other(TooLargeMarker))?;
    for item in &central {
        out.write_all(&0x0201_4b50u32.to_le_bytes())?;
        out.write_all(&20u16.to_le_bytes())?; // version made by
        out.write_all(&20u16.to_le_bytes())?; // version needed
        out.write_all(&item.flags.to_le_bytes())?;
        out.write_all(&item.method.to_le_bytes())?;
        out.write_all(&item.time.to_le_bytes())?;
        out.write_all(&item.date.to_le_bytes())?;
        out.write_all(&item.crc.to_le_bytes())?;
        out.write_all(&item.csize.to_le_bytes())?;
        out.write_all(&item.usize.to_le_bytes())?;
        out.write_all(&(item.name.len() as u16).to_le_bytes())?;
        out.write_all(&0u16.to_le_bytes())?; // extra
        out.write_all(&0u16.to_le_bytes())?; // comment
        out.write_all(&0u16.to_le_bytes())?; // disk
        out.write_all(&0u16.to_le_bytes())?; // internal attributes
        let external: u32 = if item.dir { 0x10 } else { 0 };
        out.write_all(&external.to_le_bytes())?;
        out.write_all(&item.offset.to_le_bytes())?;
        out.write_all(item.name.as_bytes())?;
    }
    let directory_size =
        u32::try_from(out.written).map_err(|_| io::Error::other(TooLargeMarker))? - directory_start;
    out.write_all(&0x0605_4b50u32.to_le_bytes())?;
    out.write_all(&0u16.to_le_bytes())?; // this disk
    out.write_all(&0u16.to_le_bytes())?; // directory disk
    out.write_all(&(central.len() as u16).to_le_bytes())?;
    out.write_all(&(central.len() as u16).to_le_bytes())?;
    out.write_all(&directory_size.to_le_bytes())?;
    out.write_all(&directory_start.to_le_bytes())?;
    out.write_all(&0u16.to_le_bytes())?; // comment length
    out.flush()
}

fn write_local_header<W: Write>(
    out: &mut W,
    name: &str,
    flags: u16,
    method: u16,
    time: u16,
    date: u16,
) -> io::Result<()> {
    out.write_all(&0x0403_4b50u32.to_le_bytes())?;
    out.write_all(&20u16.to_le_bytes())?;
    out.write_all(&flags.to_le_bytes())?;
    out.write_all(&method.to_le_bytes())?;
    out.write_all(&time.to_le_bytes())?;
    out.write_all(&date.to_le_bytes())?;
    out.write_all(&0u32.to_le_bytes())?; // crc, in the descriptor
    out.write_all(&0u32.to_le_bytes())?; // compressed size
    out.write_all(&0u32.to_le_bytes())?; // size
    out.write_all(&(name.len() as u16).to_le_bytes())?;
    out.write_all(&0u16.to_le_bytes())?; // extra
    out.write_all(name.as_bytes())
}

/// MS-DOS time and date words for a timestamp (UTC; years before 1980 and
/// after 2107 are clamped).
pub fn dos_time(time: SystemTime) -> (u16, u16) {
    let secs = time
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let days = secs.div_euclid(86_400);
    let rest = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    if year < 1980 {
        return (0, (1 << 5) | 1);
    }
    let year = year.min(2107);
    let hour = (rest / 3600) as u16;
    let minute = ((rest % 3600) / 60) as u16;
    let second = (rest % 60) as u16;
    (
        (hour << 11) | (minute << 5) | (second / 2),
        (((year - 1980) as u16) << 9) | ((month as u16) << 5) | day as u16,
    )
}

/// Days since 1970-01-01 to (year, month, day), proleptic Gregorian.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (year + i64::from(month <= 2), month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "quill-archive-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn u16_at(bytes: &[u8], at: usize) -> u16 {
        u16::from_le_bytes([bytes[at], bytes[at + 1]])
    }

    fn u32_at(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
    }

    /// (name, method, crc, usize, extracted bytes) of every entry, read from
    /// the central directory.
    fn read_back(zip: &[u8]) -> Vec<(String, u16, u32, u32, Vec<u8>)> {
        let eocd = zip.len() - 22;
        assert_eq!(u32_at(zip, eocd), 0x0605_4b50);
        let count = usize::from(u16_at(zip, eocd + 10));
        let mut at = u32_at(zip, eocd + 16) as usize;
        let mut out = Vec::new();
        for _ in 0..count {
            assert_eq!(u32_at(zip, at), 0x0201_4b50);
            let method = u16_at(zip, at + 10);
            let crc = u32_at(zip, at + 16);
            let csize = u32_at(zip, at + 20) as usize;
            let size = u32_at(zip, at + 24);
            let name_len = usize::from(u16_at(zip, at + 28));
            let offset = u32_at(zip, at + 42) as usize;
            let name = String::from_utf8(zip[at + 46..at + 46 + name_len].to_vec()).unwrap();
            assert_eq!(u32_at(zip, offset), 0x0403_4b50);
            let local_name = usize::from(u16_at(zip, offset + 26));
            let data = offset + 30 + local_name;
            let raw = &zip[data..data + csize];
            let bytes = if method == 8 {
                let mut inflated = Vec::new();
                flate2::read::DeflateDecoder::new(raw)
                    .read_to_end(&mut inflated)
                    .unwrap();
                inflated
            } else {
                raw.to_vec()
            };
            out.push((name, method, crc, size, bytes));
            at += 46 + name_len;
        }
        out
    }

    #[test]
    fn folder_archive_roundtrips_with_a_root_name() {
        let root = scratch("folder").join("Trip");
        std::fs::create_dir_all(root.join("photos")).unwrap();
        std::fs::write(root.join("notes.txt"), "hello hello hello hello").unwrap();
        std::fs::write(root.join("photos").join("a.png"), [1u8, 2, 3, 4]).unwrap();
        let out = scratch("folder-out");
        let source = ArchiveSource::Folder(root.clone());
        let path = build_archive(&source, &out, ARCHIVE_SIZE_LIMIT).unwrap();
        assert_eq!(path.file_name().unwrap(), "Trip.zip");
        let entries = read_back(&std::fs::read(&path).unwrap());
        let names: Vec<&str> = entries.iter().map(|e| e.0.as_str()).collect();
        assert_eq!(
            names,
            ["Trip/notes.txt", "Trip/photos/", "Trip/photos/a.png"]
        );
        // Text is deflated and its checksum and bytes survive.
        assert_eq!(entries[0].1, 8);
        assert_eq!(entries[0].4, b"hello hello hello hello");
        let mut crc = flate2::Crc::new();
        crc.update(b"hello hello hello hello");
        assert_eq!(entries[0].2, crc.sum());
        // The directory has no data; the PNG is stored as is.
        assert_eq!((entries[1].1, entries[1].3), (0, 0));
        assert_eq!(
            (entries[2].1, entries[2].4.as_slice()),
            (0, &[1u8, 2, 3, 4][..])
        );
        let _ = std::fs::remove_dir_all(root.parent().unwrap());
        let _ = std::fs::remove_dir_all(out);
    }

    #[test]
    fn loose_files_archive_renames_duplicates_and_names_after_the_parent() {
        let parent = scratch("files").join("Receipts");
        std::fs::create_dir_all(&parent).unwrap();
        let a = parent.join("a.txt");
        std::fs::write(&a, "one").unwrap();
        let source = ArchiveSource::Files(vec![a.clone(), a.clone()]);
        assert_eq!(archive_name(&source), "Receipts.zip");
        let entries = plan_entries(&source).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["a.txt", "a (2).txt"]);
        let _ = std::fs::remove_dir_all(parent.parent().unwrap());
    }

    #[test]
    fn archive_names_fall_back_to_archive() {
        let source =
            ArchiveSource::Files(vec![PathBuf::from("/x/a.txt"), PathBuf::from("/y/b.txt")]);
        assert_eq!(archive_name(&source), "Archive.zip");
        assert_eq!(
            archive_name(&ArchiveSource::Folder(PathBuf::from("/"))),
            "Archive.zip"
        );
    }

    #[test]
    fn dedupe_matches_case_insensitively_and_skips_taken_names() {
        let mut counts = HashMap::new();
        assert_eq!(deduped_name(&mut counts, "a.txt"), "a.txt");
        assert_eq!(deduped_name(&mut counts, "A.TXT"), "A (2).TXT");
        let mut counts = HashMap::new();
        assert_eq!(deduped_name(&mut counts, "a (2).txt"), "a (2).txt");
        assert_eq!(deduped_name(&mut counts, "a.txt"), "a.txt");
        assert_eq!(deduped_name(&mut counts, "a.txt"), "a (3).txt");
        let mut counts = HashMap::new();
        assert_eq!(deduped_name(&mut counts, "README"), "README");
        assert_eq!(deduped_name(&mut counts, "README"), "README (2)");
    }

    #[test]
    fn compressed_formats_are_stored() {
        assert!(already_compressed("Trip/photos/a.PNG"));
        assert!(already_compressed("movie.mp4"));
        assert!(!already_compressed("notes.txt"));
        assert!(!already_compressed("png"));
        assert!(!already_compressed("dir.png/readme"));
    }

    #[test]
    fn empty_and_oversized_archives_are_refused() {
        let root = scratch("limits").join("Empty");
        std::fs::create_dir_all(root.join("inner")).unwrap();
        let out = scratch("limits-out");
        assert_eq!(
            build_archive(
                &ArchiveSource::Folder(root.clone()),
                &out,
                ARCHIVE_SIZE_LIMIT
            ),
            Err(ArchiveError::Empty)
        );
        std::fs::write(root.join("big.bin"), vec![7u8; 4096]).unwrap();
        assert_eq!(
            build_archive(&ArchiveSource::Folder(root.clone()), &out, 100),
            Err(ArchiveError::TooLarge)
        );
        // A refused archive leaves no folder behind.
        assert_eq!(std::fs::read_dir(&out).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(root.parent().unwrap());
        let _ = std::fs::remove_dir_all(out);
    }

    #[test]
    fn dos_time_encodes_utc_fields() {
        // 2024-02-29 13:45:30 UTC.
        let time = UNIX_EPOCH + Duration::from_secs(1_709_214_330);
        let (t, d) = dos_time(time);
        assert_eq!((t >> 11, (t >> 5) & 63, (t & 31) * 2), (13, 45, 30));
        assert_eq!(((d >> 9) + 1980, (d >> 5) & 15, d & 31), (2024, 2, 29));
        assert_eq!(dos_time(UNIX_EPOCH), (0, 33));
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_links_are_skipped() {
        let root = scratch("links").join("Dir");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("real.txt"), "x").unwrap();
        std::os::unix::fs::symlink(root.join("real.txt"), root.join("link.txt")).unwrap();
        let names: Vec<String> = plan_entries(&ArchiveSource::Folder(root.clone()))
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(names, ["Dir/real.txt"]);
        let _ = std::fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn stale_archives_are_removed() {
        let out = scratch("stale");
        std::fs::create_dir_all(out.join("old")).unwrap();
        remove_stale(&out, Duration::from_secs(3600));
        assert!(out.join("old").exists());
        std::thread::sleep(Duration::from_millis(20));
        remove_stale(&out, Duration::from_millis(1));
        assert!(!out.join("old").exists());
        let _ = std::fs::remove_dir_all(out);
    }
}
