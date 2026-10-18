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
fn the_bundle_is_found_from_its_executable_and_the_receipt_sits_beside_it() {
    let root = scratch("layout");
    let bundle = root.join("Quill.app");
    fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
    let exe = bundle.join("Contents/MacOS/quill");
    fs::write(&exe, b"x").unwrap();
    assert_eq!(bundle_of(&exe), None, "no Info.plist yet");
    fs::write(bundle.join("Contents/Info.plist"), b"<plist/>").unwrap();
    assert_eq!(bundle_of(&exe), Some(bundle.clone()));
    assert_eq!(
        receipt_for(&exe),
        Some(root.join(".Quill.app.last-update.json"))
    );
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
