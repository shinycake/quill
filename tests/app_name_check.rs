//! scripts/check-app-name.sh on fixture trees: app-meaning "Telegram" copy
//! fails, service-meaning copy and comments pass, the allowlist works.
#![cfg(unix)]

use std::path::PathBuf;
use std::process::Command;

fn run(dir: &str, allow: &str) -> (bool, String) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("bash")
        .current_dir(&root)
        .arg("scripts/check-app-name.sh")
        .args(["-a", allow, dir])
        .output()
        .expect("run check-app-name.sh");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

const ALLOW: &str = "tests/fixtures/app-name/allow.txt";

#[test]
fn flags_app_meaning_phrases_outside_comments() {
    let (ok, out) = run("tests/fixtures/app-name/bad", ALLOW);
    assert!(!ok, "{out}");
    assert!(out.contains("quit Telegram"), "{out}");
    assert!(out.contains("Telegram will"), "{out}");
    assert!(out.contains("Telegram Desktop needs"), "{out}");
    assert!(!out.contains("Comment mentioning"), "{out}");
    assert!(out.contains("Refer to the app as Quill"), "{out}");
}

#[test]
fn passes_service_copy_and_honours_allowlist() {
    let (ok, out) = run("tests/fixtures/app-name/good", ALLOW);
    assert!(ok, "{out}");
    let (ok, _) = run("tests/fixtures/app-name/good", "/nonexistent");
    assert!(!ok, "without the allowlist 'Telegram will call' must fail");
}

#[test]
fn repository_sources_pass() {
    let (ok, out) = run("src", "scripts/app-name-allowlist.txt");
    assert!(ok, "{out}");
}
