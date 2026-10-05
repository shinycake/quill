//! Path sandbox: display only under allowed roots; send only via explicit user pick.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, OnceLock};

/// Canonicalize `candidate` and return it only if it is an existing file under an allowed root.
///
/// Roots may be directories (`tdlib_files`, demo fixtures) or an explicit file allowlist.
/// Symlinks that resolve outside a root are rejected.
///
/// Renders ask for every visible image's path each frame, so verdicts are
/// memoized for [`VERDICT_TTL`] (a symlink swapped in afterwards is caught
/// on the next check) and root canonicalizations for the process.
pub fn sandboxed_display_path(candidate: &str, allowed_roots: &[PathBuf]) -> Option<PathBuf> {
    if candidate.is_empty() {
        return None;
    }
    let key = (candidate.to_owned(), roots_key(allowed_roots));
    let now = std::time::Instant::now();
    if let Ok(cache) = VERDICTS.lock()
        && let Some((at, verdict)) = cache.get(&key)
        && now.duration_since(*at) < VERDICT_TTL
    {
        return verdict.clone();
    }
    let verdict = sandboxed_display_path_uncached(candidate, allowed_roots);
    if let Ok(mut cache) = VERDICTS.lock() {
        if cache.len() > 8192 {
            cache.retain(|_, (at, _)| now.duration_since(*at) < VERDICT_TTL);
        }
        cache.insert(key, (now, verdict.clone()));
    }
    verdict
}

const VERDICT_TTL: std::time::Duration = std::time::Duration::from_secs(1);

type VerdictCache = HashMap<(String, u64), (std::time::Instant, Option<PathBuf>)>;
static VERDICTS: LazyLock<Mutex<VerdictCache>> = LazyLock::new(|| Mutex::new(HashMap::new()));
static CANONICAL_ROOTS: LazyLock<Mutex<HashMap<PathBuf, PathBuf>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn roots_key(roots: &[PathBuf]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    roots.hash(&mut hasher);
    hasher.finish()
}

fn canonical_root(root: &Path) -> Option<PathBuf> {
    if let Ok(cache) = CANONICAL_ROOTS.lock()
        && let Some(canon) = cache.get(root)
    {
        return Some(canon.clone());
    }
    let canon = std::fs::canonicalize(root).ok()?;
    if let Ok(mut cache) = CANONICAL_ROOTS.lock() {
        cache.insert(root.to_path_buf(), canon.clone());
    }
    Some(canon)
}

fn sandboxed_display_path_uncached(candidate: &str, allowed_roots: &[PathBuf]) -> Option<PathBuf> {
    let file = canonical_file(Path::new(candidate))?;
    for root in allowed_roots {
        let Some(root_canon) = canonical_root(root) else {
            continue;
        };
        if file == root_canon || file.starts_with(&root_canon) {
            return Some(file);
        }
    }
    None
}

static ENSURED_DIRS: LazyLock<Mutex<HashSet<PathBuf>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

/// [`secure_create_dir`] once per process (and again after
/// [`sweep_media_caches`] removes the caches). Render paths call this for
/// the frame-cache roots every frame.
pub fn ensure_private_dir(path: &Path) {
    if ENSURED_DIRS.lock().is_ok_and(|set| set.contains(path)) {
        return;
    }
    if secure_create_dir(path).is_ok()
        && let Ok(mut set) = ENSURED_DIRS.lock()
    {
        set.insert(path.to_path_buf());
    }
}

/// Validate a path the user explicitly picked for sending.
///
/// Returns the canonical file path, or `None` if missing / not a file.
/// Callers must invoke this only from an explicit attach action — never from
/// untrusted TDLib JSON `local.path` values when building `sendMessage`.
pub fn pick_send_path(candidate: &Path) -> Option<PathBuf> {
    canonical_file(candidate)
}

/// True when `path` equals a previously picked canonical send path.
///
/// Send builders should only embed paths that pass this check against the
/// attachment frozen at composer submit (not arbitrary strings from JSON).
pub fn is_explicit_send_path(path: &Path, picked: &Path) -> bool {
    let Some(candidate) = canonical_file(path) else {
        return false;
    };
    let Ok(picked_canon) = std::fs::canonicalize(picked) else {
        return false;
    };
    candidate == picked_canon && picked_canon.is_file()
}

fn canonical_file(path: &Path) -> Option<PathBuf> {
    let canon = std::fs::canonicalize(path).ok()?;
    canon.is_file().then_some(canon)
}

/// Account scope for private media caches. The app runs one Telegram account
/// per process; `prepare_connect` sets the real scope, default is "primary".
static MEDIA_CACHE_SCOPE: OnceLock<String> = OnceLock::new();

/// Set the account scope used by [`media_cache_base`]. Called from
/// `plan_restore` once per connect; later calls are ignored.
pub fn set_media_cache_scope(scope: &str) {
    let _ = MEDIA_CACHE_SCOPE.set(scope.to_owned());
}

fn media_cache_scope() -> &'static str {
    // Non-initializing read: a startup sweep must not poison the scope
    // before `prepare_connect` sets it.
    static FALLBACK: &str = "primary";
    MEDIA_CACHE_SCOPE.get().map_or(FALLBACK, String::as_str)
}

/// Account-scoped base for decrypted media scratch (GIF/video/viewer frames,
/// video-note thumbnails): `{temp}/quill-media-cache/{account}`. Create with
/// [`secure_create_dir`] — never world-readable, never through a symlink.
pub fn media_cache_base() -> PathBuf {
    cache_temp_dir()
        .join("quill-media-cache")
        .join(media_cache_scope())
}

// Unit-test sweeps must not remove media owned by a live app or UI demo.
pub(crate) fn cache_temp_dir() -> PathBuf {
    #[cfg(test)]
    {
        std::env::temp_dir().join(format!("quill-test-cache-{}", std::process::id()))
    }
    #[cfg(not(test))]
    {
        // Render paths resolve cache roots every frame; read `TMPDIR` once.
        static TEMP: OnceLock<PathBuf> = OnceLock::new();
        TEMP.get_or_init(std::env::temp_dir).clone()
    }
}

/// Tests that create or delete `{temp}/quill-media-cache` must hold this for
/// the whole test. `sweep_removes_stale_viewer_frame_caches` asserts the
/// account base is gone after the sweep; a parallel test recreating
/// `video-frames` or `gif-frames` under that base makes the assertion flake.
#[cfg(test)]
pub(crate) fn lock_shared_media_cache() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Remove every account's media cache plus the legacy pre-fix layouts
/// (world-readable). Startup sweep, logout, media expiry. Best-effort; the
/// directories are regenerable scratch. Sweeps the whole parent without
/// reading the account scope, so a startup sweep can never poison it.
pub fn sweep_media_caches() {
    // Cached directory and canonicalization facts no longer hold.
    if let Ok(mut set) = ENSURED_DIRS.lock() {
        set.clear();
    }
    if let Ok(mut cache) = CANONICAL_ROOTS.lock() {
        cache.clear();
    }
    if let Ok(mut cache) = VERDICTS.lock() {
        cache.clear();
    }
    let _ = std::fs::remove_dir_all(cache_temp_dir().join("quill-media-cache"));
    for legacy in [
        "quill-gif-frames",
        "quill-video-frames",
        "quill-viewer-frames",
        "quill-video-note-thumbs",
    ] {
        let _ = std::fs::remove_dir_all(cache_temp_dir().join(legacy));
    }
}

/// Create a private directory: mode 0700 on unix, repairing pre-existing
/// directories that are too permissive. Only manages components strictly
/// below the temp dir; each is checked with `symlink_metadata` so a planted
/// symlink is removed, never followed. Rejects paths outside the temp dir.
pub fn secure_create_dir(path: &Path) -> std::io::Result<()> {
    let temp = std::env::temp_dir();
    let rel = path.strip_prefix(&temp).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "media cache must live under the temp dir",
        )
    })?;
    let mut cur = temp;
    for component in rel.components() {
        // Only plain names are allowed: `..`/prefixes/root would resolve
        // outside the cache tree via `symlink_metadata`'s `..` handling.
        if !matches!(component, std::path::Component::Normal(_)) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "media cache path must be plain components",
            ));
        }
        cur.push(component);
        match std::fs::symlink_metadata(&cur) {
            Ok(meta) if meta.file_type().is_symlink() => {
                std::fs::remove_file(&cur)?;
                std::fs::create_dir(&cur)?;
            }
            Ok(meta) if meta.is_dir() => {}
            Ok(_) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "media cache path occupied by non-directory",
                ));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&cur)?;
            }
            Err(e) => return Err(e),
        }
        restrict_dir(&cur)?;
    }
    Ok(())
}

#[cfg(unix)]
fn restrict_dir(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn restrict_dir(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

/// Owner-only file permissions for decrypted media (defense in depth; the
/// 0700 parent directory is the real barrier). No-op off unix.
pub fn restrict_file(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    let _ = path;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn scratch(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "quill-sandbox-{}-{}-{}",
            label,
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[cfg(unix)]
    fn mode_of(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    #[cfg(unix)]
    fn secure_create_dir_is_owner_only() {
        let target = std::env::temp_dir().join(format!(
            "quill-secure-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        secure_create_dir(&target).unwrap();
        assert_eq!(mode_of(&target), 0o700, "cache dir must be 0700");
        let _ = fs::remove_dir_all(&target);
    }

    #[test]
    #[cfg(unix)]
    fn secure_create_dir_repairs_loose_permissions() {
        let target = std::env::temp_dir().join(format!(
            "quill-repair-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        // Pre-existing world-readable dir (the old 0755 layout).
        fs::create_dir_all(&target).unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).unwrap();
        secure_create_dir(&target).unwrap();
        assert_eq!(mode_of(&target), 0o700, "pre-existing dir is tightened");
        let _ = fs::remove_dir_all(&target);
    }

    #[test]
    fn secure_create_dir_replaces_symlink_without_following() {
        #[cfg(not(unix))]
        return;
        let victim = scratch("victim");
        let link = std::env::temp_dir().join(format!(
            "quill-link-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        #[cfg(unix)]
        std::os::unix::fs::symlink(&victim, &link).unwrap();
        secure_create_dir(&link).unwrap();
        assert!(
            fs::symlink_metadata(&link).unwrap().is_dir()
                && !fs::symlink_metadata(&link)
                    .unwrap()
                    .file_type()
                    .is_symlink(),
            "planted symlink is replaced by a real directory"
        );
        assert!(
            fs::read_dir(&victim).unwrap().next().is_none(),
            "symlink target is never written through"
        );
        let _ = fs::remove_dir_all(&link);
        let _ = fs::remove_dir_all(&victim);
    }

    #[test]
    fn secure_create_dir_rejects_paths_outside_temp() {
        for outside in ["/quill-secure-outside-temp-test", "/"] {
            assert!(
                secure_create_dir(Path::new(outside)).is_err(),
                "must not manage directories outside the temp dir: {outside}"
            );
        }
    }

    #[test]
    fn secure_create_dir_rejects_parent_traversal() {
        // Shares `{temp}/quill-media-cache` with the viewer-cache sweep test.
        let _guard = lock_shared_media_cache();
        // `temp/quill-media-cache/../evil` strips to an under-temp path, but
        // `..` must never reach symlink_metadata's resolution.
        let traversal = std::env::temp_dir()
            .join("quill-media-cache")
            .join("..")
            .join("quill-evil-traversal");
        assert!(
            secure_create_dir(&traversal).is_err(),
            "parent traversal must fail closed"
        );
        assert!(
            !std::env::temp_dir().join("quill-evil-traversal").exists(),
            "traversal must not create anything outside the cache tree"
        );
    }

    #[test]
    #[cfg(unix)]
    fn restrict_file_is_owner_only() {
        let dir = scratch("restrict");
        let file = dir.join("frame.png");
        fs::write(&file, b"png").unwrap();
        restrict_file(&file).unwrap();
        assert_eq!(mode_of(&file), 0o600, "frame file must be 0600");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn accepts_file_under_tdlib_files() {
        let root = scratch("ok");
        let tdlib_files = root.join("files");
        fs::create_dir_all(&tdlib_files).unwrap();
        let photo = tdlib_files.join("thumb.png");
        fs::write(&photo, [1, 2, 3]).unwrap();
        let got =
            sandboxed_display_path(photo.to_str().unwrap(), std::slice::from_ref(&tdlib_files));
        assert_eq!(got, Some(std::fs::canonicalize(&photo).unwrap()));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_path_outside_roots() {
        let root = scratch("out");
        let tdlib_files = root.join("files");
        fs::create_dir_all(&tdlib_files).unwrap();
        let outside = root.join("evil.png");
        fs::write(&outside, [9]).unwrap();
        assert_eq!(
            sandboxed_display_path(outside.to_str().unwrap(), &[tdlib_files]),
            None
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_empty_and_missing() {
        assert_eq!(sandboxed_display_path("", &[PathBuf::from("/tmp")]), None);
        assert_eq!(
            sandboxed_display_path("/no/such/quill-media.png", &[PathBuf::from("/tmp")]),
            None
        );
    }

    #[test]
    fn demo_allowlist_file_is_accepted() {
        let root = scratch("demo");
        let fixture = root.join("demo-thumb.png");
        fs::write(&fixture, [4, 5]).unwrap();
        let got = sandboxed_display_path(fixture.to_str().unwrap(), std::slice::from_ref(&fixture));
        assert_eq!(got, Some(std::fs::canonicalize(&fixture).unwrap()));
        let dir_root = scratch("demo-dir");
        let fixtures = dir_root.join("fixtures");
        fs::create_dir_all(&fixtures).unwrap();
        let nested = fixtures.join("demo-thumb.png");
        fs::write(&nested, [6]).unwrap();
        let got = sandboxed_display_path(nested.to_str().unwrap(), &[fixtures]);
        assert_eq!(got, Some(std::fs::canonicalize(&nested).unwrap()));
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&dir_root);
    }

    #[test]
    fn pick_send_path_requires_existing_file() {
        let root = scratch("pick");
        let file = root.join("photo.png");
        fs::write(&file, [1]).unwrap();
        let got = pick_send_path(&file).unwrap();
        assert_eq!(got, std::fs::canonicalize(&file).unwrap());
        assert!(pick_send_path(&root.join("missing.png")).is_none());
        assert!(!is_explicit_send_path(Path::new("/etc/passwd"), &file));
        assert!(is_explicit_send_path(&file, &got));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn pick_send_path_is_independent_of_display_roots() {
        // Sending uses the explicit pick, not the display allowlist.
        let root = scratch("send-any");
        let outside = root.join("picked.bin");
        fs::write(&outside, [2]).unwrap();
        let picked = pick_send_path(&outside).unwrap();
        assert!(is_explicit_send_path(&outside, &picked));
        assert_eq!(
            sandboxed_display_path(outside.to_str().unwrap(), &[root.join("files")]),
            None
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape() {
        let root = scratch("link");
        let tdlib_files = root.join("files");
        fs::create_dir_all(&tdlib_files).unwrap();
        let outside = root.join("outside.png");
        fs::write(&outside, [7]).unwrap();
        let link = tdlib_files.join("alias.png");
        match std::os::unix::fs::symlink(&outside, &link) {
            Ok(()) => {
                assert_eq!(
                    sandboxed_display_path(link.to_str().unwrap(), &[tdlib_files]),
                    None
                );
            }
            Err(_) => {
                // Environment without symlink permission: skip the escape case.
            }
        }
        let _ = fs::remove_dir_all(&root);
    }
}
