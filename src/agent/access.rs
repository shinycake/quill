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

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
struct Stored {
    enabled: bool,
    /// Every write waits for the user's click in Quill.
    confirm_sends: bool,
    /// Agents may read Quill's window and click, type and scroll in it
    /// ("Let agents drive Quill's window", `super::ui_driver`).
    ui_driver: bool,
    /// How the glow moves while an agent drives ("Takeover effect").
    takeover_effect: TakeoverEffect,
}

/// "Takeover effect": whether the light sweeps round the glow while an
/// agent drives the window. Reduced motion always keeps it still.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TakeoverEffect {
    /// Sweeps unless Quill's Power Saving or the system's low-power mode
    /// says to save.
    #[default]
    Automatic,
    /// Always sweeps.
    GlowAndSweep,
    /// Never sweeps: the still glow only.
    StillGlow,
}

impl TakeoverEffect {
    /// Whether the light sweeps, given reduced motion (the system's, or
    /// `QUILL_REDUCED_MOTION`) and whether power is being saved (Quill's
    /// Power Saving for interface animations, which a battery saver or
    /// heat forces, or the system's low-power mode).
    pub const fn sweeps(self, reduce_motion: bool, saving_power: bool) -> bool {
        match self {
            Self::Automatic => !reduce_motion && !saving_power,
            Self::GlowAndSweep => !reduce_motion,
            Self::StillGlow => false,
        }
    }
}

/// The "Takeover effect" preference.
pub fn takeover_effect() -> TakeoverEffect {
    stored().takeover_effect
}

pub fn set_takeover_effect(effect: TakeoverEffect) {
    if super::ui_driver::guard::refuse_if_injecting("change the Agent access settings") {
        return;
    }
    save(|stored| stored.takeover_effect = effect);
}

static STORED: RwLock<Option<Stored>> = RwLock::new(None);
/// Lib tests that turn the switch (which denies every waiting prompt) or
/// wait on prompts hold this, so they don't run into each other.
#[cfg(test)]
pub(crate) static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
/// Bumped whenever the switch goes off, so open connections notice.
static GENERATION: AtomicU64 = AtomicU64::new(0);

fn root() -> Option<PathBuf> {
    crate::settings::safe_app_root()
}

fn load(path: &Path) -> Stored {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Stored>(&bytes).ok())
        .unwrap_or_default()
}

fn stored() -> Stored {
    if let Ok(guard) = STORED.read()
        && let Some(stored) = *guard
    {
        return stored;
    }
    let loaded = root()
        .map(|root| load(&root.join(SETTINGS_FILE)))
        .unwrap_or_default();
    if let Ok(mut guard) = STORED.write() {
        return *guard.get_or_insert(loaded);
    }
    loaded
}

fn save(change: impl FnOnce(&mut Stored)) {
    let mut next = stored();
    change(&mut next);
    if crate::fast_buttons::persistence_enabled()
        && let Some(root) = root()
        && let Err(err) = crate::settings::write_json_atomic(&root.join(SETTINGS_FILE), &next)
    {
        eprintln!("quill: could not save {SETTINGS_FILE}: {err}");
    }
    if let Ok(mut guard) = STORED.write() {
        *guard = Some(next);
    }
}

/// Whether agents may connect.
pub fn enabled() -> bool {
    stored().enabled
}

/// Turn agent access on or off. Off deletes the token and ends every open
/// agent connection.
pub fn set_enabled(on: bool) {
    if super::ui_driver::guard::refuse_if_injecting("change the Agent access settings") {
        return;
    }
    save(|stored| stored.enabled = on);
    if !on {
        GENERATION.fetch_add(1, Ordering::SeqCst);
        super::ui_driver::takeover::forget_hold();
        super::prompts::deny_all();
        super::ui_driver::bridge::refuse_all(
            "Agent access was turned off in Quill, so nothing was sent to its window.",
        );
    }
    crate::wake::AGENT.poke();
    publish_token();
}

/// Whether each write an agent makes waits for the user's click in Quill.
pub fn confirm_sends() -> bool {
    stored().confirm_sends
}

pub fn set_confirm_sends(on: bool) {
    if super::ui_driver::guard::refuse_if_injecting("change the Agent access settings") {
        return;
    }
    save(|stored| stored.confirm_sends = on);
    crate::wake::AGENT.poke();
}

/// Whether agents may drive Quill's window (read its element tree, click,
/// type and scroll). Off by default; needs the main switch too.
pub fn ui_driver() -> bool {
    stored().ui_driver
}

pub fn set_ui_driver(on: bool) {
    if super::ui_driver::guard::refuse_if_injecting("change the Agent access settings") {
        return;
    }
    save(|stored| stored.ui_driver = on);
    if !on {
        // Off and on again is how the user lifts a Stop or a pause.
        super::ui_driver::takeover::forget_hold();
    }
    crate::wake::AGENT.poke();
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
pub(crate) fn write_owner_only(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
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
    load(&root.join(SETTINGS_FILE)).enabled
}

/// Read the token a running Quill published under `root`.
pub fn read_published_token(root: &Path) -> Option<String> {
    let text = std::fs::read_to_string(token_path(root)).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::{Stored, TakeoverEffect};

    #[test]
    fn the_takeover_effect_sweeps_per_mode_and_reduced_motion_wins() {
        use TakeoverEffect::*;
        // (reduce motion, saving power): Automatic, Glow and sweep, Still glow.
        let table = [
            ((false, false), [true, true, false]),
            ((false, true), [false, true, false]),
            ((true, false), [false, false, false]),
            ((true, true), [false, false, false]),
        ];
        for ((reduce, saving), want) in table {
            let got = [Automatic, GlowAndSweep, StillGlow].map(|e| e.sweeps(reduce, saving));
            assert_eq!(got, want, "reduce {reduce}, saving {saving}");
        }
    }

    #[test]
    fn the_takeover_effect_defaults_to_automatic_in_old_settings() {
        let old: Stored = serde_json::from_str(r#"{"enabled":true,"ui_driver":true}"#)
            .unwrap_or_else(|_| panic!("parses"));
        assert_eq!(old.takeover_effect, TakeoverEffect::Automatic);
        let still: Stored = serde_json::from_str(r#"{"takeover_effect":"still_glow"}"#)
            .unwrap_or_else(|_| panic!("parses"));
        assert_eq!(still.takeover_effect, TakeoverEffect::StillGlow);
    }
}
