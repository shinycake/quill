#![allow(clippy::indexing_slicing)]
use super::*;
use std::fs;

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("quill-swap-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn tree(root: &Path, files: &[(&str, &str)]) {
    for (name, data) in files {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, data).unwrap();
    }
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}

fn names(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    out.sort();
    out
}

/// The two-rename path, as on a filesystem without RENAME_EXCHANGE.
fn plain(_: &Path, _: &Path) -> io::Result<Exchange> {
    Ok(Exchange::Unsupported)
}

/// The real exchange (atomic on Linux and macOS, the plain path elsewhere)
/// and the forced plain path.
const STRATEGIES: [(&str, ExchangeFn); 2] = [("exchange", exchange), ("plain", plain)];

#[test]
fn swap_dir_replaces_every_file_and_keeps_the_backup_until_the_next_start() {
    for (tag, strategy) in STRATEGIES {
        let root = scratch(&format!("dir-ok-{tag}"));
        let (install, staged, backup) = (root.join("quill"), root.join("new"), root.join("bak"));
        tree(
            &install,
            &[("quill", "old"), ("lib/libtdjson.so", "old-td")],
        );
        tree(&staged, &[("quill", "new"), ("lib/libtdjson.so", "new-td")]);
        swap_dir_with(&install, &staged, &backup, || Ok(()), strategy).unwrap();
        assert_eq!(read(&install.join("quill")), "new", "{tag}");
        assert_eq!(read(&install.join("lib/libtdjson.so")), "new-td", "{tag}");
        assert!(!staged.exists(), "{tag}");
        // The old tree waits for the new version's first start.
        assert_eq!(read(&backup.join("quill")), "old", "{tag}");
        recover_or_clear_backup(&install, &install.join("quill"), &backup).unwrap();
        assert!(!backup.exists(), "{tag}");
        assert_eq!(names(&root), ["quill"], "{tag}");
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn swap_dir_restores_the_old_install_when_launch_fails() {
    for (tag, strategy) in STRATEGIES {
        let root = scratch(&format!("dir-launch-{tag}"));
        // A dotted name: the failed tree must not be named by replacing an
        // "extension" (quill-0.5 -> quill-0.update-failed).
        let (install, staged, backup) =
            (root.join("quill-0.5"), root.join("new"), root.join("bak"));
        tree(
            &install,
            &[("quill", "old"), ("lib/libtdjson.so", "old-td")],
        );
        tree(&staged, &[("quill", "new"), ("lib/libtdjson.so", "new-td")]);
        // Someone else's directory with that mangled name is never touched.
        tree(&root.join("quill-0.update-failed"), &[("keep", "me")]);
        let result = swap_dir_with(
            &install,
            &staged,
            &backup,
            || Err(io::Error::other("cannot start")),
            strategy,
        );
        assert_eq!(result.unwrap_err().to_string(), "cannot start", "{tag}");
        assert_eq!(read(&install.join("quill")), "old", "{tag}");
        assert_eq!(read(&install.join("lib/libtdjson.so")), "old-td", "{tag}");
        // No backup, staged or failed tree is left beside it.
        assert_eq!(
            names(&root),
            ["quill-0.5", "quill-0.update-failed"],
            "{tag}"
        );
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn swap_dir_restores_the_old_install_when_the_stage_is_missing() {
    for (tag, strategy) in STRATEGIES {
        let root = scratch(&format!("dir-missing-{tag}"));
        let (install, backup) = (root.join("quill"), root.join("bak"));
        tree(&install, &[("quill", "old")]);
        let result = swap_dir_with(&install, &root.join("absent"), &backup, || Ok(()), strategy);
        assert!(result.is_err(), "{tag}");
        assert_eq!(read(&install.join("quill")), "old", "{tag}");
        assert!(!backup.exists(), "{tag}");
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn swap_dir_keeps_an_install_when_the_rollback_rename_fails() {
    for (tag, strategy) in STRATEGIES {
        let root = scratch(&format!("dir-rollback-{tag}"));
        let (install, staged, backup) = (root.join("quill"), root.join("new"), root.join("bak"));
        tree(&install, &[("quill", "old")]);
        tree(&staged, &[("quill", "new")]);
        let elsewhere = root.join("elsewhere");
        // The backup vanishes before rollback, so putting it back fails.
        let result = swap_dir_with(
            &install,
            &staged,
            &backup,
            || {
                fs::rename(&backup, &elsewhere)?;
                Err(io::Error::other("cannot start"))
            },
            strategy,
        );
        let message = result.unwrap_err().to_string();
        assert!(
            message.starts_with("cannot start; restoring"),
            "{tag}: {message}"
        );
        // The install path still holds a complete tree (the new one).
        assert_eq!(read(&install.join("quill")), "new", "{tag}");
        assert_eq!(read(&elsewhere.join("quill")), "old", "{tag}");
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn swap_dir_refuses_when_a_previous_backup_exists() {
    let root = scratch("dir-backup");
    let (install, staged, backup) = (root.join("quill"), root.join("new"), root.join("bak"));
    tree(&install, &[("quill", "old")]);
    tree(&staged, &[("quill", "new")]);
    tree(&backup, &[("keep", "me")]);
    assert!(swap_dir(&install, &staged, &backup, || Ok(())).is_err());
    assert_eq!(read(&install.join("quill")), "old");
    assert!(backup.join("keep").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_stale_backup_beside_an_intact_install_is_cleared_and_no_longer_blocks() {
    let root = scratch("dir-stale");
    let (install, staged, backup) = (root.join("quill"), root.join("new"), root.join("bak"));
    tree(&install, &[("quill", "current")]);
    tree(&backup, &[("quill", "older"), ("lib/libtdjson.so", "td")]);
    tree(&staged, &[("quill", "new")]);
    recover_or_clear_backup(&install, &install.join("quill"), &backup).unwrap();
    assert!(!backup.exists());
    assert_eq!(read(&install.join("quill")), "current");
    swap_dir(&install, &staged, &backup, || Ok(())).unwrap();
    assert_eq!(read(&install.join("quill")), "new");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_missing_install_is_recovered_from_its_backup() {
    let root = scratch("dir-recover");
    let (install, backup) = (root.join("quill"), root.join("bak"));
    // A plain-rename swap killed between its two renames.
    tree(&backup, &[("quill", "old"), ("lib/libtdjson.so", "td")]);
    recover_or_clear_backup(&install, &install.join("quill"), &backup).unwrap();
    assert_eq!(read(&install.join("quill")), "old");
    assert_eq!(read(&install.join("lib/libtdjson.so")), "td");
    assert!(!backup.exists());
    // Nothing to do without a backup.
    recover_or_clear_backup(&install, &install.join("quill"), &backup).unwrap();
    assert_eq!(read(&install.join("quill")), "old");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_incomplete_install_with_a_backup_is_left_alone() {
    let root = scratch("dir-partial");
    let (install, backup) = (root.join("quill"), root.join("bak"));
    tree(&install, &[("lib/libtdjson.so", "td")]);
    tree(&backup, &[("quill", "old")]);
    assert!(recover_or_clear_backup(&install, &install.join("quill"), &backup).is_err());
    assert_eq!(read(&backup.join("quill")), "old");
    assert!(install.join("lib/libtdjson.so").exists());
    fs::remove_dir_all(root).unwrap();
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn the_atomic_exchange_swaps_two_directories() {
    let root = scratch("exchange");
    let (a, b) = (root.join("a"), root.join("b"));
    tree(&a, &[("f", "a")]);
    tree(&b, &[("f", "b")]);
    assert!(matches!(exchange(&a, &b).unwrap(), Exchange::Done));
    assert_eq!(read(&a.join("f")), "b");
    assert_eq!(read(&b.join("f")), "a");
    // A missing side is an error, not a silent fallback.
    assert!(exchange(&a, &root.join("absent")).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn interrupted_update_entries_are_cleared() {
    let root = scratch("temp-entries");
    tree(
        &root,
        &[
            (".quill-update-1-0.2.0-5.new/quill", "x"),
            (".quill-update-1-0.2.0-5.archive", "x"),
            (".quill-update-1-0.2.0-5.json", "x"),
            (".quill-update-failed-1-2-0/quill", "x"),
            (".quill-leftovers", "x"),
            ("quill/quill", "x"),
            ("notes.txt", "x"),
        ],
    );
    assert_eq!(clear_temp_entries(&root), 4);
    assert_eq!(names(&root), [".quill-leftovers", "notes.txt", "quill"]);
    assert_eq!(clear_temp_entries(&root), 0);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn swap_files_moves_new_files_in_and_lists_the_old_ones_for_startup() {
    let root = scratch("files-ok");
    let (install, staged) = (root.join("quill"), root.join("new"));
    tree(
        &install,
        &[
            ("quill.exe", "old"),
            ("tdjson.dll", "old-td"),
            ("keep.txt", "k"),
        ],
    );
    tree(
        &staged,
        &[
            ("quill.exe", "new"),
            ("tdjson.dll", "new-td"),
            ("licenses/a.txt", "a"),
        ],
    );
    swap_files(&install, &staged, || Ok(())).unwrap();
    assert_eq!(read(&install.join("quill.exe")), "new");
    assert_eq!(read(&install.join("tdjson.dll")), "new-td");
    assert_eq!(read(&install.join("licenses/a.txt")), "a");
    assert_eq!(read(&install.join("keep.txt")), "k");
    assert_eq!(read(&install.join("quill.exe.old")), "old");
    assert_eq!(
        read(&install.join(LEFTOVERS_FILE)),
        "quill.exe.old\ntdjson.dll.old\n"
    );
    assert!(!staged.exists());
    // The next start deletes exactly those and the list itself.
    assert_eq!(remove_leftovers(&install), 2);
    assert_eq!(
        names(&install),
        ["keep.txt", "licenses", "quill.exe", "tdjson.dll"]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn swap_files_rolls_back_everything_when_launch_fails() {
    let root = scratch("files-rollback");
    let (install, staged) = (root.join("quill"), root.join("new"));
    tree(&install, &[("quill.exe", "old"), ("tdjson.dll", "old-td")]);
    tree(
        &staged,
        &[
            ("quill.exe", "new"),
            ("tdjson.dll", "new-td"),
            ("extra.dll", "x"),
        ],
    );
    let result = swap_files(&install, &staged, || Err(io::Error::other("no")));
    assert_eq!(result.unwrap_err().to_string(), "no");
    assert_eq!(read(&install.join("quill.exe")), "old");
    assert_eq!(read(&install.join("tdjson.dll")), "old-td");
    assert!(!install.join("extra.dll").exists());
    assert!(!install.join("quill.exe.old").exists());
    // The list named asides that went back; the next start just drops it.
    assert_eq!(remove_leftovers(&install), 0);
    assert_eq!(names(&install), ["quill.exe", "tdjson.dll"]);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn swap_files_never_leaves_a_file_missing_when_its_rollback_fails() {
    let root = scratch("files-rollback-fails");
    let (install, staged) = (root.join("quill"), root.join("new"));
    tree(&install, &[("quill.exe", "old")]);
    tree(&staged, &[("quill.exe", "new")]);
    let aside = install.join("quill.exe.old");
    let result = swap_files(&install, &staged, || {
        // The old file disappears, so it cannot be put back.
        fs::remove_file(&aside)?;
        Err(io::Error::other("no"))
    });
    let message = result.unwrap_err().to_string();
    assert!(message.starts_with("no; restoring"), "{message}");
    assert_eq!(read(&install.join("quill.exe")), "new");
    assert!(
        !names(&install).iter().any(|n| n.contains("rollback")),
        "{:?}",
        names(&install)
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn swap_files_picks_a_fresh_name_when_a_leftover_cannot_be_cleared() {
    let root = scratch("files-aside");
    let install = root.join("quill");
    tree(&install, &[("quill.exe", "old")]);
    // A leftover directory named like the aside file cannot be removed with
    // remove_file, standing in for a locked file.
    fs::create_dir_all(install.join("quill.exe.old")).unwrap();
    let aside = aside_name(&install.join("quill.exe")).unwrap();
    assert_eq!(aside, install.join("quill.exe.1.old"));
    assert_eq!(
        parking_name(&install.join("quill.exe")).unwrap(),
        install.join("quill.exe.rollback-0.old")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn startup_cleanup_removes_only_listed_leftovers() {
    let root = scratch("cleanup");
    tree(
        &root,
        &[
            ("quill.exe", "x"),
            ("quill.exe.old", "x"),
            ("tdjson.dll.1.old", "x"),
            ("licenses/deep/ffmpeg.dll.old", "x"),
            ("notes.old", "mine"),
        ],
    );
    // A folder that was never updated keeps its own `.old` files.
    assert_eq!(remove_leftovers(&root), 0);
    assert!(root.join("quill.exe.old").exists());
    record_leftovers(
        &root,
        &[
            root.join("quill.exe.old"),
            root.join("tdjson.dll.1.old"),
            root.join("licenses/deep/ffmpeg.dll.old"),
        ],
    )
    .unwrap();
    assert_eq!(remove_leftovers(&root), 3);
    assert_eq!(names(&root), ["licenses", "notes.old", "quill.exe"]);
    assert_eq!(remove_leftovers(&root), 0);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn leftover_list_entries_cannot_point_outside_the_install() {
    let root = scratch("cleanup-escape");
    let install = root.join("quill");
    tree(
        &root,
        &[
            ("outside.old", "x"),
            ("quill/inside.old", "x"),
            ("quill/a.txt", "x"),
        ],
    );
    fs::write(
        install.join(LEFTOVERS_FILE),
        "../outside.old\n/tmp/abs.old\nsub\\x.old\nC:/x.old\n./inside.old\na.txt\n\ninside.old\n",
    )
    .unwrap();
    assert_eq!(remove_leftovers(&install), 1);
    assert!(root.join("outside.old").exists());
    assert!(install.join("a.txt").exists());
    assert!(!install.join("inside.old").exists());
    assert!(!install.join(LEFTOVERS_FILE).exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn writable_probe_and_managed_paths() {
    let root = scratch("probe");
    assert!(dir_writable(&root));
    assert!(!dir_writable(&root.join("missing")));
    assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
    assert!(distro_managed(Path::new("/usr/lib/quill")));
    assert!(distro_managed(Path::new("/tmp/.mount_Quill1/usr")));
    assert!(!distro_managed(Path::new("/home/me/.local/opt/quill")));
    assert!(distro_managed(Path::new(
        r"C:\Users\me\AppData\Local\Microsoft\WinGet\Packages\shinycake.Quill_abc\quill"
    )));
    assert!(distro_managed(Path::new(
        r"C:\Users\me\scoop\apps\quill\1.0.0"
    )));
    assert!(distro_managed(Path::new(
        r"C:\ProgramData\scoop\apps\quill\current"
    )));
    assert!(!distro_managed(Path::new(
        r"C:\Users\me\AppData\Local\Quill"
    )));
    assert!(!distro_managed(Path::new(r"D:\scoop-notes\quill")));
    fs::remove_dir_all(root).unwrap();
}

/// Opens `path` the way an antivirus scanner or indexer might: sharing read
/// access only, so renaming or deleting it fails with a sharing violation
/// until the handle closes.
#[cfg(windows)]
fn hold(path: &Path) -> fs::File {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_SHARE_READ: u32 = 0x1;
    fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(path)
        .unwrap()
}

#[cfg(windows)]
#[test]
fn windows_swap_files_retries_while_a_file_is_briefly_held() {
    let root = scratch("win-retry");
    let (install, staged) = (root.join("quill"), root.join("new"));
    tree(&install, &[("quill.exe", "old"), ("tdjson.dll", "old-td")]);
    tree(&staged, &[("quill.exe", "new"), ("tdjson.dll", "new-td")]);
    let held = hold(&install.join("quill.exe"));
    // A plain rename fails while the handle is open.
    let probe = fs::rename(install.join("quill.exe"), install.join("probe"));
    assert_eq!(probe.unwrap_err().raw_os_error(), Some(32));
    let releaser = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(700));
        drop(held);
    });
    swap_files(&install, &staged, || Ok(())).unwrap();
    releaser.join().unwrap();
    assert_eq!(read(&install.join("quill.exe")), "new");
    assert_eq!(read(&install.join("tdjson.dll")), "new-td");
    assert_eq!(remove_leftovers(&install), 2);
    fs::remove_dir_all(root).unwrap();
}

#[cfg(windows)]
#[test]
fn windows_swap_files_rolls_back_when_a_file_stays_held() {
    let root = scratch("win-held");
    let (install, staged) = (root.join("quill"), root.join("new"));
    tree(&install, &[("a.dll", "old-a"), ("quill.exe", "old")]);
    tree(&staged, &[("a.dll", "new-a"), ("quill.exe", "new")]);
    let held = hold(&install.join("quill.exe"));
    let result = swap_files(&install, &staged, || Ok(()));
    drop(held);
    assert_eq!(result.unwrap_err().raw_os_error(), Some(32));
    // `a.dll` went in first and came back out.
    assert_eq!(read(&install.join("a.dll")), "old-a");
    assert_eq!(read(&install.join("quill.exe")), "old");
    assert!(staged.join("quill.exe").exists());
    let left = names(&install);
    assert!(!left.iter().any(|n| n.contains("rollback")), "{left:?}");
    fs::remove_dir_all(root).unwrap();
}

#[cfg(windows)]
#[test]
fn windows_held_leftovers_stay_listed_until_they_can_go() {
    let root = scratch("win-leftover");
    tree(&root, &[("quill.exe.old", "x")]);
    record_leftovers(&root, &[root.join("quill.exe.old")]).unwrap();
    let held = hold(&root.join("quill.exe.old"));
    assert_eq!(remove_leftovers(&root), 0);
    assert!(root.join(LEFTOVERS_FILE).exists());
    drop(held);
    assert_eq!(remove_leftovers(&root), 1);
    assert!(!root.join(LEFTOVERS_FILE).exists());
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn swap_dir_leaves_both_trees_intact_when_the_backup_cannot_be_made() {
    // The backup folder cannot be written (App Management on macOS 13+
    // refuses the rename with EPERM in the same way): the exchange path swaps
    // back, the plain path fails before its second rename. Nothing moved.
    use std::os::unix::fs::PermissionsExt;
    for (tag, strategy) in STRATEGIES {
        let root = scratch(&format!("dir-no-backup-{tag}"));
        let (install, staged) = (root.join("quill"), root.join("new"));
        let closed = root.join("closed");
        fs::create_dir_all(&closed).unwrap();
        tree(&install, &[("quill", "old")]);
        tree(&staged, &[("quill", "new")]);
        fs::set_permissions(&closed, fs::Permissions::from_mode(0o555)).unwrap();
        if dir_writable(&closed) {
            // Root writes anywhere; nothing to test here.
            fs::remove_dir_all(root).unwrap();
            continue;
        }
        let result = swap_dir_with(
            &install,
            &staged,
            &closed.join("bak"),
            || panic!("{tag}: nothing to launch"),
            strategy,
        );
        assert!(result.is_err(), "{tag}");
        assert_eq!(read(&install.join("quill")), "old", "{tag}");
        assert_eq!(read(&staged.join("quill")), "new", "{tag}");
        fs::set_permissions(&closed, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(names(&root), ["closed", "new", "quill"], "{tag}");
        fs::remove_dir_all(root).unwrap();
    }
}
