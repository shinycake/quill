//! `quill agent connect <app>`: how to add Quill to an MCP app, with this
//! Quill's real path, and with `--yes` the config written for the user
//! (merged into what is there, after a backup).

use super::{Exit, Failure, Global, Output, files};
use crate::agent::SETTINGS_PLACE;
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

    const fn arg_name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::ClaudeDesktop => "claude-desktop",
            Self::Cursor => "cursor",
        }
    }

    /// The app's MCP config file on this system, when it has one.
    pub fn config_file(self) -> Option<ConfigFile> {
        let dirs = directories::BaseDirs::new()?;
        match self {
            Self::ClaudeCode => None,
            // ~/Library/Application Support on macOS, %APPDATA% on Windows,
            // $XDG_CONFIG_HOME or ~/.config on Linux.
            Self::ClaudeDesktop => {
                let plain = dirs
                    .config_dir()
                    .join("Claude")
                    .join("claude_desktop_config.json");
                // %LOCALAPPDATA%\Packages, where Store apps keep their files.
                let packages = cfg!(windows).then(|| dirs.data_local_dir().join("Packages"));
                Some(claude_desktop_config(plain, packages.as_deref()))
            }
            Self::Cursor => Some(ConfigFile {
                path: dirs.home_dir().join(".cursor").join("mcp.json"),
                store: None,
            }),
        }
    }
}

/// Where an app's MCP config lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigFile {
    pub path: PathBuf,
    /// Set for Claude Desktop from the Microsoft Store.
    pub store: Option<StoreInstall>,
}

/// Claude Desktop from the Microsoft Store (MSIX) reads its config from its
/// package folder, not from %APPDATA%.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreInstall {
    /// Only the Store copy's config folder exists.
    Only,
    /// A config also exists at this %APPDATA% path, which the Store copy
    /// does not read; Quill leaves it alone.
    AlsoPlain(PathBuf),
}

impl ConfigFile {
    /// One sentence on the Store install, for the output.
    fn store_note(&self) -> Option<String> {
        match self.store.as_ref()? {
            StoreInstall::Only => Some(
                "This is Claude Desktop from the Microsoft Store, which keeps its config in its package folder."
                    .into(),
            ),
            StoreInstall::AlsoPlain(plain) => Some(format!(
                "Claude Desktop from the Microsoft Store reads this file, not {}, so that one is left as it is.",
                plain.display()
            )),
        }
    }
}

/// Claude Desktop's config: the Microsoft Store copy's, under a `Claude_*`
/// folder in `packages`, when there is one; `plain` otherwise.
pub fn claude_desktop_config(plain: PathBuf, packages: Option<&Path>) -> ConfigFile {
    let Some(store_path) = packages.and_then(store_config) else {
        return ConfigFile {
            path: plain,
            store: None,
        };
    };
    let store = if plain.exists() {
        StoreInstall::AlsoPlain(plain)
    } else {
        StoreInstall::Only
    };
    ConfigFile {
        path: store_path,
        store: Some(store),
    }
}

/// `<package>\LocalCache\Roaming\Claude\claude_desktop_config.json` for the
/// first `Claude_*` package in `packages`, preferring one whose config
/// folder already exists.
fn store_config(packages: &Path) -> Option<PathBuf> {
    let config_dir = |package: &Path| package.join("LocalCache").join("Roaming").join("Claude");
    let mut found: Vec<PathBuf> = std::fs::read_dir(packages)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("Claude_"))
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    found.sort();
    let package = found
        .iter()
        .find(|package| config_dir(package).is_dir())
        .or_else(|| found.first())?;
    Some(config_dir(package).join("claude_desktop_config.json"))
}

/// This executable, as a string for configs and commands.
fn executable() -> String {
    crate::agent::link::executable()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| "quill".to_string())
}

/// A copy macOS runs from a randomized read-only mount (App Translocation)
/// because it was opened from Downloads or a disk image. The path changes
/// on every launch, so a config naming it breaks.
pub fn translocated(exe: &str) -> bool {
    exe.contains("/AppTranslocation/")
}

/// How to run `quill agent` on this system, for the next-step hints: on
/// Windows the console helper, which waits and passes the exit code on.
fn cli_name() -> &'static str {
    if cfg!(windows) { "quill-cli" } else { "quill" }
}

/// Quote `word` for a POSIX shell.
pub fn posix_quote(word: &str) -> String {
    let plain = |c: char| c.is_ascii_alphanumeric() || "/._-+:".contains(c);
    if !word.is_empty() && word.chars().all(plain) {
        word.to_string()
    } else {
        format!("'{}'", word.replace('\'', "'\\''"))
    }
}

/// Quote a Windows path (which can't hold `"`) for PowerShell and cmd. One
/// form works in both unless the path has `$` or a backtick, which
/// PowerShell expands inside double quotes; then it is
/// `(PowerShell, Some(cmd))`.
pub fn windows_quote(word: &str) -> (String, Option<String>) {
    let plain = |c: char| c.is_ascii_alphanumeric() || "/\\._-+:".contains(c);
    if !word.is_empty() && word.chars().all(plain) {
        return (word.to_string(), None);
    }
    let cmd = format!("\"{word}\"");
    if word.contains(['$', '`']) {
        (format!("'{}'", word.replace('\'', "''")), Some(cmd))
    } else {
        (cmd, None)
    }
}

/// `line` around `exe` quoted for the shell, with a label per shell when
/// the shells need different lines.
pub fn shell_lines(
    exe: &str,
    windows: bool,
    line: impl Fn(&str) -> String,
) -> Vec<(&'static str, String)> {
    if !windows {
        return vec![("", line(&posix_quote(exe)))];
    }
    match windows_quote(exe) {
        (both, None) => vec![("", line(&both))],
        (powershell, Some(cmd)) => vec![
            ("PowerShell", line(&powershell)),
            ("Command Prompt", line(&cmd)),
        ],
    }
}

/// The `claude mcp add` line for `exe`.
pub fn claude_code_commands(exe: &str, windows: bool) -> Vec<(&'static str, String)> {
    shell_lines(exe, windows, |quoted| {
        format!("claude mcp add quill -- {quoted} mcp")
    })
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
    if translocated(&exe) {
        return Err(Failure::new(
            Exit::LocalFile,
            format!(
                "Quill is running from a temporary copy macOS made ({exe}), because it was opened from Downloads or a disk image. That path changes on every launch, so Quill won't put it in a config. Move Quill to /Applications, open it from there, then run this again."
            ),
        ));
    }
    if app == App::ClaudeCode {
        return Ok(claude_code(global, &exe, print));
    }
    let entry = server_entry(&exe);
    let snippet = json!({"mcpServers": {"quill": entry}});
    let config = app.config_file().ok_or_else(|| {
        Failure::new(
            Exit::LocalFile,
            "Could not find your home folder, so the config file's place is unknown.",
        )
    })?;
    let path = &config.path;
    let note = config.store_note();
    if print {
        return Ok(if global.json {
            Output::Json(snippet)
        } else {
            Output::Text(format!("{snippet:#}"))
        });
    }
    if !yes {
        if global.json {
            return Ok(Output::Json(json!({
                "app": app.arg_name(),
                "config": path.display().to_string(),
                "store_install": config.store.is_some(),
                "entry": snippet,
            })));
        }
        let mut text = format!(
            "Add this to {} ({}), merging with what is there:\n\n{snippet:#}\n\n",
            path.display(),
            app.label(),
        );
        if let Some(note) = &note {
            text.push_str(note);
            text.push(' ');
        }
        text.push_str(&format!(
            "Or run `{} agent connect {} --yes` to add it for you; the old file is backed up first. \
Then restart {} and turn on Agent access in Quill ({SETTINGS_PLACE}).",
            cli_name(),
            app.arg_name(),
            app.label(),
        ));
        return Ok(Output::Text(text));
    }
    let outcome = merge_into(path, &entry)?;
    let backup = match &outcome {
        Merge::Written {
            backup: Some(backup),
            ..
        } => Some(backup.display().to_string()),
        _ => None,
    };
    if global.json {
        return Ok(Output::Json(json!({
            "app": app.arg_name(),
            "config": path.display().to_string(),
            "store_install": config.store.is_some(),
            "changed": matches!(outcome, Merge::Written { .. }),
            "replaced": matches!(outcome, Merge::Written { replaced: true, .. }),
            "backup": backup,
        })));
    }
    let mut text = match &outcome {
        Merge::AlreadySet => format!("{} already has Quill in {}.", app.label(), path.display()),
        Merge::Written { replaced: true, .. } => {
            format!("Replaced your Quill entry in {}.", path.display())
        }
        Merge::Written { .. } => format!("Added Quill to {}.", path.display()),
    };
    if let Some(backup) = &backup {
        text.push_str(&format!(" The old file is saved as {backup}."));
    }
    if let Some(note) = &note {
        text.push(' ');
        text.push_str(note);
    }
    if matches!(outcome, Merge::Written { .. }) {
        text.push_str(&format!(
            " Restart {} to load it, and turn on Agent access in Quill ({SETTINGS_PLACE}).",
            app.label()
        ));
    }
    Ok(Output::Text(text))
}

fn claude_code(global: &Global, exe: &str, print: bool) -> Output {
    let commands = claude_code_commands(exe, cfg!(windows));
    if global.json {
        let lines: Map<String, Value> = commands
            .iter()
            .map(|(shell, line)| {
                let key = match *shell {
                    "" | "PowerShell" => "command",
                    _ => "command_cmd",
                };
                (key.to_string(), Value::from(line.as_str()))
            })
            .collect();
        let mut out = json!({"app": "claude-code"});
        if let Some(object) = out.as_object_mut() {
            object.extend(lines);
        }
        return Output::Json(out);
    }
    let shown: Vec<String> = commands
        .iter()
        .map(|(shell, line)| {
            if shell.is_empty() {
                line.clone()
            } else {
                format!("{shell}:\n  {line}")
            }
        })
        .collect();
    if print {
        return Output::Text(shown.join("\n"));
    }
    Output::Text(format!(
        "Run this in a terminal to add Quill to Claude Code:\n\n  {}\n\n\
Add --scope user after `add` to use it in every project. Then turn on Agent access in Quill \
({SETTINGS_PLACE}) and ask Claude to call quill_status.",
        shown.join("\n\n  ")
    ))
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
    /// `replaced` when a different `mcpServers.quill` was there before.
    Written {
        backup: Option<PathBuf>,
        replaced: bool,
    },
}

/// Put `entry` at `mcpServers.quill` in the JSON file at `path`, keeping
/// everything else and its key order. A file that is not a JSON object is
/// left alone. A symlinked config stays a link: the file it points to is
/// rewritten, with its permissions, beside a backup that has them too.
pub fn merge_into(path: &Path, entry: &Value) -> Result<Merge, Failure> {
    let local = |message: String| Failure::new(Exit::LocalFile, message);
    let target = files::resolve_links(path);
    let existing = match std::fs::read_to_string(&target) {
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
    let replaced = servers.insert("quill".into(), entry.clone()).is_some();
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| local(format!("Could not create {} ({e}).", parent.display())))?;
    }
    let like = files::permissions_of(&target);
    let backup = match existing {
        Some(text) => {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or_default();
            let named = |n: u32| {
                let mut name = target.as_os_str().to_owned();
                name.push(format!(".quill-backup-{stamp}"));
                if n > 1 {
                    name.push(format!("-{n}"));
                }
                PathBuf::from(name)
            };
            let backup = files::back_up(&text, like.as_ref(), named)
                .map_err(|e| local(format!("Could not back up {} ({e}).", target.display())))?;
            Some(backup)
        }
        None => None,
    };
    let text = format!("{:#}\n", Value::Object(root));
    files::write_atomic(&target, &text, like.as_ref())
        .map_err(|e| local(format!("Could not write {} ({e}).", path.display())))?;
    Ok(Merge::Written { backup, replaced })
}
