//! `quill agent install-skill`: write the bundled Agent Skill. Idempotent:
//! a file still as Quill last wrote it is updated, a file the user edited
//! is kept unless `--force` (which saves theirs as `<file>.bak`).

use super::{Exit, Failure, Global, Output};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// The skill's files: SKILL.md, and the guide every agent reads
/// (`quill://guide`), which SKILL.md points to.
pub const FILES: [(&str, &str); 2] = [
    ("SKILL.md", include_str!("../skill/SKILL.md")),
    ("guide.md", crate::agent::guide::GUIDE),
];

/// What Quill last wrote, so a later run can tell an edit from an old copy.
const MANIFEST: &str = ".quill-skill.json";

pub(super) fn run(global: &Global, args: &[String]) -> Result<Output, Failure> {
    let mut dir = None;
    let mut force = false;
    let mut print = false;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--force" => force = true,
            "--print" => print = true,
            "--dir" => {
                dir = Some(PathBuf::from(iter.next().ok_or_else(|| {
                    Failure::new(Exit::Usage, "--dir needs a folder.")
                })?));
            }
            other => {
                return Err(Failure::new(
                    Exit::Usage,
                    format!(
                        "install-skill has no option {other}. See quill agent help install-skill."
                    ),
                ));
            }
        }
    }
    if print {
        let [(_, skill), _] = FILES;
        return Ok(Output::Text(skill.to_string()));
    }
    let dir = match dir {
        Some(dir) => dir,
        None => default_dir().ok_or_else(|| {
            Failure::new(
                Exit::LocalFile,
                "Could not find your home folder. Pass --dir.",
            )
        })?,
    };
    let report = install(&dir, force)?;
    if global.json {
        return Ok(Output::Json(report.to_json(&dir)));
    }
    Ok(Output::Text(report.to_text(&dir)))
}

/// `~/.claude/skills/quill`.
pub fn default_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.home_dir().join(".claude/skills/quill"))
}

fn sha(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// What happened to each file.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub written: Vec<String>,
    pub updated: Vec<String>,
    pub unchanged: Vec<String>,
    /// Files the user edited, kept, with the diff to the bundled version.
    pub kept: Vec<(String, String)>,
    /// Edited files replaced by --force; the user's copy is `<file>.bak`.
    pub replaced: Vec<String>,
}

impl Report {
    fn to_json(&self, dir: &Path) -> Value {
        json!({
            "dir": dir.display().to_string(),
            "written": self.written,
            "updated": self.updated,
            "unchanged": self.unchanged,
            "replaced": self.replaced,
        })
    }

    fn to_text(&self, dir: &Path) -> String {
        let mut lines = vec![format!("Quill skill in {}", dir.display())];
        for (label, files) in [
            ("Wrote", &self.written),
            ("Updated", &self.updated),
            ("Already up to date", &self.unchanged),
            ("Replaced (your version is in <file>.bak)", &self.replaced),
        ] {
            if !files.is_empty() {
                lines.push(format!("{label}: {}", files.join(", ")));
            }
        }
        lines.push("Claude Code picks it up in its next session.".into());
        lines.join("\n")
    }
}

/// Install the skill into `dir`.
pub fn install(dir: &Path, force: bool) -> Result<Report, Failure> {
    let io_failure = |path: &Path, e: std::io::Error| {
        Failure::new(
            Exit::LocalFile,
            format!("Could not write {} ({e}).", path.display()),
        )
    };
    std::fs::create_dir_all(dir).map_err(|e| io_failure(dir, e))?;
    let manifest_path = dir.join(MANIFEST);
    let manifest: Map<String, Value> = std::fs::read_to_string(&manifest_path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|v| v.get("files").and_then(Value::as_object).cloned())
        .unwrap_or_default();
    let mut report = Report::default();
    let mut plan = Vec::new();
    for (name, bundled) in FILES {
        let path = dir.join(name);
        match std::fs::read_to_string(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                plan.push((path, bundled));
                report.written.push(name.to_string());
            }
            Err(e) => {
                return Err(Failure::new(
                    Exit::LocalFile,
                    format!("Could not read {} ({e}).", path.display()),
                ));
            }
            Ok(current) if current == bundled => report.unchanged.push(name.to_string()),
            Ok(current)
                if manifest.get(name).and_then(Value::as_str) == Some(sha(&current).as_str()) =>
            {
                plan.push((path, bundled));
                report.updated.push(name.to_string());
            }
            Ok(current) if force => {
                let backup = dir.join(format!("{name}.bak"));
                std::fs::write(&backup, current).map_err(|e| io_failure(&backup, e))?;
                plan.push((path, bundled));
                report.replaced.push(name.to_string());
            }
            Ok(current) => report
                .kept
                .push((name.to_string(), diff(&current, bundled, name))),
        }
    }
    if !report.kept.is_empty() {
        let mut message = String::from("You changed these skill files, so nothing was written:\n");
        for (name, diff) in &report.kept {
            message.push_str(&format!("\n{name} (- yours, + Quill's):\n{diff}\n"));
        }
        message.push_str(
            "\nRun again with --force to replace them; your versions are saved as <file>.bak.",
        );
        return Err(Failure::new(Exit::LocalFile, message));
    }
    for (path, text) in &plan {
        std::fs::write(path, text).map_err(|e| io_failure(path, e))?;
    }
    let files: Map<String, Value> = FILES
        .iter()
        .map(|(name, text)| ((*name).to_string(), Value::from(sha(text))))
        .collect();
    let manifest = json!({"version": crate::version::APP, "files": files});
    std::fs::write(&manifest_path, format!("{manifest:#}\n"))
        .map_err(|e| io_failure(&manifest_path, e))?;
    Ok(report)
}

/// The lines that differ between `old` and `new`, with two lines of context
/// around the changed middle.
pub fn diff(old: &str, new: &str, name: &str) -> String {
    let old: Vec<&str> = old.lines().collect();
    let new: Vec<&str> = new.lines().collect();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take_while(|(a, b)| a == b)
        .count()
        .min(old.len().saturating_sub(prefix))
        .min(new.len().saturating_sub(prefix));
    let context = prefix.saturating_sub(2);
    let mut out = vec![format!(
        "--- {name} (yours)\n+++ {name} (Quill {})",
        crate::version::APP
    )];
    let old_end = old.len().saturating_sub(suffix);
    let new_end = new.len().saturating_sub(suffix);
    for line in old
        .iter()
        .skip(context)
        .take(prefix.saturating_sub(context))
    {
        out.push(format!("  {line}"));
    }
    let removed = old.iter().skip(prefix).take(old_end.saturating_sub(prefix));
    let added = new.iter().skip(prefix).take(new_end.saturating_sub(prefix));
    const MAX: usize = 60;
    let mut shown = 0usize;
    for line in removed {
        shown += 1;
        if shown <= MAX {
            out.push(format!("- {line}"));
        }
    }
    for line in added {
        shown += 1;
        if shown <= MAX {
            out.push(format!("+ {line}"));
        }
    }
    if shown > MAX {
        out.push(format!(
            "  … {} more changed lines",
            shown.saturating_sub(MAX)
        ));
    }
    for line in old.iter().skip(old_end).take(2) {
        out.push(format!("  {line}"));
    }
    out.join("\n")
}
