//! scripts/check-file-size.sh and scripts/check-hotspots.sh on fixtures:
//! size limit, baseline rules and trailing test modules; hotspot rules,
//! overrides, and the big `match` / `pub enum` warnings.
#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn scratch(name: &str) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "quill-structure-{name}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

// ---------------------------------------------------------------- file size

const SIZE: &str = "tests/fixtures/file-size";

fn size(baseline: &str, dir: &str, extra: &[&str]) -> (bool, String) {
    let out = Command::new("bash")
        .current_dir(repo())
        .arg("scripts/check-file-size.sh")
        .args(["-l", "10", "-b", baseline])
        .args(extra)
        .arg(dir)
        .output()
        .expect("run check-file-size.sh");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

#[test]
fn size_passes_small_files_test_modules_and_shrunk_baseline() {
    let (ok, out) = size(&format!("{SIZE}/good.txt"), &format!("{SIZE}/good"), &[]);
    assert!(ok, "{out}");
    // trailing.rs has 8 counted lines plus a test module; tests/ and
    // *_tests.rs files have 20 lines and are skipped.
    assert!(!out.contains("trailing.rs"), "{out}");
    assert!(!out.contains("inside.rs"), "{out}");
    assert!(!out.contains("sample_tests.rs"), "{out}");
    assert!(
        out.contains("shrunk.rs shrank to 13 (baseline 16)"),
        "{out}"
    );
}

#[test]
fn size_fails_new_files_over_the_limit() {
    let (ok, out) = size("/nonexistent", &format!("{SIZE}/over"), &[]);
    assert!(!ok, "{out}");
    assert!(
        out.contains("OVER tests/fixtures/file-size/over/big.rs: 15 lines"),
        "{out}"
    );
    // A test module followed by more code is not trailing, so it counts.
    assert!(out.contains("over/middle.rs: 12 lines"), "{out}");
}

#[test]
fn size_fails_when_a_baselined_file_grows() {
    let (ok, out) = size(&format!("{SIZE}/grew.txt"), &format!("{SIZE}/grew"), &[]);
    assert!(!ok, "{out}");
    assert!(
        out.contains("GREW tests/fixtures/file-size/grew/grown.rs: 14 lines, baseline 12"),
        "{out}"
    );
}

#[test]
fn size_fails_stale_and_under_limit_baseline_entries() {
    let (ok, out) = size(&format!("{SIZE}/under.txt"), &format!("{SIZE}/under"), &[]);
    assert!(!ok, "{out}");
    assert!(
        out.contains("UNDER tests/fixtures/file-size/under/under.rs: 9 lines"),
        "{out}"
    );
    assert!(
        out.contains("STALE tests/fixtures/file-size/under/gone.rs"),
        "{out}"
    );
}

#[test]
fn size_update_lowers_and_drops_entries() {
    let dir = scratch("size-update");
    let baseline = dir.join("baseline.txt");
    fs::write(
        &baseline,
        "# kept comment\n16 tests/fixtures/file-size/good/shrunk.rs\n",
    )
    .unwrap();
    let b = baseline.to_str().unwrap();
    let (ok, out) = size(b, &format!("{SIZE}/good"), &["--update"]);
    assert!(ok, "{out}");
    assert_eq!(
        fs::read_to_string(&baseline).unwrap(),
        "# kept comment\n13 tests/fixtures/file-size/good/shrunk.rs\n"
    );
    // Under the limit: --update removes the entry instead of failing.
    fs::write(&baseline, "12 tests/fixtures/file-size/under/under.rs\n").unwrap();
    let (ok, out) = size(b, &format!("{SIZE}/under"), &["--update"]);
    assert!(ok, "{out}");
    assert_eq!(fs::read_to_string(&baseline).unwrap(), "");
    // It never raises an entry.
    fs::write(&baseline, "12 tests/fixtures/file-size/grew/grown.rs\n").unwrap();
    let (ok, _) = size(b, &format!("{SIZE}/grew"), &["--update"]);
    assert!(!ok);
    let _ = fs::remove_dir_all(dir);
}

// ----------------------------------------------------------------- hotspots

const RULES: &str = "\
# fixture rules
mods.rs | mod-lines | add or remove `mod` lines
main.rs | none | nothing
app.rs | max:2 | one field
enum.rs | re:[[:space:]]*Variant[0-9]+, | variant lines only
";

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .output()
        .expect("git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A repo whose `base` branch holds the hotspot files; HEAD is a `pr`
/// branch on top of it.
fn hotspot_repo(name: &str) -> PathBuf {
    let dir = scratch(name);
    git(&dir, &["init", "-q", "-b", "base"]);
    fs::write(dir.join("hotspots.txt"), RULES).unwrap();
    fs::write(dir.join("mods.rs"), "mod a;\nmod b;\n").unwrap();
    fs::write(dir.join("main.rs"), "fn main() {}\n").unwrap();
    fs::write(dir.join("app.rs"), "pub struct App {\n    a: u32,\n}\n").unwrap();
    fs::write(dir.join("enum.rs"), "pub enum E {\n    Variant1,\n}\n").unwrap();
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-qm", "base"]);
    git(&dir, &["checkout", "-qb", "pr"]);
    dir
}

fn commit(dir: &Path, file: &str, body: &str, message: &str) {
    fs::write(dir.join(file), body).unwrap();
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-qm", message]);
}

fn hotspots(dir: &Path, extra: &[&str], event: Option<&Path>) -> (bool, String) {
    let mut cmd = Command::new("bash");
    cmd.current_dir(dir)
        .arg(repo().join("scripts/check-hotspots.sh"))
        .args(["-f", "hotspots.txt"])
        .args(extra)
        .arg("base")
        .env_remove("GITHUB_ACTIONS")
        .env_remove("GITHUB_EVENT_PATH");
    if let Some(event) = event {
        cmd.env("GITHUB_EVENT_PATH", event);
    }
    let out = cmd.output().expect("run check-hotspots.sh");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

#[test]
fn hotspots_allow_the_listed_changes() {
    let dir = hotspot_repo("allowed");
    commit(&dir, "mods.rs", "mod a;\nmod b;\nmod c;\n", "add a module");
    commit(
        &dir,
        "app.rs",
        "pub struct App {\n    a: u32,\n    b: u32,\n}\n",
        "one field",
    );
    commit(
        &dir,
        "enum.rs",
        "pub enum E {\n    Variant1,\n    Variant2,\n}\n",
        "variant",
    );
    commit(&dir, "other.rs", "fn free() {}\n", "not a hotspot");
    let (ok, out) = hotspots(&dir, &[], None);
    assert!(ok, "{out}");
    assert!(out.contains("hotspots OK"), "{out}");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn hotspots_fail_changes_beyond_the_rule() {
    let dir = hotspot_repo("beyond");
    commit(
        &dir,
        "mods.rs",
        "mod a;\nmod b;\nfn helper() {}\n",
        "code in a module list",
    );
    commit(&dir, "main.rs", "fn main() { start(); }\n", "main");
    commit(
        &dir,
        "app.rs",
        "pub struct App {\n    a: u32,\n    b: u32,\n    c: u32,\n    d: u32,\n}\n",
        "three fields",
    );
    commit(
        &dir,
        "enum.rs",
        "pub enum E {\n    Variant1,\n    Other,\n}\n",
        "named variant",
    );
    let (ok, out) = hotspots(&dir, &[], None);
    assert!(!ok, "{out}");
    assert!(out.contains("HOTSPOT mods.rs"), "{out}");
    assert!(out.contains("+fn helper() {}"), "{out}");
    assert!(
        out.contains("HOTSPOT main.rs: a normal PR may only: nothing"),
        "{out}"
    );
    assert!(out.contains("HOTSPOT app.rs"), "{out}");
    assert!(out.contains("3 changed lines (allowed 2)"), "{out}");
    assert!(out.contains("HOTSPOT enum.rs"), "{out}");
    assert!(out.contains("hotspot-ok"), "{out}");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn hotspots_accept_a_trailer_or_label() {
    let dir = hotspot_repo("override");
    commit(&dir, "main.rs", "fn main() { start(); }\n", "main");
    let (ok, _) = hotspots(&dir, &[], None);
    assert!(!ok);
    let (ok, out) = hotspots(&dir, &["--labels", "ui,hotspot-ok"], None);
    assert!(ok, "{out}");
    assert!(out.contains("allowed by label hotspot-ok"), "{out}");
    let event = dir.join("event.json");
    fs::write(
        &event,
        r#"{"pull_request":{"labels":[{"name":"hotspot-ok"}]}}"#,
    )
    .unwrap();
    let (ok, out) = hotspots(&dir, &[], Some(&event));
    assert!(ok, "{out}");
    commit(
        &dir,
        "main.rs",
        "fn main() { start(); stop(); }\n",
        "main again\n\nHotspot-change: new CLI flag",
    );
    let (ok, out) = hotspots(&dir, &[], None);
    assert!(ok, "{out}");
    assert!(
        out.contains("allowed by Hotspot-change: new CLI flag"),
        "{out}"
    );
    let _ = fs::remove_dir_all(dir);
}

fn big_match(arms: usize) -> String {
    let mut s = String::from("fn f(x: u32) -> u32 {\n    match x {\n");
    for i in 0..arms {
        s += &format!("        {i} => {i},\n");
    }
    s += "        _ => 0,\n    }\n}\n";
    s
}

fn big_enum(variants: usize) -> String {
    let mut s = String::from("pub enum Big {\n");
    for i in 0..variants {
        s += &format!("    V{i},\n    /// doc\n");
    }
    s += "}\n";
    s
}

#[test]
fn hotspots_warn_on_new_big_matches_and_enums() {
    let dir = hotspot_repo("warn");
    commit(
        &dir,
        "small.rs",
        &big_match(29),
        "30 arms with the wildcard",
    );
    commit(&dir, "large.rs", &big_match(30), "31 arms");
    commit(
        &dir,
        "enums.rs",
        &(big_enum(40) + &big_enum(41).replace("Big", "Bigger")),
        "enums",
    );
    let (ok, out) = hotspots(&dir, &[], None);
    assert!(ok, "warnings never fail: {out}");
    assert!(!out.contains("small.rs"), "{out}");
    assert!(
        out.contains("warning: large.rs:2: match with 31 arms"),
        "{out}"
    );
    assert!(out.contains("pub enum with 41 variants"), "{out}");
    assert!(!out.contains("40 variants"), "{out}");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn hotspots_do_not_warn_on_an_unchanged_big_match() {
    let dir = hotspot_repo("unchanged");
    git(&dir, &["checkout", "-q", "base"]);
    commit(&dir, "large.rs", &big_match(40), "big match on base");
    git(&dir, &["checkout", "-q", "pr"]);
    git(&dir, &["merge", "-q", "--no-edit", "base"]);
    // Edit the file without adding arms.
    let body = big_match(40).replace("fn f(", "pub fn f(");
    commit(&dir, "large.rs", &body, "touch it");
    let (ok, out) = hotspots(&dir, &[], None);
    assert!(ok, "{out}");
    assert!(!out.contains("warning"), "{out}");
    let _ = fs::remove_dir_all(dir);
}
