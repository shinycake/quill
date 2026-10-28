//! The Agent access switches (Settings > Agent access) and the token agents
//! authenticate with.
//!
//! Off by default. While it is on, a fresh random token sits in
//! `agent-token` in the app data directory, readable by the owner only; it
//! changes on every launch of Quill (and on "Reset token") and is deleted
//! when the switch goes off. `quill mcp` reads it and presents it on the
//! socket, so only a process running as this user can connect.

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

const SETTINGS_FILE: &str = "agent_access.json";
const TOKEN_FILE: &str = "agent-token";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Agents may send raw TDLib requests (`td_call`). On by default, as
    /// before the switch existed, so an old settings file reads as on.
    raw_calls: bool,
}

impl Default for Stored {
    fn default() -> Self {
        Self {
            enabled: false,
            confirm_sends: false,
            ui_driver: false,
            takeover_effect: TakeoverEffect::default(),
            raw_calls: true,
        }
    }
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
/// Bumped whenever the switch goes off or the token is reset, so open
/// connections notice.
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
        super::activity::allow_all_again();
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

/// Whether agents may send raw TDLib requests (`td_call`, "Raw TDLib
/// calls" on the Agent access page). No other tool depends on it.
pub fn raw_calls() -> bool {
    stored().raw_calls
}

pub fn set_raw_calls(on: bool) {
    if super::ui_driver::guard::refuse_if_injecting("change the Agent access settings") {
        return;
    }
    save(|stored| stored.raw_calls = on);
    crate::wake::AGENT.poke();
}

/// "Reset token": a new token, and every open agent connection ends as it
/// does when the switch goes off, while Agent access stays on. An agent
/// set up with `quill mcp` reads the new token when it reconnects;
/// anything that kept a copy of the old one can't connect again.
pub fn reset_token() {
    if super::ui_driver::guard::refuse_if_injecting("change the Agent access settings") {
        return;
    }
    renew_token();
    GENERATION.fetch_add(1, Ordering::SeqCst);
    super::prompts::deny_all();
    super::ui_driver::bridge::refuse_all(
        "The user reset the agent token in Quill, so nothing was sent to its window. Reconnect, then retry.",
    );
    crate::wake::AGENT.poke();
    publish_token();
}

/// The switch's generation; a connection opened under one generation ends
/// once it changes.
pub fn generation() -> u64 {
    GENERATION.load(Ordering::SeqCst)
}

static TOKEN: RwLock<Option<String>> = RwLock::new(None);

fn fresh_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// This launch's token, made on first use and replaced by [`reset_token`].
/// Empty only when its lock is poisoned, and then nothing matches it.
fn token() -> String {
    if let Ok(guard) = TOKEN.read()
        && let Some(token) = guard.as_ref()
    {
        return token.clone();
    }
    match TOKEN.write() {
        Ok(mut guard) => guard.get_or_insert_with(fresh_token).clone(),
        Err(_) => String::new(),
    }
}

fn renew_token() {
    if let Ok(mut guard) = TOKEN.write() {
        *guard = Some(fresh_token());
    }
}

/// Whether `presented` is this launch's token, compared in constant time.
pub fn token_matches(presented: &str) -> bool {
    let expected = token();
    let expected = expected.as_bytes();
    let presented = presented.as_bytes();
    if expected.is_empty() || presented.len() != expected.len() {
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

    #[test]
    fn raw_calls_stay_on_for_old_settings_and_save_off() {
        let old: Stored =
            serde_json::from_str(r#"{"enabled":true}"#).unwrap_or_else(|_| panic!("parses"));
        assert!(old.raw_calls, "a file from before the switch keeps td_call");
        assert!(Stored::default().raw_calls);
        let off = Stored {
            raw_calls: false,
            ..Stored::default()
        };
        let json = serde_json::to_string(&off).unwrap_or_default();
        let back: Stored = serde_json::from_str(&json).unwrap_or_default();
        assert_eq!(back, off);
    }

    #[test]
    fn off_and_on_again_keeps_grants_log_token_and_switches() {
        use crate::agent::{activity, grants};
        use crate::ids::AccountKey;
        let _guard = crate::locks::lock_or_recover(&super::TEST_LOCK);
        crate::fast_buttons::set_persistence(false);
        let account = AccountKey::primary();
        super::set_enabled(true);
        super::set_confirm_sends(true);
        super::set_ui_driver(true);
        super::set_raw_calls(false);
        super::set_takeover_effect(TakeoverEffect::StillGlow);
        grants::update(&account, |g| g.contacts = true);
        activity::record(activity::Action {
            id: 0,
            time: 0,
            account: account.clone(),
            agent: "Claude Code".into(),
            tool: "send_message".into(),
            chat_id: 7,
            chat_title: "Ann".into(),
            summary: "kept while off".into(),
            ok: true,
            declined: false,
            message_ids: Vec::new(),
            undone: false,
        });
        let token = super::token();
        let kept = |when: &str| {
            assert!(grants::for_account(&account).contacts, "{when}: grants");
            assert!(
                activity::recent(&account)
                    .iter()
                    .any(|a| a.summary == "kept while off"),
                "{when}: activity log"
            );
            assert!(super::confirm_sends() && super::ui_driver(), "{when}");
            assert!(!super::raw_calls(), "{when}: raw calls");
            assert_eq!(super::takeover_effect(), TakeoverEffect::StillGlow);
            assert!(super::token_matches(&token), "{when}: token");
        };

        super::set_enabled(false);
        assert!(!super::enabled());
        kept("off");
        super::set_enabled(true);
        kept("on again");

        super::set_confirm_sends(false);
        super::set_ui_driver(false);
        super::set_raw_calls(true);
        super::set_takeover_effect(TakeoverEffect::Automatic);
        grants::update(&account, |g| g.contacts = false);
        super::set_enabled(false);
    }

    #[test]
    fn a_reset_token_replaces_the_old_one_and_ends_connections() {
        let _guard = crate::locks::lock_or_recover(&super::TEST_LOCK);
        let before = super::token();
        let generation = super::generation();
        assert!(super::token_matches(&before));
        super::renew_token();
        super::GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let after = super::token();
        assert_ne!(before, after);
        assert!(!super::token_matches(&before), "the old token is refused");
        assert!(super::token_matches(&after));
        assert!(super::generation() > generation, "open connections end");
        assert!(!super::token_matches(""));
    }
}
