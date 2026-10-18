use super::codesign::{Facts, Rejection, Signer, judge, verify};
use super::*;
use std::process::Command;

/// Writes an ad-hoc signed bundle at `bundle` with `compile` producing its
/// executable, `version` in its `Info.plist` and `payload` as a resource.
pub(crate) fn write_bundle(
    bundle: &Path,
    version: &str,
    payload: &[u8],
    compile: impl FnOnce(&Path),
) {
    write_bundle_with_id(
        bundle,
        "org.shinycake.quill.updater-test",
        version,
        payload,
        compile,
    );
}

pub(crate) fn write_bundle_with_id(
    bundle: &Path,
    identifier: &str,
    version: &str,
    payload: &[u8],
    compile: impl FnOnce(&Path),
) {
    let contents = bundle.join("Contents");
    fs::create_dir_all(contents.join("MacOS")).unwrap();
    fs::create_dir_all(contents.join("Resources")).unwrap();
    fs::write(
        contents.join("Info.plist"),
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Quill</string>
<key>CFBundleIdentifier</key><string>{identifier}</string>
<key>CFBundleVersion</key><string>{version}</string>
<key>CFBundleShortVersionString</key><string>{version}</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleExecutable</key><string>quill</string>
</dict></plist>
"#
        ),
    )
    .unwrap();
    fs::write(contents.join("Resources/payload"), payload).unwrap();
    compile(&executable(bundle));
    sign(bundle);
}

pub(crate) fn sign(bundle: &Path) {
    let status = Command::new("/usr/bin/codesign")
        .args(["--force", "--deep", "--sign", "-"])
        .arg(bundle)
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "codesign failed for {}", bundle.display());
}

/// Compiles a fixture executable to `exe`: `--version` prints `version`,
/// `--build-info` prints `ui`, anything else is written (when `record`) to
/// `started` four directories up, beside the bundle.
pub(crate) fn compile_app(exe: &Path, version: &str, record: bool) {
    let source = exe.with_extension("c");
    fs::write(
        &source,
        format!(
            r#"#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <libgen.h>
int main(int argc, char **argv) {{
    if (argc > 1 && strcmp(argv[1], "--version") == 0) {{ puts("{version}"); return 0; }}
    if (argc > 1 && strcmp(argv[1], "--build-info") == 0) {{ puts("ui"); return 0; }}
    if (!{record}) {{ puts("{version}"); return 0; }}
    char path[4096];
    snprintf(path, sizeof path, "%s", argv[0]);
    char *dir = path;
    for (int i = 0; i < 4; i++) dir = dirname(dir);
    char started[4200];
    snprintf(started, sizeof started, "%s/started", dir);
    FILE *out = fopen(started, "w");
    if (!out) return 1;
    for (int i = 1; i < argc; i++) fprintf(out, "%s%s", i > 1 ? " " : "", argv[i]);
    fputc('\n', out);
    const char *env = getenv("QUILL_TEST_ENV");
    if (env && argc > 1 && strcmp(argv[argc - 1], "--record-env") == 0) fprintf(out, "env=%s\n", env);
    fclose(out);
    return 0;
}}
"#,
            record = i32::from(record)
        ),
    )
    .unwrap();
    let status = Command::new("/usr/bin/cc")
        .arg("-O0")
        .arg("-o")
        .arg(exe)
        .arg(&source)
        .status()
        .unwrap();
    assert!(status.success(), "cc failed");
    fs::remove_file(source).unwrap();
}

fn scratch(tag: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("quill-macos-update-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::canonicalize(root).unwrap()
}

fn facts() -> Facts {
    Facts {
        running: Some(Signer::Certificate),
        staged_valid: true,
        same_bundle_id: true,
        version_matches: true,
        satisfies_requirement: Some(true),
    }
}

#[test]
fn a_certificate_signed_copy_only_takes_a_bundle_that_satisfies_its_requirement() {
    assert_eq!(judge(&facts()), Ok(()));
    let other_team = Facts {
        satisfies_requirement: Some(false),
        ..facts()
    };
    assert!(matches!(judge(&other_team), Err(Rejection::Mismatch(_))));
    // An ad-hoc release against the owner's Apple Development install.
    let unchecked = Facts {
        satisfies_requirement: None,
        ..facts()
    };
    assert!(matches!(judge(&unchecked), Err(Rejection::Mismatch(_))));
}

#[test]
fn an_ad_hoc_copy_pins_no_identity_and_takes_any_valid_bundle() {
    let ad_hoc = Facts {
        running: Some(Signer::AdHoc),
        satisfies_requirement: None,
        ..facts()
    };
    assert_eq!(judge(&ad_hoc), Ok(()));
    let unsigned = Facts {
        running: None,
        ..ad_hoc.clone()
    };
    assert!(matches!(judge(&unsigned), Err(Rejection::Mismatch(_))));
}

#[test]
fn a_broken_signature_another_identifier_or_version_is_invalid() {
    for broken in [
        Facts {
            staged_valid: false,
            ..facts()
        },
        Facts {
            same_bundle_id: false,
            ..facts()
        },
        Facts {
            version_matches: false,
            ..facts()
        },
    ] {
        assert!(
            matches!(judge(&broken), Err(Rejection::Invalid(_))),
            "{broken:?}"
        );
    }
}

#[test]
fn the_most_useful_location_reason_wins() {
    let fine = Location {
        translocated: false,
        disk_image: false,
        read_only: false,
        package_manager: false,
    };
    assert_eq!(judge_location(fine), Ok(()));
    assert_eq!(
        judge_location(Location {
            translocated: true,
            read_only: true,
            ..fine
        }),
        Err(Unavailable::Translocated)
    );
    assert_eq!(
        judge_location(Location {
            disk_image: true,
            read_only: true,
            ..fine
        }),
        Err(Unavailable::DiskImage)
    );
    assert_eq!(
        judge_location(Location {
            read_only: true,
            ..fine
        }),
        Err(Unavailable::ReadOnly)
    );
    assert_eq!(
        judge_location(Location {
            package_manager: true,
            ..fine
        }),
        Err(Unavailable::PackageManager)
    );
}

#[test]
fn translocated_and_cask_paths_are_recognised() {
    assert!(translocated(Path::new(
        "/private/var/folders/x1/T/AppTranslocation/1F2E-3D/d/Quill.app/Contents/MacOS/quill"
    )));
    assert!(!translocated(Path::new(
        "/Applications/Quill.app/Contents/MacOS/quill"
    )));
    assert!(homebrew_cask(Path::new(
        "/opt/homebrew/Caskroom/quill/1.0/Quill.app/Contents/MacOS/quill"
    )));
    assert!(homebrew_cask(Path::new(
        "/usr/local/Caskroom/quill/1.0/Quill.app"
    )));
    assert!(!homebrew_cask(Path::new(
        "/Applications/Quill.app/Contents/MacOS/quill"
    )));
    assert!(!homebrew_cask(Path::new(
        "/Users/me/Applications/Quill.app"
    )));
}

#[test]
fn hdiutil_mount_points_are_read_from_its_plist() {
    let plist = r#"<plist><dict><key>images</key><array><dict>
<key>image-path</key><string>/Users/me/Downloads/Quill.dmg</string>
<key>system-entities</key><array>
<dict><key>dev-entry</key><string>/dev/disk5</string></dict>
<dict><key>dev-entry</key><string>/dev/disk5s1</string><key>mount-point</key><string>/Volumes/Quill &amp; Co</string></dict>
</array></dict></array></dict></plist>"#;
    assert_eq!(
        mounted_images(plist),
        vec![PathBuf::from("/Volumes/Quill & Co")]
    );
    assert!(mounted_images("<plist><dict></dict></plist>").is_empty());
}

#[test]
fn the_bundle_is_found_from_its_executable() {
    let root = scratch("layout");
    let bundle = root.join("Quill.app");
    fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
    let exe = bundle.join("Contents/MacOS/quill");
    fs::write(&exe, b"x").unwrap();
    assert_eq!(bundle_of(&exe), None, "no Info.plist yet");
    fs::write(bundle.join("Contents/Info.plist"), b"<plist/>").unwrap();
    assert_eq!(bundle_of(&exe), Some(bundle.clone()));
    assert_eq!(
        bundle_of(&bundle.join("Contents/MacOS/quill-webview")),
        None
    );
    assert_eq!(bundle_of(&root.join("quill")), None);
    assert!(!read_only_volume(&root));
    assert_eq!(check_location(&exe, &bundle), Ok(()));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn the_release_zip_unpacks_to_one_bundle_or_not_at_all() {
    let root = scratch("unpack");
    let source = root.join("src/Quill.app");
    write_bundle(&source, "1.0.0", b"p", |exe| {
        compile_app(exe, "Quill 1.0.0", false)
    });
    let archive = root.join("quill.zip");
    let status = Command::new("/usr/bin/ditto")
        .args(["-c", "-k", "--sequesterRsrc", "--keepParent"])
        .arg(&source)
        .arg(&archive)
        .status()
        .unwrap();
    assert!(status.success());
    let staged = root.join(".quill-update-1.new");
    unpack(&archive, &staged).unwrap();
    assert!(executable(&staged).is_file());
    assert!(!staged.with_extension("unpack").exists());
    assert_eq!(verify(&source, &staged, "1.0.0"), Ok(()));

    // Two top-level entries.
    let two = root.join("two.zip");
    fs::write(root.join("src/stray.txt"), b"x").unwrap();
    let status = Command::new("/usr/bin/ditto")
        .args(["-c", "-k", "--sequesterRsrc"])
        .arg(root.join("src"))
        .arg(&two)
        .status()
        .unwrap();
    assert!(status.success());
    let other = root.join(".quill-update-2.new");
    assert!(unpack(&two, &other).is_err());
    assert!(!other.exists() && !other.with_extension("unpack").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn verification_accepts_a_matching_bundle_and_refuses_a_changed_or_foreign_one() {
    let root = scratch("verify");
    let running = root.join("Quill.app");
    write_bundle(&running, "1.0.0", b"old", |exe| {
        compile_app(exe, "Quill 1.0.0", false)
    });
    let staged = root.join(".quill-update-1.new");
    write_bundle(&staged, "2.0.0", b"new", |exe| {
        compile_app(exe, "Quill 2.0.0", false)
    });
    assert_eq!(verify(&running, &staged, "2.0.0"), Ok(()));
    assert!(
        matches!(
            verify(&running, &staged, "2.0.1"),
            Err(Rejection::Invalid(_))
        ),
        "the release version is not the bundle's"
    );

    // A changed resource breaks the seal.
    fs::write(staged.join("Contents/Resources/payload"), b"evil").unwrap();
    assert!(matches!(
        verify(&running, &staged, "2.0.0"),
        Err(Rejection::Invalid(_))
    ));
    // An added file too.
    fs::write(staged.join("Contents/Resources/payload"), b"new").unwrap();
    fs::write(staged.join("Contents/Resources/extra"), b"x").unwrap();
    assert!(matches!(
        verify(&running, &staged, "2.0.0"),
        Err(Rejection::Invalid(_))
    ));
    fs::remove_file(staged.join("Contents/Resources/extra")).unwrap();
    assert_eq!(verify(&running, &staged, "2.0.0"), Ok(()));

    // Another bundle identifier.
    let foreign = root.join(".quill-update-2.new");
    write_bundle_with_id(&foreign, "org.example.other", "2.0.0", b"new", |exe| {
        compile_app(exe, "Quill 2.0.0", false)
    });
    assert!(matches!(
        verify(&running, &foreign, "2.0.0"),
        Err(Rejection::Invalid(_))
    ));

    // An unsigned running copy updates by hand.
    let unsigned = root.join("Unsigned.app");
    write_bundle(&unsigned, "1.0.0", b"old", |exe| {
        compile_app(exe, "Quill 1.0.0", false)
    });
    let status = Command::new("/usr/bin/codesign")
        .args(["--remove-signature"])
        .arg(&unsigned)
        .status()
        .unwrap();
    assert!(status.success());
    assert!(matches!(
        verify(&unsigned, &staged, "2.0.0"),
        Err(Rejection::Mismatch(_))
    ));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn quarantine_is_removed_from_every_entry_and_the_signature_survives() {
    let root = scratch("quarantine");
    let bundle = root.join("Quill.app");
    write_bundle(&bundle, "1.0.0", b"p", |exe| {
        compile_app(exe, "Quill 1.0.0", false)
    });
    let payload = bundle.join("Contents/Resources/payload");
    for path in [&bundle, &payload] {
        let status = Command::new("/usr/bin/xattr")
            .args(["-w", "com.apple.quarantine", "0083;00000000;Safari;"])
            .arg(path)
            .status()
            .unwrap();
        assert!(status.success());
    }
    let listed = |path: &Path| {
        String::from_utf8(
            Command::new("/usr/bin/xattr")
                .arg(path)
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
    };
    assert!(listed(&payload).contains("com.apple.quarantine"));
    remove_quarantine(&bundle).unwrap();
    assert!(!listed(&bundle).contains("com.apple.quarantine"));
    assert!(!listed(&payload).contains("com.apple.quarantine"));
    assert_eq!(verify(&bundle, &bundle, "1.0.0"), Ok(()));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn open_starts_the_bundle_with_its_arguments_and_environment() {
    // `launch` goes through LaunchServices for real: the fixture records
    // its arguments and `QUILL_TEST_ENV` in `started` beside the bundle.
    // That is also the proof that `open` passes the environment through,
    // which is why `launch` forwards nothing on the command line.
    let root = scratch("open");
    let bundle = root.join("Quill.app");
    write_bundle(&bundle, "1.0.0", b"x", |exe| {
        compile_app(exe, "Quill 1.0.0", true)
    });
    // SAFETY: tests in this binary never read this variable concurrently;
    // it exists for this test's child process only.
    unsafe { std::env::set_var("QUILL_TEST_ENV", "through-open") };
    launch(
        &bundle,
        &["--start-minimized".to_string(), "--record-env".to_string()],
    )
    .unwrap();
    let started = root.join("started");
    let mut text = None;
    for _ in 0..300 {
        if let Ok(read) = fs::read_to_string(&started) {
            text = Some(read);
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let text = text.expect("the bundle started through LaunchServices");
    assert_eq!(text, "--start-minimized --record-env\nenv=through-open\n");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_refused_bundle_keeps_its_quarantine_mark() {
    use super::super::{Layout, accept_staged, ensure_private_dir};
    let root = scratch("quarantine-kept");
    let running = root.join("Quill.app");
    write_bundle(&running, "1.0.0", b"old", |exe| {
        compile_app(exe, "Quill 1.0.0", false)
    });
    let layout = Layout::of(&executable(&running)).unwrap();
    ensure_private_dir(&layout.stage_root).unwrap();
    let release = |version: &str| crate::updater::ReleaseInfo {
        version: version.into(),
        notes: String::new(),
        url: format!("{}/tag/v{version}", crate::updater::RELEASES_URL),
        asset: None,
    };
    let quarantine = |path: &Path| {
        let status = Command::new("/usr/bin/xattr")
            .args(["-w", "com.apple.quarantine", "0083;00000000;Quill;"])
            .arg(path)
            .status()
            .unwrap();
        assert!(status.success());
    };
    let quarantined = |path: &Path| {
        Command::new("/usr/bin/xattr")
            .args(["-p", "com.apple.quarantine"])
            .arg(path)
            .stderr(std::process::Stdio::null())
            .output()
            .unwrap()
            .status
            .success()
    };
    // A tampered bundle fails verification and stays quarantined.
    let bad = layout.stage_root.join(".quill-update-1.new");
    write_bundle(&bad, "2.0.0", b"new", |exe| {
        compile_app(exe, "Quill 2.0.0", false)
    });
    fs::write(bad.join("Contents/Resources/payload"), b"evil").unwrap();
    quarantine(&bad);
    quarantine(&bad.join("Contents/Resources/payload"));
    assert!(accept_staged(&layout, &bad, &release("2.0.0")).is_err());
    assert!(quarantined(&bad));
    assert!(quarantined(&bad.join("Contents/Resources/payload")));
    // A good one passes and loses the mark. Only a resource carries it here:
    // running the executable of a quarantined bundle makes Gatekeeper assess
    // the bundle first, which on this host does not return within the probe's
    // 5 s (so a quarantined bundle fails the probe and is refused as well).
    let good = layout.stage_root.join(".quill-update-2.new");
    write_bundle(&good, "2.0.0", b"new", |exe| {
        compile_app(exe, "Quill 2.0.0", false)
    });
    quarantine(&good.join("Contents/Resources/payload"));
    assert_eq!(accept_staged(&layout, &good, &release("2.0.0")), Ok(()));
    assert!(!quarantined(&good.join("Contents/Resources/payload")));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn only_the_strict_nested_check_catches_a_tampered_helper() {
    // A helper executable inside the bundle keeps its own signature. Changing
    // its bytes leaves its code directory, and so the outer seal's cdhash of
    // it, untouched: only `kSecCSCheckNestedCode` validates the helper itself.
    use super::codesign::{STRICT, check, identifier_requirement, static_code};
    let root = scratch("nested");
    let bundle = root.join("Quill.app");
    write_bundle(&bundle, "1.0.0", b"x", |exe| {
        compile_app(exe, "Quill 1.0.0", false)
    });
    let helper = bundle.join("Contents/MacOS/quill-helper");
    compile_app(&helper, "Helper 7.7.7", false);
    sign(&bundle);
    // A static-code object remembers its verdict, so each check gets a new
    // one, as `verify` does.
    let requirement = identifier_requirement("org.shinycake.quill.updater-test").unwrap();
    assert_eq!(
        check(&static_code(&bundle).unwrap(), STRICT, Some(&requirement)),
        Ok(())
    );
    let mut bytes = fs::read(&helper).unwrap();
    let needle = b"Helper 7.7.7";
    let at = bytes
        .windows(needle.len())
        .position(|window| window == needle)
        .unwrap();
    bytes
        .get_mut(at..at + needle.len())
        .unwrap()
        .copy_from_slice(b"Helper 6.6.6");
    fs::write(&helper, bytes).unwrap();
    assert_eq!(
        check(&static_code(&bundle).unwrap(), 0, Some(&requirement)),
        Ok(()),
        "the default check only seals the helper's cdhash"
    );
    assert!(check(&static_code(&bundle).unwrap(), STRICT, Some(&requirement)).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_ad_hoc_signature_has_no_certificate_and_a_cdhash_requirement() {
    // The certificate side of `gather`, on what every test host has: an
    // ad-hoc bundle has no certificate chain (so it is `Signer::AdHoc`), and
    // its designated requirement is its own cdhash, which only it satisfies.
    use super::codesign::{SigningInfo, check, designated_requirement, signer, static_code};
    let root = scratch("adhoc-dr");
    let a = root.join("A.app");
    write_bundle(&a, "1.0.0", b"a", |exe| {
        compile_app(exe, "Quill 1.0.0", false)
    });
    let b = root.join("B.app");
    write_bundle(&b, "1.0.1", b"b", |exe| {
        compile_app(exe, "Quill 1.0.1", false)
    });
    let code_a = static_code(&a).unwrap();
    let info_a = SigningInfo::of(&code_a).unwrap();
    assert_eq!(info_a.certificate_count(), 0);
    assert_eq!(signer(&code_a, &info_a), Some(Signer::AdHoc));
    let requirement = designated_requirement(&code_a).unwrap();
    assert_eq!(check(&code_a, 0, Some(&requirement)), Ok(()));
    let code_b = static_code(&b).unwrap();
    assert!(check(&code_b, 0, Some(&requirement)).is_err());
    fs::remove_dir_all(root).unwrap();
}

/// A code-signing identity on this Mac, when `security` lists one and the
/// test is allowed to use it (`QUILL_TEST_CODESIGN_IDENTITY`, or the first
/// Apple Development identity). CI runners have none: the test is skipped.
fn local_identity() -> Option<String> {
    if let Ok(name) = std::env::var("QUILL_TEST_CODESIGN_IDENTITY") {
        return Some(name);
    }
    let output = Command::new("/usr/bin/security")
        .args(["find-identity", "-v", "-p", "codesigning"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines().find_map(|line| {
        let start = line.find("\"Apple Development:")? + 1;
        let end = line.get(start..)?.find('"')? + start;
        line.get(start..end).map(str::to_string)
    })
}

#[test]
fn a_certificate_signed_copy_pins_its_identity_for_real() {
    let Some(identity) = local_identity() else {
        eprintln!("skipped: no Apple Development identity on this host");
        return;
    };
    use super::codesign::{SigningInfo, signer, static_code};
    let root = scratch("cert");
    let running = root.join("Quill.app");
    write_bundle(&running, "1.0.0", b"old", |exe| {
        compile_app(exe, "Quill 1.0.0", false)
    });
    let sign_with = |bundle: &Path| {
        let status = Command::new("/usr/bin/codesign")
            .args(["--force", "--deep", "--sign", &identity])
            .arg(bundle)
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "codesign with {identity} failed");
    };
    sign_with(&running);
    let code = static_code(&running).unwrap();
    let info = SigningInfo::of(&code).unwrap();
    assert!(info.certificate_count() > 0);
    assert_eq!(signer(&code, &info), Some(Signer::Certificate));
    // The same identity's new version installs; an ad-hoc one does not.
    let same = root.join(".quill-update-1.new");
    write_bundle(&same, "2.0.0", b"new", |exe| {
        compile_app(exe, "Quill 2.0.0", false)
    });
    sign_with(&same);
    assert_eq!(verify(&running, &same, "2.0.0"), Ok(()));
    let adhoc = root.join(".quill-update-2.new");
    write_bundle(&adhoc, "2.0.0", b"new", |exe| {
        compile_app(exe, "Quill 2.0.0", false)
    });
    assert!(matches!(
        verify(&running, &adhoc, "2.0.0"),
        Err(Rejection::Mismatch(_))
    ));
    fs::remove_dir_all(root).unwrap();
}
