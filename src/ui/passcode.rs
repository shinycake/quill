//! Local passcode UI (tdesktop `Settings::LocalPasscode`, `AutoLockBox`,
//! `Window::PasscodeLockWidget`): the Settings › Privacy dialog (turn on,
//! change, turn off, auto-lock, Touch ID), the full-window lock screen, the
//! chat-list lock button, auto-lock by idle time, and Cmd/Ctrl+L.
//! The crypto lives in `quill::passcode`; see
//! docs/decisions/codex-local-passcode.md.

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Input, InputContentType, InputEvent, InputState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::AccountKey;
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

/// How long the wrong-passcode shake runs.
const SHAKE_MS: u64 = 420;

pub(crate) struct PasscodeUi {
    /// A passcode is set (cached from `passcode.json`; refreshed after each
    /// change instead of read per frame).
    pub(crate) enabled: bool,
    /// The lock screen covers the window.
    pub(crate) locked: bool,
    /// Startup was locked: the connection starts after the first unlock.
    pub(crate) deferred_connect: bool,
    /// Focus the lock field on the next frame.
    focus_lock_input: bool,
    /// Offer the system prompt as soon as the lock screen shows.
    suggest_system_unlock: bool,
    pub(crate) open: bool,
    pub(crate) view: PasscodeView,
    pub(crate) old: Entity<InputState>,
    pub(crate) new: Entity<InputState>,
    pub(crate) confirm: Entity<InputState>,
    pub(crate) lock_input: Entity<InputState>,
    pub(crate) custom_time: Entity<InputState>,
    pub(crate) error: Option<String>,
    pub(crate) lock_error: Option<String>,
    pub(crate) busy: bool,
    pub(crate) autolock_secs: u32,
    pub(crate) system_unlock: bool,
    pub(crate) logout_confirm: bool,
    shake_started: Option<Instant>,
    /// Persisted flood wait shown on the lock screen.
    flood_until: Option<Instant>,
    idle: IdleTracker,
    /// Master key kept while locked so Touch ID can unlock again without the
    /// passcode (tdesktop: "enter your passcode before you can use Touch
    /// ID"). Only held when the user turned system unlock on.
    system_unlock_key: Option<MasterKey>,
    /// Demo fixtures must not touch the Keychain or the real data folder.
    pub(crate) demo: bool,
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
        let old = field(window, cx, "Current passcode");
        let new = field(window, cx, "New passcode");
        let confirm = field(window, cx, "Re-enter new passcode");
        let lock_input = field(window, cx, "Your passcode");
        let custom_time = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("10:00")
                .submit_on_enter(true)
        });
        for input in [&old, &new, &confirm] {
            cx.subscribe_in(input, window, |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.submit_passcode_form(window, cx);
                }
            })
            .detach();
        }
        cx.subscribe_in(
            &lock_input,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => this.submit_unlock(window, cx),
                InputEvent::Change if this.passcode_ui.lock_error.take().is_some() => {
                    cx.notify();
                }
                _ => {}
            },
        )
        .detach();
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
        Self {
            enabled,
            // The app starts locked when a passcode is set (tdesktop).
            locked: enabled,
            deferred_connect: enabled,
            focus_lock_input: enabled,
            suggest_system_unlock: false,
            open: false,
            view: PasscodeView::Status,
            old,
            new,
            confirm,
            lock_input,
            custom_time,
            error: None,
            lock_error: None,
            busy: false,
            autolock_secs: config
                .as_ref()
                .map_or(passcode::DEFAULT_AUTOLOCK_SECS, |c| c.autolock_secs),
            system_unlock: config.as_ref().is_some_and(|c| c.system_unlock),
            logout_confirm: false,
            shake_started: None,
            flood_until: None,
            idle: IdleTracker::default(),
            system_unlock_key: None,
            demo: false,
        }
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
            "Auto-lock if away for"
        } else {
            "Auto-lock if inactive for"
        }
    }

    /// Demo/test fixture: show the lock screen without a real passcode.
    pub(crate) fn fixture(&mut self, enabled: bool, locked: bool, error: Option<&str>) {
        self.demo = true;
        self.enabled = enabled;
        self.locked = locked;
        self.lock_error = error.map(str::to_string);
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
        PasscodeError::Flood { retry_in_ms } => {
            format!(
                "Too many tries. Try again in {} s.",
                retry_in_ms.div_ceil(1000)
            )
        }
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
        self.passcode_ui.open = true;
        self.passcode_ui.view = PasscodeView::Status;
        self.passcode_ui.error = None;
        cx.notify();
    }

    pub(crate) fn close_passcode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.passcode_ui.open = false;
        self.passcode_ui.view = PasscodeView::Status;
        self.passcode_ui.error = None;
        self.passcode_ui.busy = false;
        self.clear_passcode_fields(window, cx);
        cx.notify();
    }

    fn clear_passcode_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for input in [
            &self.passcode_ui.old,
            &self.passcode_ui.new,
            &self.passcode_ui.confirm,
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
        self.passcode_ui.view = view;
        self.passcode_ui.error = None;
        self.clear_passcode_fields(window, cx);
        let first = match view {
            PasscodeView::Create => self.passcode_ui.new.clone(),
            _ => self.passcode_ui.old.clone(),
        };
        first.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    /// Enter in any of the form fields.
    pub(crate) fn submit_passcode_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.passcode_ui.busy || !self.passcode_ui.open {
            return;
        }
        match self.passcode_ui.view {
            PasscodeView::Status => {}
            PasscodeView::Create => self.submit_create_passcode(window, cx),
            PasscodeView::Change => self.submit_change_passcode(window, cx),
            PasscodeView::Remove => self.submit_remove_passcode(window, cx),
        }
    }

    fn read_field(&self, input: &Entity<InputState>, cx: &App) -> String {
        input.read(cx).value().to_string()
    }

    /// Run a slow passcode operation (PBKDF2) off the UI thread.
    fn run_passcode_job<T: Send + 'static>(
        &mut self,
        job: impl FnOnce() -> T + Send + 'static,
        done: impl FnOnce(&mut Self, T, &mut Window, &mut Context<Self>) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.passcode_ui.busy = true;
        self.passcode_ui.error = None;
        let task = cx.background_executor().spawn(async move { job() });
        cx.spawn_in(window, async move |this, cx| {
            let out = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.passcode_ui.busy = false;
                done(this, out, window, cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn submit_create_passcode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let new = self.read_field(&self.passcode_ui.new, cx);
        let confirm = self.read_field(&self.passcode_ui.confirm, cx);
        if let Err(err) = passcode::validate_new(&new, &confirm, None) {
            self.passcode_ui.error = Some(err.to_string());
            cx.notify();
            return;
        }
        let Some(root) = self.passcode_root() else {
            self.passcode_ui.error = Some("No app data folder is available.".into());
            cx.notify();
            return;
        };
        let demo = self.passcode_ui.demo;
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
                    this.passcode_ui.enabled = true;
                    this.passcode_ui.autolock_secs = passcode::DEFAULT_AUTOLOCK_SECS;
                    this.goto_passcode_view(PasscodeView::Status, window, cx);
                    this.status_note = "Local passcode turned on".into();
                }
                Err(err) => this.passcode_ui.error = Some(error_text(&err)),
            },
            window,
            cx,
        );
    }

    fn submit_change_passcode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let old = self.read_field(&self.passcode_ui.old, cx);
        let new = self.read_field(&self.passcode_ui.new, cx);
        let confirm = self.read_field(&self.passcode_ui.confirm, cx);
        if old.is_empty() {
            self.passcode_ui.error = Some("Enter your current passcode".into());
            cx.notify();
            return;
        }
        if let Err(err) = passcode::validate_new(&new, &confirm, Some(&old)) {
            self.passcode_ui.error = Some(err.to_string());
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
                    this.status_note = "Local passcode changed".into();
                }
                Err(err) => this.passcode_ui.error = Some(error_text(&err)),
            },
            window,
            cx,
        );
    }

    fn submit_remove_passcode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let old = self.read_field(&self.passcode_ui.old, cx);
        if old.is_empty() {
            self.passcode_ui.error = Some("Enter your current passcode".into());
            cx.notify();
            return;
        }
        let Some(root) = self.passcode_root() else {
            return;
        };
        let demo = self.passcode_ui.demo;
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
                    this.passcode_ui.enabled = false;
                    this.passcode_ui.system_unlock = false;
                    this.passcode_ui.system_unlock_key = None;
                    this.goto_passcode_view(PasscodeView::Status, window, cx);
                    this.status_note = "Local passcode turned off".into();
                }
                Err(err) => this.passcode_ui.error = Some(error_text(&err)),
            },
            window,
            cx,
        );
    }

    pub(crate) fn set_autolock_secs(&mut self, secs: u32, cx: &mut Context<Self>) {
        self.passcode_ui.autolock_secs = secs;
        if !self.passcode_ui.demo
            && let Some(root) = self.passcode_root()
        {
            let _ = passcode::set_autolock(&root, secs);
        }
        cx.notify();
    }

    fn apply_custom_autolock(&mut self, cx: &mut Context<Self>) {
        let text = self.passcode_ui.custom_time.read(cx).value().to_string();
        match passcode::parse_hhmm(&text) {
            Some(secs) => {
                self.passcode_ui.error = None;
                self.set_autolock_secs(secs, cx);
            }
            None => {
                self.passcode_ui.error = Some("Enter a time as hours:minutes, like 0:30".into());
                cx.notify();
            }
        }
    }

    pub(crate) fn set_system_unlock(&mut self, on: bool, cx: &mut Context<Self>) {
        self.passcode_ui.system_unlock = on;
        if !on {
            self.passcode_ui.system_unlock_key = None;
        } else {
            // The master key is in memory while unlocked; keep it for the
            // next lock so the system prompt can unlock again.
            self.passcode_ui.system_unlock_key = global_unlock().key();
        }
        if !self.passcode_ui.demo
            && let Some(root) = self.passcode_root()
        {
            let _ = passcode::set_system_unlock(&root, on);
        }
        cx.notify();
    }

    /// Lock now (lock button, Cmd/Ctrl+L, tray, auto-lock).
    pub(crate) fn lock_by_passcode(&mut self, cx: &mut Context<Self>) {
        if !self.passcode_ui.enabled || self.passcode_ui.locked {
            return;
        }
        let state = global_unlock();
        if self.passcode_ui.system_unlock {
            self.passcode_ui.system_unlock_key = state.key();
        }
        // The in-memory master key goes away: unlocking needs the passcode
        // (or the system prompt that still holds it for this process).
        state.clear();
        self.passcode_ui.locked = true;
        self.passcode_ui.focus_lock_input = true;
        self.passcode_ui.suggest_system_unlock = self.passcode_ui.system_unlock_key.is_some();
        self.passcode_ui.lock_error = None;
        self.passcode_ui.logout_confirm = false;
        // tdesktop closes the media viewer and call panels when it locks.
        self.media_viewer.close();
        self.story_viewer.close();
        self.close_context_menus(cx);
        cx.notify();
    }

    fn unlock_done(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.passcode_ui.locked = false;
        self.passcode_ui.lock_error = None;
        self.passcode_ui.flood_until = None;
        self.passcode_ui.logout_confirm = false;
        self.passcode_ui
            .lock_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        if self.passcode_ui.system_unlock {
            self.passcode_ui.system_unlock_key = global_unlock().key();
        }
        super::presence::note_input();
        if std::mem::take(&mut self.passcode_ui.deferred_connect) {
            self.start_connection(cx);
        }
        cx.notify();
    }

    fn lock_failed(&mut self, message: String, cx: &mut Context<Self>) {
        self.passcode_ui.lock_error = Some(message);
        self.passcode_ui.shake_started = Some(Instant::now());
        self.passcode_ui.focus_lock_input = true;
        cx.notify();
    }

    /// Submit on the lock screen.
    pub(crate) fn submit_unlock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.passcode_ui.locked || self.passcode_ui.busy {
            return;
        }
        let typed = self.read_field(&self.passcode_ui.lock_input, cx);
        if typed.is_empty() {
            self.lock_failed("Enter your passcode".into(), cx);
            return;
        }
        if self.passcode_ui.demo {
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
            |this, result, window, cx| match result {
                Ok(()) => this.unlock_done(window, cx),
                Err(PasscodeError::Wrong) => {
                    this.passcode_ui
                        .lock_input
                        .update(cx, |input, cx| input.set_value("", window, cx));
                    this.lock_failed(PasscodeError::Wrong.to_string(), cx);
                }
                Err(err @ PasscodeError::Flood { retry_in_ms }) => {
                    this.passcode_ui.flood_until =
                        Some(Instant::now() + Duration::from_millis(retry_in_ms));
                    this.lock_failed(error_text(&err), cx);
                }
                Err(err) => this.lock_failed(err.to_string(), cx),
            },
            window,
            cx,
        );
    }

    /// Touch ID / system password on the lock screen.
    pub(crate) fn unlock_with_system(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(master) = self.passcode_ui.system_unlock_key.clone() else {
            self.lock_failed(
                "Enter your passcode once before using system unlock".into(),
                cx,
            );
            return;
        };
        if self.passcode_ui.busy {
            return;
        }
        self.passcode_ui.busy = true;
        let receiver = super::system_unlock::authenticate("unlock Quill");
        self.spawn_system_unlock_wait(receiver, master, window, cx);
    }

    fn spawn_system_unlock_wait(
        &mut self,
        receiver: std::sync::mpsc::Receiver<bool>,
        master: MasterKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.spawn_in(window, async move |this, cx| {
            let ok = loop {
                match receiver.try_recv() {
                    Ok(ok) => break ok,
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        cx.background_executor()
                            .timer(Duration::from_millis(100))
                            .await;
                    }
                    Err(_) => break false,
                }
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.passcode_ui.busy = false;
                if ok {
                    global_unlock().set(master);
                    this.unlock_done(window, cx);
                } else {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Once a second: auto-lock by idle time, and refresh the flood
    /// countdown. Notifies only when something changed.
    pub(crate) fn passcode_tick(&mut self, cx: &mut Context<Self>) {
        let ui = &mut self.passcode_ui;
        if ui.locked {
            if ui.flood_until.is_some_and(|t| Instant::now() >= t) {
                ui.flood_until = None;
                ui.lock_error = None;
                cx.notify();
            } else if ui.flood_until.is_some() {
                cx.notify();
            }
            return;
        }
        if !ui.enabled || ui.demo {
            return;
        }
        // OS idle where available ("away"), else input to this window.
        let idle = passcode::os_idle_ms().unwrap_or_else(super::presence::idle_ms);
        let effective = ui.idle.effective_idle(passcode::unix_ms_now(), idle);
        if passcode::autolock_due(effective, ui.autolock_secs) {
            self.lock_by_passcode(cx);
        }
    }

    pub(crate) fn spawn_passcode_tick(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if this.update(cx, |this, cx| this.passcode_tick(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    /// "Log out" on the lock screen: wipe the local data after confirming.
    pub(crate) fn confirm_lock_logout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.passcode_ui.demo
            && let Some(root) = self.passcode_root()
        {
            let accounts = account_keys(&root);
            // Close the client first so TDLib is not writing while the
            // database folders are removed.
            if let Some(mut live) = self.live.take() {
                live.shutdown(quill::connect::CLIENT_CLOSE_TIMEOUT);
            }
            let store = os_secret_store();
            for account in &accounts {
                let _ = store.delete(account);
            }
            passcode::wipe_local_data(&root, &accounts, &global_unlock());
        }
        self.passcode_ui.enabled = false;
        self.passcode_ui.system_unlock = false;
        self.passcode_ui.system_unlock_key = None;
        self.passcode_ui.logout_confirm = false;
        self.passcode_ui.deferred_connect = false;
        self.passcode_ui.locked = false;
        self.passcode_ui
            .lock_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        if !self.passcode_ui.demo {
            self.start_connection(cx);
        }
        cx.notify();
    }

    /// Tray "Lock Quill": lock, or show the passcode settings when none
    /// is set yet.
    pub fn lock_from_tray(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.passcode_ui.enabled {
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
            .tooltip("Lock Quill")
            .accessibility_label("Lock Quill")
            .on_click(cx.listener(|this, _, _, cx| this.lock_by_passcode(cx)))
            .into_any_element()
    }

    /// Per-frame work for the lock screen (focus, shake clock).
    pub(crate) fn passcode_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.passcode_ui.locked {
            if std::mem::take(&mut self.passcode_ui.focus_lock_input) {
                let input = self.passcode_ui.lock_input.clone();
                input.update(cx, |input, cx| input.focus(window, cx));
            }
            if std::mem::take(&mut self.passcode_ui.suggest_system_unlock) {
                self.unlock_with_system(window, cx);
            }
            if self.passcode_ui.shaking() {
                self.request_animation_tick(60, cx);
            }
        }
    }

    pub(crate) fn lock_overlay(&self, cx: &mut Context<Self>) -> AnyElement {
        let ui = &self.passcode_ui;
        let muted = cx.theme().muted_foreground;
        let waiting = ui.flood_until.is_some();
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
                    .aria_label("Enter your local passcode")
                    .text_xl()
                    .font_semibold()
                    .child("Enter your local passcode"),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("Unlock Quill to continue."),
            )
            .child(
                div().w_full().relative().left(px(shake)).child(
                    Input::new(&ui.lock_input)
                        .aria_label("Local passcode")
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
                    .child(ui.lock_error.clone().unwrap_or_default()),
            )
            .child(
                Button::new("lock-submit")
                    .label("Submit")
                    .primary()
                    .w_full()
                    .loading(ui.busy)
                    .disabled(ui.busy || waiting)
                    .on_click(cx.listener(|this, _, window, cx| this.submit_unlock(window, cx))),
            );
        if system {
            column = column.child(
                Button::new("lock-system-unlock")
                    .label(super::system_unlock::label())
                    .ghost()
                    .disabled(ui.busy)
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
                    .child(div().text_sm().child("Log out and delete local data?"))
                    .child(div().text_xs().text_color(muted).child(
                        "Without the passcode the saved data cannot be opened. Logging out \
                         removes it from this device, for every account, and turns the \
                         passcode off. Your chats stay in the Telegram cloud.",
                    ))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .justify_end()
                            .child(
                                Button::new("lock-logout-cancel")
                                    .label("Cancel")
                                    .ghost()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.passcode_ui.logout_confirm = false;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("lock-logout-confirm")
                                    .label("Log out")
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
                    .label("Log out")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.passcode_ui.logout_confirm = true;
                        cx.notify();
                    })),
            )
        };
        div()
            .id("passcode-lock")
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

    pub(crate) fn build_passcode_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::Passcode, |this, window, cx| {
                this.close_passcode(window, cx);
            });
        app.update(cx, |this, cx| {
            let body = this.passcode_body(cx);
            let title = match this.passcode_ui.view {
                PasscodeView::Status => "Local passcode",
                PasscodeView::Create => "Create local passcode",
                PasscodeView::Change => "Change passcode",
                PasscodeView::Remove => "Turn off passcode",
            };
            let footer = div().flex().justify_end().child(
                Button::new("close-passcode")
                    .label("Close")
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_passcode(window, cx);
                        this.close_kit_dialog_if_done(DialogKind::Passcode, window, cx);
                    })),
            );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(title))
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }

    fn passcode_field(
        &self,
        label: &'static str,
        input: &Entity<InputState>,
    ) -> impl IntoElement + use<> {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(div().text_sm().font_semibold().child(label))
            .child(
                Input::new(input)
                    .aria_label(label)
                    .content_type(InputContentType::Password)
                    .h(px(40.)),
            )
    }

    fn passcode_body(&mut self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let ui = &self.passcode_ui;
        let busy = ui.busy;
        let mut body = div().flex().flex_col().gap_3().w_full();
        if let Some(error) = ui.error.clone() {
            body = body.child(
                div()
                    .id("passcode-error")
                    .role(Role::Status)
                    .text_sm()
                    .text_color(danger())
                    .child(error),
            );
        }
        match ui.view {
            PasscodeView::Status => {
                body = body
                    .child(div().text_sm().text_color(muted).child(
                        "When a local passcode is set, a lock icon appears at the top of your \
                         chat list. Click it to lock Quill.",
                    ))
                    .child(div().text_sm().text_color(muted).child(
                        "Note: if you forget your passcode, you'll need to log out of Quill \
                         and log in again.",
                    ));
                if !ui.enabled {
                    body = body.child(
                        Button::new("passcode-turn-on")
                            .label("Turn on passcode")
                            .primary()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.goto_passcode_view(PasscodeView::Create, window, cx)
                            })),
                    );
                } else {
                    body = body.child(self.passcode_enabled_section(cx));
                }
            }
            PasscodeView::Create => {
                body = body
                    .child(self.passcode_field("Passcode", &ui.new))
                    .child(self.passcode_field("Confirm passcode", &ui.confirm))
                    .child(div().text_xs().text_color(muted).child(
                        "The passcode protects your saved data on this device. It is never \
                         stored or sent anywhere.",
                    ))
                    .child(self.passcode_form_buttons(cx, "Save passcode", false));
            }
            PasscodeView::Change => {
                body = body
                    .child(self.passcode_field("Current passcode", &ui.old))
                    .child(self.passcode_field("New passcode", &ui.new))
                    .child(self.passcode_field("Confirm new passcode", &ui.confirm))
                    .child(self.passcode_form_buttons(cx, "Save passcode", false));
            }
            PasscodeView::Remove => {
                body = body
                    .child(div().text_sm().text_color(muted).child(
                        "Quill will open without asking for a passcode. Your saved data goes \
                         back to being protected by this device's keychain only.",
                    ))
                    .child(self.passcode_field("Current passcode", &ui.old))
                    .child(self.passcode_form_buttons(cx, "Turn off passcode", true));
            }
        }
        let _ = busy;
        body
    }

    fn passcode_form_buttons(
        &self,
        cx: &mut Context<Self>,
        label: &'static str,
        danger_action: bool,
    ) -> Div {
        let busy = self.passcode_ui.busy;
        let mut submit = Button::new("passcode-submit")
            .label(if busy { "Working…" } else { label })
            .loading(busy)
            .disabled(busy)
            .on_click(cx.listener(|this, _, window, cx| this.submit_passcode_form(window, cx)));
        submit = if danger_action {
            submit.danger()
        } else {
            submit.primary()
        };
        div()
            .flex()
            .gap_2()
            .justify_end()
            .child(
                Button::new("passcode-cancel")
                    .label("Cancel")
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.goto_passcode_view(PasscodeView::Status, window, cx)
                    })),
            )
            .child(submit)
    }

    fn passcode_enabled_section(&mut self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let secs = self.passcode_ui.autolock_secs;
        let mut section =
            div().flex().flex_col().gap_3().child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new("passcode-change")
                            .label("Change passcode")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.goto_passcode_view(PasscodeView::Change, window, cx)
                            })),
                    )
                    .child(Button::new("passcode-lock-now").label("Lock now").on_click(
                        cx.listener(|this, _, window, cx| {
                            this.close_passcode(window, cx);
                            this.close_kit_dialog_if_done(DialogKind::Passcode, window, cx);
                            this.lock_by_passcode(cx);
                        }),
                    ))
                    .child(
                        Button::new("passcode-remove")
                            .label("Turn off passcode")
                            .ghost()
                            .custom(super::security::quiet_danger(cx))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.goto_passcode_view(PasscodeView::Remove, window, cx)
                            })),
                    ),
            );
        let mut presets = div().flex().flex_wrap().gap_1();
        for preset in AUTOLOCK_PRESETS {
            presets = presets.child(
                Button::new(SharedString::from(format!("autolock-{preset}")))
                    .label(passcode::autolock_label(preset))
                    .outline()
                    .selected(preset == secs)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.passcode_ui.error = None;
                        this.set_autolock_secs(preset, cx)
                    })),
            );
        }
        let custom = !AUTOLOCK_PRESETS.contains(&secs);
        section = section.child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_sm().font_semibold().child(format!(
                    "{} {}",
                    PasscodeUi::autolock_title(),
                    passcode::autolock_label(secs)
                )))
                .child(presets)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div().w(px(96.)).child(
                                Input::new(&self.passcode_ui.custom_time)
                                    .aria_label("Custom auto-lock time, hours and minutes")
                                    .h(px(32.)),
                            ),
                        )
                        .child(
                            Button::new("autolock-custom")
                                .label("Set custom time")
                                .outline()
                                .selected(custom)
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.apply_custom_autolock(cx)),
                                ),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child("Custom time is hours:minutes, like 0:30."),
                ),
        );
        if self.system_unlock_offered() {
            section = section.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_sm()
                                    .font_semibold()
                                    .child(super::system_unlock::label()),
                            )
                            .child(div().text_xs().text_color(muted).child(
                                "Enter your passcode once after each launch, then unlock with \
                                 the system prompt.",
                            )),
                    )
                    .child(
                        Switch::new("passcode-system-unlock")
                            .checked(self.passcode_ui.system_unlock)
                            .accessibility_label(super::system_unlock::label())
                            .on_click(cx.listener(|this, &on, _, cx| {
                                this.set_system_unlock(on, cx);
                            })),
                    ),
            );
        }
        section
    }
}
