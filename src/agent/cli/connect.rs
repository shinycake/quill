//! `quill agent connect <app>`: how to add Quill to an MCP app, with this
//! Quill's real path, and with `--yes` the config written for the user
//! (merged into what is there, after a backup).

use super::{Exit, Failure, Global, Output};
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum App {
    ClaudeCode,
    ClaudeDesktop,
    Cursor,
}

impl App {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "claude-code" | "claude" => Some(Self::ClaudeCode),
            "claude-desktop" => Some(Self::ClaudeDesktop),
            "cursor" => Some(Self::Cursor),
            _ => None,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::ClaudeDesktop => "Claude Desktop",
            Self::Cursor => "Cursor",
        }
    }

    /// The app's MCP config file on this system, when it has one.
    pub fn config_path(self) -> Option<PathBuf> {
        let dirs = directories::BaseDirs::new()?;
        match self {
            Self::ClaudeCode => None,
            // ~/Library/Application Support on macOS, %APPDATA% on Windows,
            // $XDG_CONFIG_HOME or ~/.config on Linux.
            Self::ClaudeDesktop => Some(
                dirs.config_dir()
                    .join("Claude")
                    .join("claude_desktop_config.json"),
            ),
            Self::Cursor => Some(dirs.home_dir().join(".cursor").join("mcp.json")),
        }
    }
}

/// This executable, as a string for configs and commands.
fn executable() -> String {
    crate::agent::link::executable()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| "quill".to_string())
}

/// Quote `word` for the shell the command is pasted into: POSIX shells, or
/// PowerShell and cmd on Windows, where paths cannot contain `"`.
pub fn shell_quote(word: &str) -> String {
    let plain =
        |c: char| c.is_ascii_alphanumeric() || "/._-+:".contains(c) || (cfg!(windows) && c == '\\');
    if !word.is_empty() && word.chars().all(plain) {
        word.to_string()
    } else if cfg!(windows) {
        format!("\"{word}\"")
    } else {
        format!("'{}'", word.replace('\'', "'\\''"))
    }
}

/// The `claude mcp add` line for `exe`.
pub fn claude_code_command(exe: &str) -> String {
    format!("claude mcp add quill -- {} mcp", shell_quote(exe))
}

/// The server entry MCP apps take under `mcpServers`.
pub fn server_entry(exe: &str) -> Value {
    json!({"command": exe, "args": ["mcp"]})
}

pub(super) fn run(global: &Global, args: &[String]) -> Result<Output, Failure> {
    let mut app = None;
    let mut print = false;
    let mut yes = false;
    for arg in args {
        match arg.as_str() {
            "--print" => print = true,
            "--yes" | "-y" => yes = true,
            name if app.is_none() && !name.starts_with('-') => {
                app = Some(App::parse(name).ok_or_else(|| usage(name))?);
            }
            other => return Err(usage(other)),
        }
    }
    let app = app.ok_or_else(|| usage(""))?;
    let exe = executable();
    if app == App::ClaudeCode {
        let command = claude_code_command(&exe);
        if print || global.json {
            return Ok(if global.json {
                Output::Json(json!({"app": "claude-code", "command": command}))
            } else {
                Output::Text(command)
            });
        }
        return Ok(Output::Text(format!(
            "Run this in a terminal to add Quill to Claude Code:\n\n  {command}\n\n\
Add --scope user after `add` to use it in every project. Then turn on Agent access in Quill \
(Settings > Advanced > Agent access) and ask Claude to call quill_status."
        )));
    }
    let entry = server_entry(&exe);
    let snippet = json!({"mcpServers": {"quill": entry}});
    let path = app.config_path().ok_or_else(|| {
        Failure::new(
            Exit::LocalFile,
            "Could not find your home folder, so the config file's place is unknown.",
        )
    })?;
    if print {
        return Ok(if global.json {
            Output::Json(snippet)
        } else {
            Output::Text(format!("{snippet:#}"))
        });
    }
    if !yes {
        let text = format!(
            "Add this to {} ({}), merging with what is there:\n\n{snippet:#}\n\n\
Or run `quill agent connect {} --yes` to add it for you; the old file is backed up first. \
Then restart {} and turn on Agent access in Quill (Settings > Advanced > Agent access).",
            path.display(),
            app.label(),
            arg_name(app),
            app.label(),
        );
        return Ok(if global.json {
            Output::Json(
                json!({"app": arg_name(app), "config": path.display().to_string(), "entry": snippet}),
            )
        } else {
            Output::Text(text)
        });
    }
    let outcome = merge_into(&path, &entry)?;
    let text = match &outcome {
        Merge::AlreadySet => format!("{} already has Quill in {}.", app.label(), path.display()),
        Merge::Written { backup } => {
            let mut text = format!("Added Quill to {}.", path.display());
            if let Some(backup) = backup {
                text.push_str(&format!(" The old file is saved as {}.", backup.display()));
            }
            text.push_str(&format!(
                " Restart {} to load it, and turn on Agent access in Quill (Settings > Advanced > Agent access).",
                app.label()
            ));
            text
        }
    };
    Ok(if global.json {
        Output::Json(json!({
            "app": arg_name(app),
            "config": path.display().to_string(),
            "changed": matches!(outcome, Merge::Written { .. }),
            "backup": match &outcome {
                Merge::Written { backup: Some(b) } => Value::from(b.display().to_string()),
                _ => Value::Null,
            },
        }))
    } else {
        Output::Text(text)
    })
}

const fn arg_name(app: App) -> &'static str {
    match app {
        App::ClaudeCode => "claude-code",
        App::ClaudeDesktop => "claude-desktop",
        App::Cursor => "cursor",
    }
}

fn usage(got: &str) -> Failure {
    let what = if got.is_empty() {
        "Name the app".to_string()
    } else {
        format!("Unknown app or option {got}")
    };
    Failure::new(
        Exit::Usage,
        format!("{what}: quill agent connect claude-code|claude-desktop|cursor [--print] [--yes]."),
    )
}

/// What `merge_into` did.
#[derive(Debug, PartialEq, Eq)]
pub enum Merge {
    AlreadySet,
    Written { backup: Option<PathBuf> },
}

/// Put `entry` at `mcpServers.quill` in the JSON file at `path`, keeping
/// everything else. A file that is not a JSON object is left alone.
pub fn merge_into(path: &Path, entry: &Value) -> Result<Merge, Failure> {
    let local = |message: String| Failure::new(Exit::LocalFile, message);
    let existing = match std::fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(local(format!("Could not read {} ({e}).", path.display()))),
    };
    let mut root = match existing.as_deref().map(str::trim) {
        None | Some("") => Map::new(),
        Some(text) => match serde_json::from_str::<Value>(text) {
            Ok(Value::Object(root)) => root,
            _ => {
                return Err(local(format!(
                    "{} is not a JSON object, so Quill left it alone. Fix it, or add the entry by hand (quill agent connect … --print).",
                    path.display()
                )));
            }
        },
    };
    let servers = root
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(servers) = servers.as_object_mut() else {
        return Err(local(format!(
            "mcpServers in {} is not an object, so Quill left the file alone.",
            path.display()
        )));
    };
    if servers.get("quill") == Some(entry) {
        return Ok(Merge::AlreadySet);
    }
    servers.insert("quill".into(), entry.clone());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| local(format!("Could not create {} ({e}).", parent.display())))?;
    }
    let backup = match existing {
        Some(text) => {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or_default();
            let mut name = path.as_os_str().to_owned();
            name.push(format!(".quill-backup-{stamp}"));
            let backup = PathBuf::from(name);
            std::fs::write(&backup, text)
                .map_err(|e| local(format!("Could not back up to {} ({e}).", backup.display())))?;
            Some(backup)
        }
        None => None,
    };
    let mut temp = path.as_os_str().to_owned();
    temp.push(".quill-tmp");
    let temp = PathBuf::from(temp);
    let text = format!("{:#}\n", Value::Object(root));
    std::fs::write(&temp, text)
        .and_then(|()| std::fs::rename(&temp, path))
        .map_err(|e| {
            let _ = std::fs::remove_file(&temp);
            local(format!("Could not write {} ({e}).", path.display()))
        })?;
    Ok(Merge::Written { backup })
}
