use super::*;

thread_local! {
    /// Makes this thread's `try_lock` fail as a filesystem without locks.
    pub(super) static LOCKS_FAIL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Runs `body` with this thread's locks failing.
fn without_locks<T>(body: impl FnOnce() -> T) -> T {
    LOCKS_FAIL.with(|fail| fail.set(true));
    let result = body();
    LOCKS_FAIL.with(|fail| fail.set(false));
    result
}

fn root(name: &str) -> PathBuf {
    let root = crate::scratch::process_dir("carrier-data-folder")
        .join(name)
        .join("webview-carrier");
    let _ = std::fs::remove_dir_all(&root);
    root
}

fn with_cookie(dir: &Path) -> PathBuf {
    let cookies = dir.join("EBWebView/Default/Cookies");
    let _ = std::fs::create_dir_all(dir.join("EBWebView/Default"));
    let _ = std::fs::write(&cookies, b"cookie");
    cookies
}

fn names(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| entry.file_name().to_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn swept(root: &Path) {
    if let Some(sweep) = sweep_in_background(root.to_path_buf()) {
        let _ = sweep.join();
    }
}

#[test]
fn each_claim_is_a_new_empty_folder() {
    let root = root("claims");
    let first = Generation::claim(&root).unwrap();
    let second = Generation::claim(&root).unwrap();
    assert_ne!(first.dir(), second.dir());
    assert!(first.dir().is_dir() && second.dir().is_dir());
    assert_eq!(std::fs::read_dir(first.dir()).unwrap().count(), 0);
}

#[test]
fn removing_a_generation_leaves_nothing_behind() {
    let root = root("remove");
    let generation = Generation::claim(&root).unwrap();
    with_cookie(generation.dir());
    generation.remove(RETRY_FOR);
    assert_eq!(names(&root), Vec::<String>::new());
}

#[test]
fn a_sweep_keeps_generations_in_use_and_removes_the_rest() {
    let root = root("sweep");
    let live = Generation::claim(&root).unwrap();
    let live_cookie = with_cookie(live.dir());
    // An earlier run's: a folder and its released lock, a folder whose
    // lock went, a lock whose folder went, and the old layout's files.
    with_cookie(&root.join("1-0"));
    let _ = std::fs::write(root.join("1-0.lock"), b"");
    with_cookie(&root.join("1-1"));
    let _ = std::fs::write(root.join("1-2.lock"), b"");
    with_cookie(&root.join("webview2"));
    swept(&root);
    assert!(live_cookie.exists());
    let live_name = live
        .dir()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert_eq!(
        names(&root),
        vec![live_name.clone(), format!("{live_name}.lock")]
    );
}

/// An earlier process with the same pid left its generation behind (a
/// crash: the folder and its lock file stay, the lock goes).
#[test]
fn a_claim_never_reuses_a_leftover_name() {
    let root = root("leftover");
    let _ = std::fs::create_dir_all(&root);
    let cookie = with_cookie(&root.join("7-0"));
    let _ = std::fs::write(root.join("7-0.lock"), b"");
    assert_eq!(Generation::try_claim(&root, "7-0").err(), Some(Miss::Taken));
    with_cookie(&root.join("7-1"));
    assert_eq!(Generation::try_claim(&root, "7-1").err(), Some(Miss::Taken));
    assert!(cookie.exists());
    let fresh = Generation::try_claim(&root, "7-2").unwrap();
    assert_eq!(std::fs::read_dir(fresh.dir()).unwrap().count(), 0);
}

/// Another process (here, another open of the same file) holds the lock.
#[test]
fn a_held_generation_is_not_swept() {
    let root = root("held");
    with_cookie(&root.join("9-0"));
    let lock = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join("9-0.lock"))
        .unwrap();
    lock.lock().unwrap();
    swept(&root);
    assert!(root.join("9-0/EBWebView/Default/Cookies").exists());
    // Unlocked, not only closed: a child another test thread is starting
    // shares this open file, and with it the lock, until its exec.
    lock.unlock().unwrap();
    drop(lock);
    swept(&root);
    assert_eq!(names(&root), Vec::<String>::new());
}

#[test]
fn only_generation_names_count() {
    assert!(is_generation("123-0"));
    assert!(!is_generation("123"));
    assert!(!is_generation("123-"));
    assert!(!is_generation("-1"));
    assert!(!is_generation("a-1"));
    assert!(!is_generation("1-2-3"));
}

#[test]
fn the_listing_is_relative_and_bounded() {
    let root = root("listing");
    let generation = Generation::claim(&root).unwrap();
    with_cookie(generation.dir());
    let lines = listing(generation.dir(), 50);
    assert!(
        lines.contains(&"EBWebView/".to_string()) || lines.contains(&"EBWebView\\".to_string()),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|line| line.ends_with("Cookies (6 bytes)")),
        "{lines:?}"
    );
    assert_eq!(listing(generation.dir(), 1).len(), 1);
}

/// Where locks cannot be taken at all, a claim gives up at the first name
/// and leaves no lock file behind.
#[test]
fn a_claim_that_cannot_lock_leaves_nothing() {
    let root = root("no-locks-claim");
    assert!(without_locks(|| Generation::claim(&root)).is_none());
    assert_eq!(names(&root), Vec::<String>::new());
    assert!(Generation::claim(&root).is_some());
}

/// A lock that cannot be tested goes only when its folder is gone; a
/// folder next to one is kept.
#[test]
fn a_sweep_that_cannot_lock_keeps_folders() {
    let root = root("no-locks-sweep");
    let _ = std::fs::create_dir_all(&root);
    let kept = with_cookie(&root.join("3-0"));
    let _ = std::fs::write(root.join("3-0.lock"), b"");
    let _ = std::fs::write(root.join("3-1.lock"), b"");
    without_locks(|| sweep(&root));
    assert!(kept.exists());
    assert_eq!(
        names(&root),
        vec!["3-0".to_string(), "3-0.lock".to_string()]
    );
}
