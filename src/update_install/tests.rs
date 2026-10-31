use super::*;

#[test]
fn only_invalid_parameter_means_the_parent_is_gone() {
    assert!(open_failure_means_gone(87));
    assert!(!open_failure_means_gone(5));
    assert!(!open_failure_means_gone(0));
}

#[test]
fn unix_liveness_sees_self_and_not_a_missing_pid() {
    assert!(unix_process_alive(std::process::id()));
    assert!(!unix_process_alive(i32::MAX as u32));
    assert!(!unix_process_alive(u32::MAX));
}

#[test]
fn digest_file_is_lowercase_hex_sha256() {
    let path = std::env::temp_dir().join(format!("quill-digest-{}", std::process::id()));
    fs::write(&path, b"abc").unwrap();
    let digest = digest_file(&path).unwrap();
    let _ = fs::remove_file(&path);
    assert_eq!(
        digest,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn the_version_probe_checks_both_answers_and_times_out() {
    use std::os::unix::process::ExitStatusExt;
    let new = fixture("new.sh");
    assert!(probe_binary(&new, "999.0.0").is_ok());
    assert!(probe_binary(&new, "999.0.1").is_err());
    assert!(
        probe_binary(&fixture("core-only.sh"), "999.0.0").is_err(),
        "a core-only build"
    );
    // A child that cannot finish by itself (`cat` waits for the end of an
    // input this test holds open), with no time left: killed and reported.
    let mut stuck = Command::new("cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let waited = wait_with_timeout(&mut stuck, Duration::ZERO);
    assert_eq!(waited.unwrap_err().kind(), std::io::ErrorKind::TimedOut);
    let status = stuck.try_wait().unwrap();
    assert_eq!(
        status.and_then(|status| status.signal()),
        Some(libc::SIGKILL)
    );
}

#[test]
fn every_reason_has_its_own_hint_and_demo_name() {
    let all = [
        Unavailable::NotPackaged,
        Unavailable::Translocated,
        Unavailable::DiskImage,
        Unavailable::ReadOnly,
        Unavailable::PackageManager,
        Unavailable::NotWritable,
    ];
    let mut hints: Vec<_> = all.iter().map(|why| why.hint()).collect();
    hints.dedup();
    assert_eq!(hints.len(), all.len());
    for (name, why) in [
        ("0", Unavailable::NotPackaged),
        ("translocated", Unavailable::Translocated),
        ("disk-image", Unavailable::DiskImage),
        ("read-only", Unavailable::ReadOnly),
        ("package-manager", Unavailable::PackageManager),
        ("not-writable", Unavailable::NotWritable),
    ] {
        assert_eq!(Unavailable::from_demo_name(name), Some(why));
    }
    assert_eq!(Unavailable::from_demo_name("1"), None);
}

fn release() -> ReleaseInfo {
    let base = crate::updater::RELEASES_URL;
    ReleaseInfo {
        version: "999.0.0".into(),
        notes: String::new(),
        url: format!("{base}/tag/v999.0.0"),
        asset: Some(crate::updater::ReleaseAsset {
            url: format!(
                "{base}/download/v999.0.0/{}",
                crate::updater::update_asset_name()
            ),
            size: 100,
            sha256: "a".repeat(64),
        }),
    }
}

/// A pid that is certainly not running (spawned and reaped).
fn exited_pid() -> u32 {
    let mut child = Command::new("/bin/sh")
        .args(["-c", "exit 0"])
        .spawn()
        .unwrap();
    let pid = child.id();
    child.wait().unwrap();
    pid
}

/// A stand-in app from `tests/fixtures/update-install`, checked in with its
/// executable bit.
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/update-install")
        .join(name)
}

/// What a fixture app answers: the probes, or records its arguments.
#[derive(Clone, Copy)]
enum App {
    /// The new version: answers the probes, and records what it was started
    /// with in `started` beside the install.
    New,
    /// The old version: records what it was started with.
    Old,
    /// A new version whose probe answer is wrong.
    BadProbe,
}

impl App {
    #[cfg(target_os = "macos")]
    fn version(self) -> &'static str {
        match self {
            App::New => "999.0.0",
            App::Old | App::BadProbe => "0.0.1",
        }
    }
}

/// Runs the fixture executable itself (the helper's real starter goes
/// through LaunchServices on macOS, which a test bundle must not) until it
/// exits, so what it records is complete once the helper returns.
fn start_directly(target: &Path, args: &[String]) -> std::io::Result<()> {
    Command::new(target)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|_| ())
}

struct Fixture {
    root: PathBuf,
    install: PathBuf,
    staged: PathBuf,
    plan_path: PathBuf,
    target: PathBuf,
    /// The old executable's bytes, to prove a rollback.
    old_exe: Vec<u8>,
}

impl Fixture {
    /// The plan as `download_and_stage` wrote it, taken over by this
    /// process as the helper.
    fn take(&self, parent: u32, args: &[&str]) -> (claim::Claim, InstallPlan) {
        let plan = InstallPlan {
            target: self.target.clone(),
            staged: self.staged.clone(),
            parent,
            release: release(),
            bundle: true,
            args: args.iter().map(|arg| arg.to_string()).collect(),
            files: manifest::of(&self.staged).unwrap(),
        };
        fs::write(&self.plan_path, serde_json::to_vec(&plan).unwrap()).unwrap();
        claim::take_over(&self.plan_path).ok().unwrap()
    }

    /// What the started version got. [`start_directly`] waited for it to
    /// exit, so the file holds all it wrote (a poll for the file while it
    /// ran could read it created and still empty).
    fn started(&self) -> Option<String> {
        fs::read_to_string(self.root.join("opt/started"))
            .ok()
            .map(|args| args.trim().to_string())
    }

    /// No update file is left where plans live.
    fn no_claim_left(&self) -> bool {
        fs::read_dir(self.plan_path.parent().unwrap())
            .unwrap()
            .flatten()
            .all(|entry| claim::kind_of(&entry.path()).is_none())
    }

    fn receipt(&self) -> InstallResult {
        serde_json::from_slice(&fs::read(receipt_for(&self.target)).unwrap()).unwrap()
    }

    fn backup(&self) -> PathBuf {
        Layout::of(&self.target).unwrap().backup().unwrap()
    }

    /// A file of the package other than the executable.
    fn payload(&self, package: &Path) -> PathBuf {
        if cfg!(target_os = "macos") {
            package.join("Contents/Resources/payload")
        } else {
            package.join("lib/libtdjson.so")
        }
    }
}

/// Writes the fixture app `app` as the package at `package`. Linux: the
/// `quill` script beside `lib/libtdjson.so`. macOS: a real bundle with a
/// compiled executable, ad-hoc signed, so the signature chain runs for real.
fn write_app(package: &Path, app: App, payload: &[u8]) {
    #[cfg(not(target_os = "macos"))]
    {
        let script = fixture(match app {
            App::New => "new.sh",
            App::Old => "old.sh",
            App::BadProbe => "bad-probe.sh",
        });
        fs::create_dir_all(package.join("lib")).unwrap();
        // Copied by `cp`, not written by this process: a child another test
        // thread is starting shares this process's open files until its
        // exec, and an executable still open for writing anywhere fails to
        // start with `ETXTBSY` ("Text file busy").
        let copied = Command::new("cp")
            .arg(script)
            .arg(package.join("quill"))
            .status()
            .unwrap();
        assert!(copied.success());
        fs::write(package.join("lib/libtdjson.so"), payload).unwrap();
    }
    #[cfg(target_os = "macos")]
    {
        macos::tests::write_bundle(package, app.version(), payload, |exe| {
            let (version, record) = match app {
                App::New => ("Quill 999.0.0", true),
                App::Old => ("Quill 0.0.1", true),
                App::BadProbe => ("Quill 0.0.1", false),
            };
            macos::tests::compile_app(exe, version, record);
        });
    }
}

/// An installed package `opt/quill` (`opt/Quill.app`) and a staged one
/// beside it, as `download_and_stage` leaves them.
fn bundle_fixture(tag: &str, old_app: App, new_app: App) -> Fixture {
    let root = std::env::temp_dir().join(format!("quill-apply-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("opt")).unwrap();
    let root = fs::canonicalize(root).unwrap();
    let install = if cfg!(target_os = "macos") {
        root.join("opt/Quill.app")
    } else {
        root.join("opt/quill")
    };
    write_app(&install, old_app, b"old-td");
    let target = package_exe(&install);
    // The stage folder: beside the package on Linux, the private update
    // folder on macOS (created closed to other users, as staging does).
    let layout = Layout::of(&target).unwrap();
    ensure_private_dir(&layout.stage_root).unwrap();
    let staged = layout.stage_root.join(".quill-update-1-999.0.0-1.new");
    write_app(&staged, new_app, b"new-td");
    let plan_path = layout.plans.join(".quill-update-1-999.0.0-1.json");
    let old_exe = fs::read(&target).unwrap();
    Fixture {
        root,
        install,
        staged,
        plan_path,
        target,
        old_exe,
    }
}

#[test]
fn apply_plan_installs_a_bundle_and_keeps_the_old_tree_until_the_next_start() {
    let fx = bundle_fixture("bundle-ok", App::Old, App::New);
    // A backup an earlier update never cleared must not block this one.
    let backup = fx.backup();
    fs::create_dir_all(backup.join("stale")).unwrap();
    let (claim, plan) = fx.take(
        exited_pid(),
        &["--start-minimized", "--", "tg://resolve?domain=a"],
    );
    apply_plan(claim, plan, fx.target.clone(), start_directly).unwrap();
    assert!(probe_binary(&fx.target, "999.0.0").is_ok());
    assert_eq!(fs::read(fx.payload(&fx.install)).unwrap(), b"new-td");
    let result = fx.receipt();
    assert!(!result.failed);
    assert_eq!(result.release.version, "999.0.0");
    assert!(!fx.staged.exists() && fx.no_claim_left());
    // The new version starts with the launch's flags and links.
    assert_eq!(
        fx.started().as_deref(),
        Some("--start-minimized -- tg://resolve?domain=a")
    );
    // The previous tree waits for the new version's first start.
    assert_eq!(fs::read(fx.payload(&backup)).unwrap(), b"old-td");
    assert!(!backup.join("stale").exists());
    swap::recover_or_clear_backup(&fx.install, &fx.target, &backup).unwrap();
    assert!(!backup.exists());
    fs::remove_dir_all(fx.root).unwrap();
}

/// The old version is started again with the launch's arguments, the
/// failure is on record for it, and nothing is left to try again.
fn assert_failed_and_old_version_restarted(fx: &Fixture) {
    assert_eq!(fs::read(&fx.target).unwrap(), fx.old_exe);
    assert_eq!(fs::read(fx.payload(&fx.install)).unwrap(), b"old-td");
    assert!(fx.receipt().failed);
    assert!(!fx.staged.exists() && fx.no_claim_left());
    assert!(!fx.backup().exists());
    assert_eq!(fx.started().as_deref(), Some("--start-minimized"));
}

#[test]
fn apply_plan_keeps_the_old_bundle_and_reports_when_the_new_one_fails_its_probe() {
    let fx = bundle_fixture("bundle-bad", App::Old, App::BadProbe);
    let (claim, plan) = fx.take(exited_pid(), &["--start-minimized"]);
    assert!(apply_plan(claim, plan, fx.target.clone(), start_directly).is_err());
    assert_failed_and_old_version_restarted(&fx);
    fs::remove_dir_all(&fx.root).unwrap();
}

#[test]
fn a_staged_package_with_a_missing_or_changed_file_is_not_installed() {
    for (tag, missing) in [("bundle-missing", true), ("bundle-corrupt", false)] {
        let fx = bundle_fixture(tag, App::Old, App::New);
        let (claim, plan) = fx.take(exited_pid(), &["--start-minimized"]);
        let library = fx.payload(&fx.staged);
        if missing {
            fs::remove_file(&library).unwrap();
        } else {
            fs::write(&library, b"bad-td").unwrap();
        }
        assert!(
            apply_plan(claim, plan, fx.target.clone(), start_directly).is_err(),
            "{tag}"
        );
        assert_failed_and_old_version_restarted(&fx);
        fs::remove_dir_all(&fx.root).unwrap();
    }
}

/// A file changed after the manifest was taken: the signature check (macOS)
/// or the manifest refuses it, and the old version keeps running.
#[test]
fn a_tampered_package_fails_the_manifest_and_the_signature_check() {
    let fx = bundle_fixture("bundle-tampered", App::Old, App::New);
    let (claim, mut plan) = fx.take(exited_pid(), &["--start-minimized"]);
    let payload = fx.payload(&fx.staged);
    fs::write(&payload, b"evil-td").unwrap();
    assert!(manifest::verify(&fx.staged, &plan.files).is_err());
    // Even with a manifest that blesses the change, the package is refused:
    // on macOS by the broken seal, elsewhere the manifest is the last word
    // and this asserts the swap never ran for a bad manifest above.
    plan.files = manifest::of(&fx.staged).unwrap();
    let layout = Layout::of(&fx.target).unwrap();
    let verified = verify_staged(&layout, &fx.staged, &plan.release);
    if cfg!(target_os = "macos") {
        assert!(verified.is_err(), "a changed file breaks the seal");
        assert!(apply_plan(claim, plan, fx.target.clone(), start_directly).is_err());
        assert_failed_and_old_version_restarted(&fx);
    } else {
        assert!(verified.is_ok(), "no signature to check here");
        claim.release();
    }
    fs::remove_dir_all(&fx.root).unwrap();
}

#[test]
fn a_handoff_refused_before_the_install_still_consumes_the_plan_and_restarts_quill() {
    // The plan names another executable: refused before any waiting.
    let fx = bundle_fixture("bundle-foreign", App::Old, App::New);
    let (claim, mut plan) = fx.take(exited_pid(), &["--start-minimized"]);
    plan.target = fx.root.join("elsewhere/quill");
    assert!(apply_plan(claim, plan, fx.target.clone(), start_directly).is_err());
    assert_failed_and_old_version_restarted(&fx);
    fs::remove_dir_all(&fx.root).unwrap();
}

#[test]
fn a_second_helper_on_the_same_plan_does_nothing() {
    let fx = bundle_fixture("bundle-twice", App::Old, App::New);
    let (claim, _plan) = fx.take(exited_pid(), &[]);
    // The ready plan is gone; the other helper's argument names it.
    assert!(matches!(
        claim::take_over(&fx.plan_path),
        Err(claim::Refused::Taken(_))
    ));
    assert!(fx.staged.exists());
    assert!(!receipt_for(&fx.target).exists());
    claim.release();
    fs::remove_dir_all(&fx.root).unwrap();
}

#[test]
fn a_claim_the_helper_cannot_rename_still_ends_with_quill_started() {
    let fx = bundle_fixture("bundle-busy", App::Old, App::New);
    let (claim, _) = fx.take(exited_pid(), &["--start-minimized"]);
    // Back to a `.claim` the launch handed over, then a rename that
    // keeps failing the way a file held by a scanner does.
    let claim_path = fx.plan_path.with_extension("claim");
    fs::rename(claim.path(), &claim_path).unwrap();
    drop(claim);
    let taken = claim::take_over_with(&claim_path, |_, _| {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "in use",
        ))
    });
    assert!(apply_claimed(taken, fx.target.clone(), start_directly).is_err());
    assert_failed_and_old_version_restarted(&fx);
    fs::remove_dir_all(&fx.root).unwrap();
}

#[test]
fn staging_first_clears_what_an_interrupted_update_left() {
    let fx = bundle_fixture("bundle-clear", App::Old, App::New);
    fs::write(&fx.plan_path, b"{}").unwrap();
    let backup = fx.backup();
    fs::create_dir_all(&backup).unwrap();
    let layout = Layout::of(&fx.target).unwrap();
    let archive = layout.stage_root.join(".quill-update-2-1.0.0-3.archive");
    fs::write(&archive, b"x").unwrap();
    clear_interrupted_update(&fx.target, &layout);
    assert!(!backup.exists() && !fx.staged.exists() && !fx.plan_path.exists());
    assert!(!archive.exists());
    assert!(fx.target.exists() && fx.payload(&fx.install).exists());
    fs::remove_dir_all(fx.root).unwrap();
}

#[test]
fn only_a_writable_unmanaged_package_can_self_update() {
    use std::os::unix::fs::PermissionsExt;
    let fx = bundle_fixture("bundle-installable", App::Old, App::New);
    // A bare binary outside the package layout (cargo build).
    let bare = fx.root.join("quill");
    fs::write(&bare, b"x").unwrap();
    assert_eq!(installable(&bare), Err(Unavailable::NotPackaged));
    assert_eq!(installable(&fx.target), Ok(()));
    // A renamed executable is not our package layout.
    let renamed = fx.target.with_file_name("quill-dev");
    fs::write(&renamed, b"x").unwrap();
    assert_eq!(installable(&renamed), Err(Unavailable::NotPackaged));
    fs::remove_file(&renamed).unwrap();
    // A read-only parent: the package cannot be swapped. (Root can write
    // anywhere, so only assert when the probe agrees.)
    let parent = fx.root.join("opt");
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o555)).unwrap();
    if !swap::dir_writable(&parent) {
        assert_eq!(installable(&fx.target), Err(Unavailable::NotWritable));
    }
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o755)).unwrap();
    // Linux: a read-only install directory too (Windows and Linux rename
    // inside it; macOS only renames the bundle as a whole).
    if cfg!(target_os = "linux") {
        fs::set_permissions(&fx.install, fs::Permissions::from_mode(0o555)).unwrap();
        if !swap::dir_writable(&fx.install) {
            assert_eq!(installable(&fx.target), Err(Unavailable::NotWritable));
        }
        fs::set_permissions(&fx.install, fs::Permissions::from_mode(0o755)).unwrap();
    }
    // An update folder another user could write to is not trusted.
    fs::set_permissions(&layout_of(&fx).plans, fs::Permissions::from_mode(0o777)).unwrap();
    assert_eq!(installable(&fx.target), Err(Unavailable::OwnedByAnother));
    fs::set_permissions(&layout_of(&fx).plans, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(installable(&fx.target), Ok(()));
    fs::remove_dir_all(fx.root).unwrap();
}

fn layout_of(fx: &Fixture) -> Layout {
    Layout::of(&fx.target).unwrap()
}

#[test]
fn only_this_users_closed_files_and_folders_are_trusted() {
    use std::os::unix::fs::MetadataExt;
    use std::os::unix::fs::PermissionsExt;
    let root = std::env::temp_dir().join(format!("quill-trusted-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let file = root.join(".quill-update-1.json");
    write_private(&file, b"{}").unwrap();
    assert!(trusted_file(&file));
    let me = fs::metadata(&file).unwrap().uid();
    // Another owner, or a group or world write bit, and the file is ignored.
    assert!(!trusted_file_as(
        &fs::metadata(&file).unwrap(),
        me.wrapping_add(1)
    ));
    fs::set_permissions(&file, fs::Permissions::from_mode(0o664)).unwrap();
    assert!(!trusted_file(&file));
    fs::set_permissions(&file, fs::Permissions::from_mode(0o602)).unwrap();
    assert!(!trusted_file(&file));
    fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(trusted_file(&file));
    // A link is never a trusted file, whatever it points at.
    let link = root.join(".quill-update-2.json");
    std::os::unix::fs::symlink(&file, &link).unwrap();
    assert!(!trusted_file(&link));
    // Folders: this user's, closed to world writes (group write stays
    // allowed, a private-group umask makes 775 folders the norm).
    assert!(trusted_dir(&root));
    assert!(!trusted_dir_as(
        &fs::metadata(&root).unwrap(),
        me.wrapping_add(1)
    ));
    fs::set_permissions(&root, fs::Permissions::from_mode(0o775)).unwrap();
    assert!(trusted_dir(&root));
    fs::set_permissions(&root, fs::Permissions::from_mode(0o777)).unwrap();
    assert!(!trusted_dir(&root));
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(!trusted_dir(&file));
    // A private folder is created closed, and one left open is refused.
    let private = root.join(".quill-updates-test");
    ensure_private_dir(&private).unwrap();
    assert_eq!(fs::metadata(&private).unwrap().mode() & 0o777, 0o700);
    assert!(ensure_private_dir(&private).is_ok());
    fs::set_permissions(&private, fs::Permissions::from_mode(0o777)).unwrap();
    assert!(ensure_private_dir(&private).is_err());
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_release_refused_as_signed_differently_is_not_downloaded_again() {
    let fx = bundle_fixture("bundle-refused", App::Old, App::New);
    let layout = layout_of(&fx);
    assert!(!refused_before(&layout, "999.0.0"));
    remember_refused(&layout, "999.0.0");
    assert!(refused_before(&layout, "999.0.0"));
    assert!(!refused_before(&layout, "999.0.1"));
    // Only this user's own note counts.
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(refused_path(&layout), fs::Permissions::from_mode(0o666)).unwrap();
    assert!(!refused_before(&layout, "999.0.0"));
    fs::remove_dir_all(fx.root).unwrap();
}

#[test]
fn the_update_receipt_is_read_once() {
    let path = std::env::temp_dir().join(format!("quill-receipt-{}.json", std::process::id()));
    let _ = fs::remove_file(&path);
    write_result(path.clone(), &release(), false).unwrap();
    let first = take_receipt(&path).expect("the first start reads the receipt");
    assert_eq!(first.release.version, release().version);
    assert!(!first.failed);
    assert!(!path.exists());
    assert!(take_receipt(&path).is_none());
}
