//! `connect --yes` and `install-skill` on the user's files: symlinks,
//! permissions, backups, key order, the Microsoft Store Claude Desktop,
//! translocated copies and shell quoting.

use super::{Exit, connect, skill};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn fresh(name: &str) -> PathBuf {
    let dir = crate::scratch::process_dir(name);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap_or_default()).unwrap_or_default()
}

fn entry() -> Value {
    connect::server_entry("/Applications/Quill.app/Contents/MacOS/quill")
}

fn backups(dir: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.to_string_lossy().contains(".quill-backup-"))
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    found
}

#[test]
fn connect_keeps_the_configs_key_order() {
    let dir = fresh("quill-connect-order");
    let path = dir.join("mcp.json");
    let theirs = r#"{"zebra": 1, "mcpServers": {"zz": {"command": "z"}, "aa": {"command": "a"}}, "alpha": 2}"#;
    std::fs::write(&path, theirs).unwrap_or_default();
    assert!(connect::merge_into(&path, &entry()).is_ok());
    let written = std::fs::read_to_string(&path).unwrap_or_default();
    let at = |needle: &str| written.find(needle).unwrap_or(usize::MAX);
    assert!(
        at("zebra") < at("mcpServers") && at("mcpServers") < at("alpha"),
        "{written}"
    );
    assert!(
        at("\"zz\"") < at("\"aa\"") && at("\"aa\"") < at("\"quill\""),
        "{written}"
    );
}

#[test]
fn connect_reports_a_replaced_entry_and_never_overwrites_a_backup() {
    let dir = fresh("quill-connect-replace");
    let path = dir.join("mcp.json");
    std::fs::write(&path, r#"{"mcpServers": {"quill": {"command": "old"}}}"#).unwrap_or_default();
    let first = connect::merge_into(&path, &entry());
    assert!(
        matches!(
            first,
            Ok(connect::Merge::Written {
                replaced: true,
                backup: Some(_)
            })
        ),
        "{first:?}"
    );
    // A second change within the same second gets its own backup.
    std::fs::write(&path, r#"{"mcpServers": {"quill": {"command": "older"}}}"#).unwrap_or_default();
    assert!(connect::merge_into(&path, &entry()).is_ok());
    let saved = backups(&dir);
    assert_eq!(saved.len(), 2, "{saved:?}");
    let commands: Vec<Value> = saved
        .iter()
        .map(|b| read_json(b)["mcpServers"]["quill"]["command"].clone())
        .collect();
    assert!(commands.contains(&json!("old")) && commands.contains(&json!("older")));
    // No temp file is left behind.
    assert!(!dir.join("mcp.json.quill-tmp").exists());
}

#[cfg(unix)]
#[test]
fn connect_writes_through_a_symlinked_config() {
    let dir = fresh("quill-connect-symlink");
    let real_dir = dir.join("dotfiles");
    let _ = std::fs::create_dir_all(&real_dir);
    let real = real_dir.join("mcp.json");
    std::fs::write(&real, r#"{"theme": "dark"}"#).unwrap_or_default();
    let link = dir.join("mcp.json");
    assert!(std::os::unix::fs::symlink("dotfiles/mcp.json", &link).is_ok());

    let merged = connect::merge_into(&link, &entry());
    assert!(
        matches!(merged, Ok(connect::Merge::Written { .. })),
        "{merged:?}"
    );
    let meta = std::fs::symlink_metadata(&link).map(|m| m.file_type().is_symlink());
    assert_eq!(meta.ok(), Some(true), "the config is still a link");
    assert_eq!(read_json(&real)["theme"], "dark");
    assert_eq!(read_json(&real)["mcpServers"]["quill"], entry());
    // The backup and the temp file sit beside the real file.
    assert_eq!(backups(&real_dir).len(), 1);
    assert!(backups(&dir).is_empty());
}

#[cfg(unix)]
#[test]
fn connect_keeps_a_private_configs_mode() {
    use std::os::unix::fs::PermissionsExt;
    let mode = |path: &Path| {
        std::fs::metadata(path)
            .map(|m| m.permissions().mode() & 0o777)
            .unwrap_or_default()
    };
    let dir = fresh("quill-connect-mode");
    let path = dir.join("claude_desktop_config.json");
    std::fs::write(&path, "{}").unwrap_or_default();
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    assert!(connect::merge_into(&path, &entry()).is_ok());
    assert_eq!(mode(&path), 0o600);
    let saved = backups(&dir);
    assert_eq!(
        saved.iter().map(|b| mode(b)).collect::<Vec<_>>(),
        vec![0o600]
    );

    // A file that was readable by others stays that way.
    std::fs::write(&path, r#"{"a": 1}"#).unwrap_or_default();
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644));
    let replaced = connect::server_entry("/usr/bin/quill");
    assert!(connect::merge_into(&path, &replaced).is_ok());
    assert_eq!(mode(&path), 0o644);

    // A new config is owner-only.
    let new = dir.join("new").join("mcp.json");
    assert!(connect::merge_into(&new, &entry()).is_ok());
    assert_eq!(mode(&new), 0o600);
}

#[test]
fn claude_desktop_from_the_store_gets_the_package_config() {
    let dir = fresh("quill-connect-store");
    let plain = dir
        .join("Roaming")
        .join("Claude")
        .join("claude_desktop_config.json");
    let packages = dir.join("Packages");
    let _ = std::fs::create_dir_all(packages.join("SomethingElse_1"));

    // No Store package: the %APPDATA% file.
    let config = connect::claude_desktop_config(plain.clone(), Some(&packages));
    assert_eq!(config.path, plain);
    assert_eq!(config.store, None);

    // A Store package, nothing in %APPDATA%.
    let package = packages.join("Claude_pzs8sxrjxfjjc");
    let _ = std::fs::create_dir_all(&package);
    let store = package
        .join("LocalCache")
        .join("Roaming")
        .join("Claude")
        .join("claude_desktop_config.json");
    let config = connect::claude_desktop_config(plain.clone(), Some(&packages));
    assert_eq!(config.path, store);
    assert_eq!(config.store, Some(connect::StoreInstall::Only));

    // Both: the Store one, naming the other.
    let _ = std::fs::create_dir_all(plain.parent().unwrap_or(dir.as_path()));
    std::fs::write(&plain, "{}").unwrap_or_default();
    let config = connect::claude_desktop_config(plain.clone(), Some(&packages));
    assert_eq!(config.path, store);
    assert_eq!(
        config.store,
        Some(connect::StoreInstall::AlsoPlain(plain.clone()))
    );

    // Not on Windows: never looked for.
    assert_eq!(
        connect::claude_desktop_config(plain.clone(), None).path,
        plain
    );
}

#[test]
fn a_translocated_copy_is_refused() {
    assert!(connect::translocated(
        "/private/var/folders/x/T/AppTranslocation/1234/d/Quill.app/Contents/MacOS/quill"
    ));
    assert!(!connect::translocated(
        "/Applications/Quill.app/Contents/MacOS/quill"
    ));
}

#[test]
fn paths_are_quoted_for_the_shell_they_are_pasted_into() {
    let posix = |exe: &str| connect::claude_code_commands(exe, false);
    let line = |exe: &str| vec![("", format!("claude mcp add quill -- {exe} mcp"))];
    assert_eq!(posix("/usr/bin/quill"), line("/usr/bin/quill"));
    assert_eq!(posix("/home/a b/quill"), line("'/home/a b/quill'"));
    assert_eq!(posix("/home/it's/quill"), line(r"'/home/it'\''s/quill'"));
    assert_eq!(posix("/home/$USER/quill"), line("'/home/$USER/quill'"));
    assert_eq!(posix("/home/a`b`/quill"), line("'/home/a`b`/quill'"));

    let windows = |exe: &str| connect::claude_code_commands(exe, true);
    assert_eq!(
        windows(r"C:\Users\ann\AppData\Local\Programs\Quill\quill.exe"),
        line(r"C:\Users\ann\AppData\Local\Programs\Quill\quill.exe")
    );
    // Double quotes work in PowerShell and cmd alike, quotes inside too.
    assert_eq!(
        windows(r"C:\Program Files\Ann's Quill\quill.exe"),
        line(r#""C:\Program Files\Ann's Quill\quill.exe""#)
    );
    // PowerShell expands $ and ` in double quotes: one line per shell.
    assert_eq!(
        windows(r"C:\Users\$ann\it's `x`\quill.exe"),
        vec![
            (
                "PowerShell",
                r"claude mcp add quill -- 'C:\Users\$ann\it''s `x`\quill.exe' mcp".to_string()
            ),
            (
                "Command Prompt",
                r#"claude mcp add quill -- "C:\Users\$ann\it's `x`\quill.exe" mcp"#.to_string()
            ),
        ]
    );
}

#[test]
fn a_second_force_keeps_the_first_backup() {
    let dir = fresh("quill-skill-backups").join("quill");
    assert!(skill::install(&dir, false).is_ok());
    let [(_, bundled), _] = skill::FILES;
    let mut replaced = Vec::new();
    for edit in ["first edit", "second edit"] {
        std::fs::write(dir.join("SKILL.md"), format!("{bundled}\n{edit}\n")).unwrap_or_default();
        let kept = skill::install(&dir, false).err().map(|f| f.exit);
        assert_eq!(kept, Some(Exit::LocalFile));
        let forced = skill::install(&dir, true).map_err(|f| f.message);
        replaced.extend(forced.map(|r| r.replaced).unwrap_or_default());
    }
    assert_eq!(
        replaced,
        vec![
            ("SKILL.md".to_string(), "SKILL.md.bak".to_string()),
            ("SKILL.md".to_string(), "SKILL.md.2.bak".to_string()),
        ]
    );
    let first = std::fs::read_to_string(dir.join("SKILL.md.bak")).unwrap_or_default();
    let second = std::fs::read_to_string(dir.join("SKILL.md.2.bak")).unwrap_or_default();
    assert!(first.ends_with("first edit\n") && second.ends_with("second edit\n"));
    assert_eq!(
        std::fs::read_to_string(dir.join("SKILL.md"))
            .ok()
            .as_deref(),
        Some(bundled)
    );
    let leftovers: Vec<String> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|name| name.ends_with(".quill-tmp"))
                .collect()
        })
        .unwrap_or_default();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}
