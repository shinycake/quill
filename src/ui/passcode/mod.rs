//! Local passcode UI (tdesktop `Settings::LocalPasscode`, `AutoLockBox`,
//! `Window::PasscodeLockWidget`): the Settings › Privacy dialog (turn on,
//! change, turn off, auto-lock, Touch ID), the full-window lock screen, the
//! chat-list lock button, auto-lock by idle time, and Cmd/Ctrl+L.
//! The crypto lives in `quill::passcode`; see
//! docs/decisions/codex-local-passcode.md.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Input, InputContentType, InputEvent, InputState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::AccountKey;
use quill::lang::{keys, tr, tr_with};
use quill::passcode::{
    self, AUTOLOCK_PRESETS, IdleTracker, MasterKey, PasscodeError, global_unlock,
};
use quill::platform::{MemorySecretStore, SecretStore, os_secret_store};
use quill::settings::{list_accounts, safe_app_root};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// The dialog's pages (tdesktop opens a separate section per step).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PasscodeView {
    Status,
    Create,
    Change,
    Remove,
}

/// A slow passcode job (PBKDF2, the system prompt). It belongs to the job,
/// not to the box: closing the box leaves it running, so a lock asked
/// meanwhile still waits for the answer (a Turn off cannot land under it).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum PasscodeJob {
    #[default]
    Idle,
    Running,
    /// A lock was asked while it ran: it lands with the answer.
    RunningThenLock,
    /// The lock screen's Log out ran meanwhile: the job's thread still
    /// runs, and its answer is dropped (`run_passcode_job`,
    /// `spawn_system_unlock_wait`).
    Cancelled,
}

impl PasscodeJob {
    pub(crate) fn is_running(self) -> bool {
        self != Self::Idle
    }
}

/// How long the wrong-passcode shake runs.
const SHAKE_MS: u64 = 420;

/// Locked or not, with the number of locks so far (a locked start counts).
/// Something parked for later records the count, and a lock in between
/// drops it (`PasscodeUi::locked_since`), whether or not Quill is still
/// locked when it lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LockState {
    Unlocked { locks: u64 },
    Locked { locks: u64 },
}

impl LockState {
    fn new(locked: bool) -> Self {
        Self::Unlocked { locks: 0 }.set(locked)
    }

    fn locks(self) -> u64 {
        match self {
            Self::Unlocked { locks } | Self::Locked { locks } => locks,
        }
    }

    /// A lock counts once, when it starts.
    fn set(self, locked: bool) -> Self {
        match (self, locked) {
            (Self::Unlocked { locks }, true) => Self::Locked {
                locks: locks.saturating_add(1),
            },
            (Self::Locked { locks }, false) => Self::Unlocked { locks },
            (same, _) => same,
        }
    }
}

pub(crate) struct PasscodeUi {
    /// A passcode is set (cached from `passcode.json`; refreshed after each
    /// change instead of read per frame).
    pub(crate) enabled: bool,
    /// Whether Quill is locked (every window draws only the lock screen,
    /// or nothing: `lock_layer`), with how many times it has locked. One
    /// value, written only by `QuillApp::set_locked` (`set_lock_flag`).
    lock_state: LockState,
    /// The account whose connection waits for the unlock: a locked start,
    /// or a reconnect asked under the lock. The unlock connects it only if
    /// it is still the account on screen, so a switch under the lock (the
    /// lock card's Answer) needs no hand clear.
    pub(crate) connect_at_unlock: Option<quill::ids::AccountKey>,
    /// Offer the system prompt as soon as the lock screen shows.
    suggest_system_unlock: bool,
    pub(crate) open: bool,
    pub(crate) view: PasscodeView,
    pub(crate) old: Entity<InputState>,
    pub(crate) new: Entity<InputState>,
    pub(crate) confirm: Entity<InputState>,
    /// The main window's field on the lock screen; a chat window's is in
    /// its slot, swapped in while that window's state is in place.
    pub(crate) lock: LockField,
    pub(crate) custom_time: Entity<InputState>,
    pub(crate) error: Option<String>,
    pub(crate) lock_error: lock_notice::LockNotice,
    pub(crate) job: PasscodeJob,
    pub(crate) autolock_secs: u32,
    pub(crate) system_unlock: bool,
    pub(crate) logout_confirm: bool,
    shake_started: Option<Instant>,
    idle: IdleTracker,
    /// Master key kept while locked so Touch ID can unlock again without the
    /// passcode (tdesktop: "enter your passcode before you can use Touch
    /// ID"). Only held when the user turned system unlock on.
    system_unlock_key: Option<MasterKey>,
    /// Demo fixtures must not touch the Keychain or the real data folder.
    pub(crate) demo: bool,
    /// What was asked while locked and runs at the unlock (`unlock_intents`).
    pub(in crate::ui) unlock_intents: UnlockIntents,
}

impl PasscodeUi {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        let field = |window: &mut Window, cx: &mut Context<QuillApp>, placeholder: &str| {
            cx.new(|cx| {
                InputState::new(window, cx)
                    .masked(true)
                    .placeholder(placeholder.to_string())
                    .submit_on_enter(true)
            })
        };
        let old = field(window, cx, tr(keys::Q_PREFS_CURRENT_PASSCODE));
        let new = field(window, cx, tr(keys::Q_PREFS_NEW_PASSCODE));
        let confirm = field(window, cx, tr(keys::PASSCODE_CONFIRM_NEW));
        let custom_time = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("10:00")
                .submit_on_enter(true)
        });
        for input in [&old, &new, &confirm] {
            // window-scope: the passcode box is the main window's; a lock it
            // held back reaches every window itself (`in_every_window`).
            cx.subscribe_in(input, window, |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.submit_passcode_form(window, cx);
                }
            })
            .detach();
        }
        cx.subscribe_in(
            &custom_time,
            window,
            |this, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.apply_custom_autolock(cx);
                }
            },
        )
        .detach();
        let root = safe_app_root();
        let config = root.as_deref().and_then(passcode::load_config);
        let enabled = config.is_some();
        let lock = LockField::new(window, cx, enabled);
        // Every window opened from now on draws through the lock layer,
        // the main window's included (its root is built after the app).
        lock_layer::register(cx);
        lock_layer::publish(enabled, cx);
        if enabled {
            quill::agent::access::set_paused_by_lock(true);
        }
        Self {
            enabled,
            // The app starts locked when a passcode is set (tdesktop).
            lock_state: LockState::new(enabled),
            connect_at_unlock: if enabled {
                root.as_deref().map(quill::settings::active_account)
            } else {
                None
            },
            suggest_system_unlock: false,
            open: false,
            view: PasscodeView::Status,
            old,
            new,
            confirm,
            lock,
            custom_time,
            error: None,
            lock_error: Default::default(),
            job: PasscodeJob::Idle,
            autolock_secs: config
                .as_ref()
                .map_or(passcode::DEFAULT_AUTOLOCK_SECS, |c| c.autolock_secs),
            system_unlock: config.as_ref().is_some_and(|c| c.system_unlock),
            logout_confirm: false,
            shake_started: None,
            idle: IdleTracker::default(),
            system_unlock_key: None,
            demo: false,
            unlock_intents: UnlockIntents::default(),
        }
    }

    /// Whether Quill is locked.
    pub(crate) fn is_locked(&self) -> bool {
        matches!(self.lock_state, LockState::Locked { .. })
    }

    /// The lock count now, for something parked until later.
    pub(crate) fn lock_count(&self) -> u64 {
        self.lock_state.locks()
    }

    /// Quill is locked now, or locked at some point since `count` was
    /// read: what was parked then no longer applies.
    pub(crate) fn locked_since(&self, count: u64) -> bool {
        self.is_locked() || self.lock_state.locks() != count
    }

    fn set_lock_flag(&mut self, locked: bool) {
        self.lock_state = self.lock_state.set(locked);
    }

    /// Set the flag alone, for tests that need a locked state without the
    /// lock's closes or the root layer.
    #[cfg(test)]
    pub(crate) fn force_locked(&mut self, locked: bool) {
        self.set_lock_flag(locked);
    }

    fn shake_offset(&self) -> f32 {
        let Some(start) = self.shake_started else {
            return 0.0;
        };
        let t = start.elapsed().as_millis() as f32 / SHAKE_MS as f32;
        if t >= 1.0 {
            return 0.0;
        }
        (t * std::f32::consts::PI * 6.0).sin() * 12.0 * (1.0 - t)
    }

    fn shaking(&self) -> bool {
        self.shake_started
            .is_some_and(|s| s.elapsed() < Duration::from_millis(SHAKE_MS))
    }

    /// "Auto-Lock if away for…" where the OS reports idle time, otherwise
    /// "inactive" (idle measured by input to this window).
    pub(crate) fn autolock_title() -> &'static str {
        if passcode::os_idle_known() {
            tr(keys::Q_PREFS_AUTO_LOCK_IF_AWAY_FOR)
        } else {
            tr(keys::Q_PREFS_AUTO_LOCK_IF_INACTIVE_FOR)
        }
    }
}

fn demo_store() -> Box<dyn SecretStore> {
    Box::new(MemorySecretStore::new())
}

fn account_keys(root: &std::path::Path) -> Vec<AccountKey> {
    list_accounts(root).into_iter().map(|r| r.key).collect()
}

fn error_text(err: &PasscodeError) -> String {
    match err {
        PasscodeError::Flood { retry_in_ms } => tr_with(
            keys::Q_PREFS_TOO_MANY_TRIES_TRY_AGAIN,
            &[("div_ceil", &(retry_in_ms.div_ceil(1000)).to_string())],
        ),
        other => other.to_string(),
    }
}

impl QuillApp {
    fn passcode_root(&self) -> Option<PathBuf> {
        safe_app_root()
    }

    /// Whether the Touch ID / system authentication switch is offered.
    pub(crate) fn system_unlock_offered(&self) -> bool {
        super::system_unlock::available()
    }

    pub(crate) fn open_passcode(&mut self, cx: &mut Context<Self>) {
        if quill::agent::ui_driver::guard::refuse_hard_if_injecting(
            quill::agent::ui_driver::guard::HardSurface::Passcode,
        ) {
            return;
        }
        self.account.passcode.open = true;
        self.open_box(DialogKind::Passcode);
        self.account.passcode.view = PasscodeView::Status;
        self.account.passcode.error = None;
        cx.notify();
    }

    pub(crate) fn close_passcode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.account.passcode.open = false;
        self.account.passcode.view = PasscodeView::Status;
        self.account.passcode.error = None;
        self.clear_passcode_fields(window, cx);
        cx.notify();
    }

    fn clear_passcode_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for input in [
            &self.account.passcode.old,
            &self.account.passcode.new,
            &self.account.passcode.confirm,
        ] {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
    }

    fn goto_passcode_view(
        &mut self,
        view: PasscodeView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.account.passcode.view = view;
        self.account.passcode.error = None;
        self.clear_passcode_fields(window, cx);
        let first = match view {
            PasscodeView::Create => self.account.passcode.new.clone(),
            _ => self.account.passcode.old.clone(),
        };
        first.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    /// Enter in any of the form fields.
    pub(crate) fn submit_passcode_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.account.passcode.job.is_running() || !self.account.passcode.open {
            return;
        }
        match self.account.passcode.view {
            PasscodeView::Status => {}
            PasscodeView::Create => self.submit_create_passcode(window, cx),
            PasscodeView::Change => self.submit_change_passcode(window, cx),
            PasscodeView::Remove => self.submit_remove_passcode(window, cx),
        }
    }

    fn read_field(&self, input: &Entity<InputState>, cx: &App) -> String {
        input.read(cx).value().to_string()
    }

    fn submit_create_passcode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let new = self.read_field(&self.account.passcode.new, cx);
        let confirm = self.read_field(&self.account.passcode.confirm, cx);
        if let Err(err) = passcode::validate_new(&new, &confirm, None) {
            self.account.passcode.error = Some(err.to_string());
            cx.notify();
            return;
        }
        let Some(root) = self.passcode_root() else {
            self.account.passcode.error = Some(tr(keys::Q_PREFS_NO_APP_DATA_FOLDER_IS).into());
            cx.notify();
            return;
        };
        let demo = self.account.passcode.demo;
        self.run_passcode_job(
            move || {
                let inner: Box<dyn SecretStore> = if demo {
                    demo_store()
                } else {
                    os_secret_store()
                };
                let accounts = account_keys(&root);
                let state = global_unlock();
                passcode::enable(
                    &root,
                    inner.as_ref(),
                    &accounts,
                    &new,
                    passcode::KDF_ITERATIONS,
                    &state,
                )
            },
            |this, result, window, cx| match result {
                Ok(()) => {
                    this.account.passcode.enabled = true;
                    this.account.passcode.autolock_secs = passcode::DEFAULT_AUTOLOCK_SECS;
                    quill::wake::PASSCODE.poke();
                    this.goto_passcode_view(PasscodeView::Status, window, cx);
                    this.set_note(StatusNote::silent(tr(
                        keys::Q_PREFS_LOCAL_PASSCODE_TURNED_ON,
                    )));
                }
                Err(err) => this.account.passcode.error = Some(error_text(&err)),
            },
            window,
            cx,
        );
    }

    fn submit_change_passcode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let old = self.read_field(&self.account.passcode.old, cx);
        let new = self.read_field(&self.account.passcode.new, cx);
        let confirm = self.read_field(&self.account.passcode.confirm, cx);
        if old.is_empty() {
            self.account.passcode.error =
                Some(tr(keys::Q_PREFS_ENTER_YOUR_CURRENT_PASSCODE).into());
            cx.notify();
            return;
        }
        if let Err(err) = passcode::validate_new(&new, &confirm, Some(&old)) {
            self.account.passcode.error = Some(err.to_string());
            cx.notify();
            return;
        }
        let Some(root) = self.passcode_root() else {
            return;
        };
        self.run_passcode_job(
            move || {
                passcode::change(
                    &root,
                    &old,
                    &new,
                    passcode::KDF_ITERATIONS,
                    &global_unlock(),
                    passcode::unix_ms_now(),
                )
            },
            |this, result, window, cx| match result {
                Ok(()) => {
                    this.goto_passcode_view(PasscodeView::Status, window, cx);
                    this.set_note(StatusNote::silent(tr(keys::Q_PREFS_LOCAL_PASSCODE_CHANGED)));
                }
                Err(err) => this.account.passcode.error = Some(error_text(&err)),
            },
            window,
            cx,
        );
    }

    fn submit_remove_passcode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let old = self.read_field(&self.account.passcode.old, cx);
        if old.is_empty() {
            self.account.passcode.error =
                Some(tr(keys::Q_PREFS_ENTER_YOUR_CURRENT_PASSCODE).into());
            cx.notify();
            return;
        }
        let Some(root) = self.passcode_root() else {
            return;
        };
        let demo = self.account.passcode.demo;
        self.run_passcode_job(
            move || {
                let inner: Box<dyn SecretStore> = if demo {
                    demo_store()
                } else {
                    os_secret_store()
                };
                let accounts = account_keys(&root);
                passcode::disable(
                    &root,
                    inner.as_ref(),
                    &accounts,
                    &old,
                    &global_unlock(),
                    passcode::unix_ms_now(),
                )
            },
            |this, result, window, cx| match result {
                Ok(()) => {
                    this.account.passcode.enabled = false;
                    quill::wake::PASSCODE.poke();
                    this.account.passcode.system_unlock = false;
                    this.account.passcode.system_unlock_key = None;
                    this.goto_passcode_view(PasscodeView::Status, window, cx);
                    this.set_note(StatusNote::silent(tr(
                        keys::Q_PREFS_LOCAL_PASSCODE_TURNED_OFF,
                    )));
                }
                Err(err) => this.account.passcode.error = Some(error_text(&err)),
            },
            window,
            cx,
        );
    }

    /// Write a passcode setting: `true` when it is saved (a demo has
    /// nothing to save). A refused write says why in the box, so the screen
    /// never shows a setting the next start does not have.
    fn save_passcode_setting(
        &mut self,
        write: impl FnOnce(&std::path::Path) -> Result<(), PasscodeError>,
    ) -> bool {
        if self.account.passcode.demo {
            return true;
        }
        let saved = match self.passcode_root() {
            Some(root) => write(&root).map_err(|err| err.to_string()),
            None => Err(tr(keys::Q_PREFS_NO_APP_DATA_FOLDER_IS).to_string()),
        };
        match saved {
            Ok(()) => true,
            Err(message) => {
                self.account.passcode.error = Some(message);
                false
            }
        }
    }

    pub(crate) fn set_autolock_secs(&mut self, secs: u32, cx: &mut Context<Self>) {
        if self.save_passcode_setting(|root| passcode::set_autolock(root, secs).map(|_| ())) {
            self.account.passcode.autolock_secs = secs;
            quill::wake::PASSCODE.poke();
        }
        cx.notify();
    }

    fn apply_custom_autolock(&mut self, cx: &mut Context<Self>) {
        let text = self
            .account
            .passcode
            .custom_time
            .read(cx)
            .value()
            .to_string();
        match passcode::parse_hhmm(&text) {
            Some(secs) => {
                self.account.passcode.error = None;
                self.set_autolock_secs(secs, cx);
            }
            None => {
                self.account.passcode.error = Some(tr(keys::Q_PREFS_ENTER_A_TIME_AS_HOURS).into());
                cx.notify();
            }
        }
    }

    pub(crate) fn set_system_unlock(&mut self, on: bool, cx: &mut Context<Self>) {
        if !self.save_passcode_setting(|root| passcode::set_system_unlock(root, on).map(|_| ())) {
            cx.notify();
            return;
        }
        self.account.passcode.system_unlock = on;
        if !on {
            self.account.passcode.system_unlock_key = None;
        } else {
            // The master key is in memory while unlocked; keep it for the
            // next lock so the system prompt can unlock again.
            self.account.passcode.system_unlock_key = global_unlock().key();
        }
        cx.notify();
    }

    /// Lock now (lock button, Cmd/Ctrl+L, tray, auto-lock).
    pub(crate) fn lock_by_passcode(&mut self, cx: &mut Context<Self>) {
        if quill::agent::ui_driver::guard::refuse_hard_if_injecting(
            quill::agent::ui_driver::guard::HardSurface::Lock,
        ) {
            return;
        }
        if !self.account.passcode.enabled || self.account.passcode.is_locked() {
            return;
        }
        // A Turn off or Change in flight would land under the lock: Turn
        // off would leave Quill locked with no passcode to unlock it. The
        // lock waits for the job's answer.
        if self.account.passcode.job.is_running() {
            self.account.passcode.job = PasscodeJob::RunningThenLock;
            return;
        }
        let state = global_unlock();
        if self.account.passcode.system_unlock {
            self.account.passcode.system_unlock_key = state.key();
        }
        // The in-memory master key goes away: unlocking needs the passcode
        // (or the system prompt that still holds it for this process).
        state.clear();
        self.set_locked(true, cx);
        self.reset_lock_fields();
        self.account.passcode.suggest_system_unlock =
            self.account.passcode.system_unlock_key.is_some();
        self.account.passcode.lock_error = Default::default();
        self.account.passcode.logout_confirm = false;
        self.close_for_lock(cx);
        cx.notify();
    }

    fn unlock_done(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_locked(false, cx);
        self.account.passcode.lock_error = Default::default();
        self.account.passcode.logout_confirm = false;
        self.account
            .passcode
            .lock
            .input
            .update(cx, |input, cx| input.set_value("", window, cx));
        // The other windows' fields empty at their next frame.
        self.clear_lock_fields();
        if self.account.passcode.system_unlock {
            self.account.passcode.system_unlock_key = global_unlock().key();
        }
        super::presence::note_input();
        self.resume_after_unlock(cx);
        cx.notify();
    }

    fn lock_failed(&mut self, message: String, cx: &mut Context<Self>) {
        self.account.passcode.lock_error = lock_notice::LockNotice::Wrong(message);
        quill::wake::PASSCODE.poke();
        self.account.passcode.shake_started = Some(Instant::now());
        self.account.passcode.lock.refocus();
        cx.notify();
    }

    /// Submit on the lock screen.
    pub(crate) fn submit_unlock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.account.passcode.is_locked() || self.account.passcode.job.is_running() {
            return;
        }
        let typed = self.read_field(&self.account.passcode.lock.input, cx);
        if typed.is_empty() {
            self.lock_failed(tr(keys::Q_PREFS_ENTER_YOUR_PASSCODE).into(), cx);
            return;
        }
        if self.account.passcode.demo {
            // Fixtures have no real passcode on disk.
            self.lock_failed(PasscodeError::Wrong.to_string(), cx);
            return;
        }
        let Some(root) = self.passcode_root() else {
            return;
        };
        self.run_passcode_job(
            move || {
                passcode::unlock(&root, &global_unlock(), &typed, passcode::unix_ms_now())
                    .map(|_| ())
            },
            Self::unlock_answered,
            window,
            cx,
        );
    }

    /// The lock screen's passcode was checked, with the state of the
    /// window it was typed in in place (`run_passcode_job`).
    pub(in crate::ui) fn unlock_answered(
        &mut self,
        result: Result<(), PasscodeError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(()) => self.unlock_done(window, cx),
            Err(PasscodeError::Wrong) => {
                self.account
                    .passcode
                    .lock
                    .input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.lock_failed(PasscodeError::Wrong.to_string(), cx);
            }
            Err(err @ PasscodeError::Flood { retry_in_ms }) => {
                self.lock_failed(error_text(&err), cx);
                self.account.passcode.lock_error = lock_notice::LockNotice::Flood {
                    until: Instant::now() + Duration::from_millis(retry_in_ms),
                    text: error_text(&err),
                };
            }
            Err(err) => self.lock_failed(err.to_string(), cx),
        }
    }

    /// Touch ID / system password on the lock screen.
    pub(crate) fn unlock_with_system(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(master) = self.account.passcode.system_unlock_key.clone() else {
            self.lock_failed(tr(keys::Q_PREFS_ENTER_YOUR_PASSCODE_ONCE_BEFORE).into(), cx);
            return;
        };
        if self.account.passcode.job.is_running() {
            return;
        }
        self.account.passcode.job = PasscodeJob::Running;
        let receiver = super::system_unlock::authenticate(tr(keys::Q_PREFS_UNLOCK_QUILL));
        self.spawn_system_unlock_wait(receiver, master, window, cx);
    }

    /// Auto-lock by idle time, and the flood countdown on the lock screen.
    /// Returns how long to sleep before the next check: until the lock
    /// deadline (re-read from the idle time then, so activity re-arms it),
    /// a second during a flood countdown, or `None` when there is nothing
    /// to time and the task parks until [`quill::wake::PASSCODE`].
    pub(crate) fn passcode_tick(&mut self, cx: &mut Context<Self>) -> Option<Duration> {
        let ui = &mut self.account.passcode;
        if ui.is_locked() {
            let flood = ui.lock_error.flood_until();
            if flood.is_some_and(|t| Instant::now() >= t) {
                ui.lock_error = Default::default();
                cx.notify();
            } else if flood.is_some() {
                cx.notify();
            }
            return flood.map(|_| Duration::from_secs(1));
        }
        if !ui.enabled || ui.demo {
            return None;
        }
        // OS idle where available ("away"), else input to this window.
        let idle = passcode::os_idle_ms().unwrap_or_else(super::presence::idle_ms);
        let effective = ui.idle.effective_idle(passcode::unix_ms_now(), idle);
        if passcode::autolock_due(effective, ui.autolock_secs) {
            self.lock_by_passcode(cx);
            return None;
        }
        let wait = passcode::autolock_sleep_ms(
            true,
            false,
            self.account.passcode.autolock_secs,
            effective,
        )?;
        self.account.passcode.idle.plan_sleep(wait);
        Some(Duration::from_millis(wait))
    }

    pub(crate) fn spawn_passcode_tick(&mut self, cx: &mut Context<Self>) {
        // screen-scope: the auto-lock timer belongs to the app and only reads the
        // passcode state, so it runs whichever account is on screen.
        Self::spawn_app(cx, async move |this, cx| {
            // Parked: nothing to time, so the next tick follows a long gap.
            let mut parked = true;
            loop {
                let seen = quill::wake::PASSCODE.epoch();
                let step = this.update(cx, |this, cx| {
                    if parked {
                        this.account.passcode.idle.reset();
                    }
                    this.passcode_tick(cx)
                });
                let Ok(wait) = step else {
                    break;
                };
                parked = wait.is_none();
                // Parked, a poke (settings, lock, unlock) wakes the task;
                // the long timeout is only a safety net.
                super::wake_wait::sleep_or_wake(
                    cx.background_executor(),
                    &quill::wake::PASSCODE,
                    seen,
                    wait.unwrap_or(Duration::from_secs(300)),
                )
                .await;
            }
        })
        .detach();
    }

    /// Tray "Lock Quill": lock, or show the passcode settings when none
    /// is set yet.
    pub fn lock_from_tray(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.account.passcode.enabled {
            self.lock_by_passcode(cx);
        } else {
            self.open_passcode(cx);
        }
        cx.activate(true);
        window.activate_window();
    }

    /// The chat-list header lock button (tdesktop shows it while a passcode
    /// is set).
    pub(crate) fn lock_button(&self, cx: &mut Context<Self>) -> AnyElement {
        Button::new("lock-app")
            .icon(gpui_kit::assets::IconName::Lock)
            .ghost()
            .tooltip(tr(keys::Q_PREFS_LOCK_QUILL))
            .accessibility_label(tr(keys::Q_PREFS_LOCK_QUILL))
            .on_click(cx.listener(|this, _, _, cx| this.lock_by_passcode(cx)))
            .into_any_element()
    }

    /// Per-frame work for the lock screen (focus, shake clock).
    pub(crate) fn passcode_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let locked = self.account.passcode.is_locked();
        self.account.passcode.lock.frame(locked, window, cx);
        if locked {
            if std::mem::take(&mut self.account.passcode.suggest_system_unlock) {
                self.unlock_with_system(window, cx);
            }
            if self.account.passcode.shaking() {
                self.request_animation_tick(60, cx);
            }
        }
    }

    pub(crate) fn lock_overlay(&self, cx: &mut Context<Self>) -> AnyElement {
        let ui = &self.account.passcode;
        let muted = cx.theme().muted_foreground;
        let waiting = ui.lock_error.flood_until().is_some();
        let shake = ui.shake_offset();
        let system = ui.system_unlock && self.system_unlock_offered();
        let mut column = div()
            .w(px(320.))
            .flex()
            .flex_col()
            .items_center()
            .gap_3()
            .child(
                div()
                    .size(px(72.))
                    .rounded_full()
                    .bg(cx.theme().primary.opacity(0.12))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        gpui_kit::component::Icon::new(gpui_kit::assets::IconName::Lock)
                            .size(px(32.))
                            .text_color(cx.theme().primary),
                    ),
            )
            .child(
                div()
                    .id("lock-title")
                    .role(Role::Heading)
                    .aria_label(tr(keys::PASSCODE_ENTER))
                    .text_xl()
                    .font_semibold()
                    .child(tr(keys::PASSCODE_ENTER)),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(tr(keys::Q_PREFS_UNLOCK_QUILL_TO_CONTINUE)),
            )
            .child(
                div().w_full().relative().left(px(shake)).child(
                    Input::new(&ui.lock.input)
                        .aria_label(tr(keys::SETTINGS_PASSCODE_TITLE))
                        .content_type(InputContentType::Password)
                        .h(px(40.)),
                ),
            )
            .child(
                div()
                    .id("lock-error")
                    .role(Role::Status)
                    .min_h(px(20.))
                    .text_sm()
                    .text_color(danger())
                    .child(ui.lock_error.text().unwrap_or_default().to_string()),
            )
            .child(
                Button::new("lock-submit")
                    .label(tr(keys::PASSCODE_SUBMIT))
                    .primary()
                    .w_full()
                    .loading(ui.job.is_running())
                    .disabled(ui.job.is_running() || waiting)
                    .on_click(cx.listener(|this, _, window, cx| this.submit_unlock(window, cx))),
            );
        if system {
            column = column.child(
                Button::new("lock-system-unlock")
                    .label(super::system_unlock::label())
                    .ghost()
                    .disabled(ui.job.is_running())
                    .on_click(
                        cx.listener(|this, _, window, cx| this.unlock_with_system(window, cx)),
                    ),
            );
        }
        column = if ui.logout_confirm {
            column.child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_3()
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .text_sm()
                            .child(tr(keys::Q_PREFS_LOG_OUT_AND_DELETE_LOCAL)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(tr(keys::Q_PREFS_WITHOUT_THE_PASSCODE_THE_SAVED)),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .justify_end()
                            .child(
                                Button::new("lock-logout-cancel")
                                    .label(tr(keys::CANCEL))
                                    .ghost()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.account.passcode.logout_confirm = false;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("lock-logout-confirm")
                                    .label(tr(keys::PASSCODE_LOGOUT))
                                    .danger()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.confirm_lock_logout(window, cx)
                                    })),
                            ),
                    ),
            )
        } else {
            column.child(
                Button::new("lock-logout")
                    .label(tr(keys::PASSCODE_LOGOUT))
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.account.passcode.logout_confirm = true;
                        cx.notify();
                    })),
            )
        };
        div()
            .id("passcode-lock")
            .track_focus(&ui.lock.screen)
            .absolute()
            .inset_0()
            .occlude()
            .bg(cx.theme().background)
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .child(column)
            .into_any_element()
    }
}

crate::ui::shell::register_dialogs! {
    /// Local passcode settings.
    Passcode => DialogSpec::new(
        200,
        |app| app.account.passcode.open,
        QuillApp::build_passcode_dialog,
    )
    .close(|app, window, cx| app.close_passcode(window, cx)),
}

mod build_passcode_dialog;
mod job;
mod lock_field;
mod lock_gate;
mod lock_logout;
mod lock_notice;
mod unlock_intents;
pub(in crate::ui) use unlock_intents::UnlockIntents;
pub(crate) mod lock_layer;
mod system_unlock_wait;
pub(crate) use lock_field::LockField;
pub(in crate::ui) use lock_gate::intercept_keys_unlocked;
#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod lock_gate_tests;
#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod lock_layer_tests;
#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod lock_waits_tests;
