//! The Agent access switch (Settings > Advanced > Agent access) and the
//! token agents authenticate with.
//!
//! Off by default. While it is on, a fresh random token sits in
//! `agent-token` in the app data directory, readable by the owner only; it
//! changes on every launch of Quill and is deleted when the switch goes off.
//! `quill mcp` reads it and presents it on the socket, so only a process
//! running as this user can connect.

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

const SETTINGS_FILE: &str = "agent_access.json";
const TOKEN_FILE: &str = "agent-token";

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Stored {
    enabled: bool,
}

static ENABLED: RwLock<Option<bool>> = RwLock::new(None);
/// Bumped whenever the switch goes off, so open connections notice.
static GENERATION: AtomicU64 = AtomicU64::new(0);

fn root() -> Option<PathBuf> {
    crate::settings::safe_app_root()
}

fn load(path: &Path) -> bool {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Stored>(&bytes).ok())
        .is_some_and(|stored| stored.enabled)
}

/// Whether agents may connect.
pub fn enabled() -> bool {
    if let Ok(guard) = ENABLED.read()
        && let Some(on) = *guard
    {
        return on;
    }
    let loaded = root().is_some_and(|root| load(&root.join(SETTINGS_FILE)));
    if let Ok(mut guard) = ENABLED.write() {
        return *guard.get_or_insert(loaded);
    }
    loaded
}

/// Turn agent access on or off. Off deletes the token and ends every open
/// agent connection.
pub fn set_enabled(on: bool) {
    if crate::fast_buttons::persistence_enabled()
        && let Some(root) = root()
        && let Err(err) =
            crate::settings::write_json_atomic(&root.join(SETTINGS_FILE), &Stored { enabled: on })
    {
        eprintln!("quill: could not save {SETTINGS_FILE}: {err}");
    }
    if let Ok(mut guard) = ENABLED.write() {
        *guard = Some(on);
    }
    if !on {
        GENERATION.fetch_add(1, Ordering::SeqCst);
        super::prompts::deny_all();
    }
    crate::wake::AGENT.poke();
    publish_token();
}

/// The switch's generation; a connection opened under one generation ends
/// once it changes.
pub fn generation() -> u64 {
    GENERATION.load(Ordering::SeqCst)
}

/// This launch's token, made on first use.
fn token() -> &'static str {
    static TOKEN: OnceLock<String> = OnceLock::new();
    TOKEN.get_or_init(|| {
        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    })
}

/// Whether `presented` is this launch's token, compared in constant time.
pub fn token_matches(presented: &str) -> bool {
    let expected = token().as_bytes();
    let presented = presented.as_bytes();
    if presented.len() != expected.len() {
        return false;
    }
    presented
        .iter()
        .zip(expected)
        .fold(0u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

pub(crate) fn token_path(root: &Path) -> PathBuf {
    root.join(TOKEN_FILE)
}

/// Write this launch's token while access is on, delete any old one while
/// it is off. Called once the socket is listening, and on every toggle.
pub fn publish_token() {
    let Some(root) = root() else {
        return;
    };
    let path = token_path(&root);
    let result = if enabled() {
        std::fs::create_dir_all(&root).and_then(|()| write_owner_only(&path, token().as_bytes()))
    } else {
        match std::fs::remove_file(&path) {
            Err(err) if err.kind() != std::io::ErrorKind::NotFound => Err(err),
            _ => Ok(()),
        }
    };
    if let Err(err) = result {
        eprintln!("quill: agent token file: {err}");
    }
}

/// Write `bytes` to `path` through a temp file that is owner-only from the
/// moment it exists. On Windows the per-user app data folder already keeps
/// other users out.
fn write_owner_only(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    let temp = PathBuf::from(temp);
    let _ = std::fs::remove_file(&temp);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&temp, path)
}

/// Whether the switch is on in the settings under `root`, read from disk
/// (for `quill mcp`, which runs in its own process).
pub fn enabled_on_disk(root: &Path) -> bool {
    load(&root.join(SETTINGS_FILE))
}

/// Read the token a running Quill published under `root`.
pub fn read_published_token(root: &Path) -> Option<String> {
    let text = std::fs::read_to_string(token_path(root)).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}
