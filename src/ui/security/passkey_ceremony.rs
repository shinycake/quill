//! The passkey ceremony dialog: creating a passkey on a USB security key,
//! or signing in with one from the QR screen (`passkey_login`) (tdesktop `webauthn/webauthn_common.cpp` `ShowTouchBox` / `ShowPinBox`,
//! and the error sheet of `webauthn/cable_box.cpp`). One kit dialog with
//! three sheets:
//!
//! - Key: "Touch your security key", "Insert your security key, then
//!   touch it to continue.", Cancel.
//! - PIN: "Security key PIN", the about line (or "Incorrect PIN. Please
//!   try again." after a wrong one), a masked field, Confirm and Cancel.
//! - Error: "Something went wrong", the reason, Close.
//!
//! The ceremony itself runs on a worker thread (`quill::webauthn`); a
//! ticker started by "Create Passkey" or "Log in using passkey" moves the
//! options from the server to the worker, the worker's events to the
//! sheets, and the result to `addLoginPasskey` or
//! `checkAuthenticationPasskey`.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::shell::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use gpui_kit::TestSupportExt;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{Key, keys, tr};
use quill::webauthn::{self, Ceremony, CeremonyError, CeremonyEvent, ParentWindow};
use zeroize::Zeroizing;

/// Rest until the server's answer is applied (`APPLIED`: the registration
/// options, the added passkey) or the worker has an event (`PASSKEY`: every
/// way a worker ends pokes it, a panic and a failed spawn included). A lock
/// and an account switch end the ceremony directly. `seen` holds the epochs
/// taken before the last pass.
async fn rest_until_passkey_news(seen: (u64, u64)) {
    use std::future::Future;
    use std::task::Poll;
    let mut applied = std::pin::pin!(quill::wake::APPLIED.wait_since(seen.0));
    let mut worker = std::pin::pin!(quill::wake::PASSKEY.wait_since(seen.1));
    std::future::poll_fn(|cx| {
        let applied = applied.as_mut().poll(cx).is_ready();
        let worker = worker.as_mut().poll(cx).is_ready();
        if applied || worker {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    })
    .await;
}

/// Which sheet the ceremony dialog shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum CeremonySheet {
    /// Waiting for the key (tdesktop's touch box).
    Key,
    /// Asking for the key's PIN.
    Pin { invalid: bool },
    /// The ceremony failed; the text says why.
    Error(Key),
    /// Windows: the system's Windows Security dialog runs the ceremony;
    /// Quill shows a spinner row and Cancel until it returns.
    System,
    /// The QR code and "Use a security key"; `bluetooth: false` says
    /// Bluetooth is unavailable instead of the code.
    Choose { bluetooth: bool },
    /// The request is on the phone.
    Continue,
}

/// What the dialog shows now: a sheet, or one of the two waits on the
/// worker. A derived view of [`CeremonyRun`], which holds the worker in
/// the waiting stage itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum CeremonyView {
    /// A phone ceremony probes Bluetooth before anything shows (tdesktop
    /// waits up to 1.5 s for the scanner before `ShowCableBox`).
    Probing,
    /// The phone was seen; the tunnel opens.
    Connecting,
    Sheet(CeremonySheet),
}

impl From<CeremonySheet> for CeremonyView {
    fn from(sheet: CeremonySheet) -> Self {
        Self::Sheet(sheet)
    }
}

/// What the ceremony is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum CeremonyMode {
    /// "Create Passkey" in Settings.
    Register,
    /// "Log in using passkey" on the QR sign-in screen.
    Login,
}

impl CeremonyRun {
    /// A running dialog on `view`, holding `worker`.
    fn at(view: CeremonyView, worker: Option<Ceremony>, qr: Option<hybrid::CableQr>) -> Self {
        match view {
            CeremonyView::Probing => Self::Probing { worker },
            CeremonyView::Connecting => Self::Connecting { worker },
            CeremonyView::Sheet(sheet) => Self::Live { sheet, worker, qr },
        }
    }
}

/// The open ceremony dialog: running, with what belongs to the run, or
/// ended with its reason. The worker and the phone sheet's QR exist only
/// while it runs, so neither outlives it.
enum CeremonyRun {
    Live {
        sheet: CeremonySheet,
        /// The running ceremony; `None` in demos and tests.
        worker: Option<Ceremony>,
        /// The phone sheet's QR code, rendered once.
        qr: Option<hybrid::CableQr>,
    },
    /// Nothing shows while the worker probes Bluetooth; its next event
    /// moves the dialog on, or ends it.
    Probing { worker: Option<Ceremony> },
    /// The phone was seen; the worker opens the tunnel.
    Connecting { worker: Option<Ceremony> },
    /// The ceremony failed; the sheet says why.
    Ended(Key),
}

pub(in crate::ui) struct CeremonyUi {
    pub(in crate::ui) mode: CeremonyMode,
    run: CeremonyRun,
}

impl std::fmt::Debug for CeremonyUi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CeremonyUi")
            .field("mode", &self.mode)
            .field("sheet", &self.sheet())
            .field("running", &self.worker().is_some())
            .finish()
    }
}

/// What the user is told for a failed registration (tdesktop
/// `AttemptRegister`).
fn register_error(error: &CeremonyError) -> Key {
    match error {
        CeremonyError::NoDevice => keys::PASSKEY_ERROR_NO_DEVICE,
        CeremonyError::AccessDenied => keys::Q_PASSKEY_ERROR_ACCESS_DENIED,
        CeremonyError::PinBlocked => keys::PASSKEY_PIN_BLOCKED,
        CeremonyError::Phone => keys::PASSKEY_CABLE_ERROR,
        CeremonyError::Cancelled | CeremonyError::NoCredentials | CeremonyError::Other(_) => {
            keys::PASSKEY_ERROR_REGISTER
        }
    }
}

/// What the user is told for a failed sign-in (tdesktop `AttemptLogin`).
fn login_error(error: &CeremonyError) -> Key {
    match error {
        CeremonyError::NoDevice => keys::PASSKEY_ERROR_NO_DEVICE,
        CeremonyError::AccessDenied => keys::Q_PASSKEY_ERROR_ACCESS_DENIED,
        CeremonyError::NoCredentials => keys::PASSKEY_ERROR_NO_CREDENTIALS,
        CeremonyError::PinBlocked => keys::PASSKEY_PIN_BLOCKED,
        CeremonyError::Phone => keys::PASSKEY_CABLE_ERROR,
        CeremonyError::Cancelled | CeremonyError::Other(_) => keys::PASSKEY_ERROR_LOGIN,
    }
}

/// The sheet a new ceremony opens on: Windows Security's own dialog, the
/// Bluetooth probe of a phone ceremony, or the security key sheet.
fn first_sheet() -> CeremonyView {
    if webauthn::system_dialog() {
        CeremonyView::Sheet(CeremonySheet::System)
    } else if webauthn::starts_with_phone() {
        CeremonyView::Probing
    } else {
        CeremonyView::Sheet(CeremonySheet::Key)
    }
}

/// The window the Windows Security dialog opens over (tdesktop
/// `NativeParentWindow`: the active window, here the one the ceremony
/// dialog is in). Nothing elsewhere.
fn parent_window(window: &Window) -> ParentWindow {
    #[cfg(windows)]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if let Ok(handle) = HasWindowHandle::window_handle(window)
            && let RawWindowHandle::Win32(handle) = handle.as_raw()
        {
            return ParentWindow(handle.hwnd.get());
        }
    }
    let _ = window;
    ParentWindow::default()
}

impl CeremonyUi {
    /// A dialog without a worker (demos, tests) or before it starts.
    pub(in crate::ui) fn new(
        mode: CeremonyMode,
        view: impl Into<CeremonyView>,
        ceremony: Option<Ceremony>,
    ) -> Self {
        Self {
            mode,
            run: CeremonyRun::at(view.into(), ceremony, None),
        }
    }

    /// What the dialog shows.
    pub(in crate::ui) fn sheet(&self) -> CeremonyView {
        match &self.run {
            CeremonyRun::Live { sheet, .. } => CeremonyView::Sheet(*sheet),
            CeremonyRun::Probing { .. } => CeremonyView::Probing,
            CeremonyRun::Connecting { .. } => CeremonyView::Connecting,
            CeremonyRun::Ended(reason) => CeremonyView::Sheet(CeremonySheet::Error(*reason)),
        }
    }

    /// Moves a running dialog to `view`, the worker with it; the QR belongs
    /// to the sheet it was made for and goes. An ended one stays ended.
    fn show(&mut self, view: impl Into<CeremonyView>) {
        if let Some(worker) = self.worker_slot().map(Option::take) {
            self.run = CeremonyRun::at(view.into(), worker, None);
        }
    }

    /// The PIN sheet opens on a running ceremony.
    fn ask_pin(&mut self, invalid: bool) {
        self.show(CeremonySheet::Pin { invalid });
    }

    /// The phone sheet opens with its QR (`None`: Bluetooth is off).
    fn show_choose(&mut self, code: Option<hybrid::CableQr>) {
        if let Some(worker) = self.worker_slot().map(Option::take) {
            let sheet = CeremonySheet::Choose {
                bluetooth: code.is_some(),
            };
            self.run = CeremonyRun::at(CeremonyView::Sheet(sheet), worker, code);
        }
    }

    /// The worker's slot in whichever running stage holds it.
    fn worker_slot(&mut self) -> Option<&mut Option<Ceremony>> {
        match &mut self.run {
            CeremonyRun::Live { worker, .. }
            | CeremonyRun::Probing { worker }
            | CeremonyRun::Connecting { worker } => Some(worker),
            CeremonyRun::Ended(_) => None,
        }
    }

    /// The ceremony failed: the worker and the QR go with the run.
    fn end(&mut self, reason: Key) {
        self.run = CeremonyRun::Ended(reason);
    }

    fn qr(&self) -> Option<&hybrid::CableQr> {
        match &self.run {
            CeremonyRun::Live { qr, .. } => qr.as_ref(),
            CeremonyRun::Probing { .. }
            | CeremonyRun::Connecting { .. }
            | CeremonyRun::Ended(_) => None,
        }
    }

    fn worker_mut(&mut self) -> Option<&mut Ceremony> {
        self.worker_slot().and_then(Option::as_mut)
    }

    pub(in crate::ui) fn worker(&self) -> Option<&Ceremony> {
        match &self.run {
            CeremonyRun::Live { worker, .. }
            | CeremonyRun::Probing { worker }
            | CeremonyRun::Connecting { worker } => worker.as_ref(),
            CeremonyRun::Ended(_) => None,
        }
    }

    /// Closing: the worker, if one runs, to be cancelled.
    fn into_worker(self) -> Option<Ceremony> {
        match self.run {
            CeremonyRun::Live { worker, .. }
            | CeremonyRun::Probing { worker }
            | CeremonyRun::Connecting { worker } => worker,
            CeremonyRun::Ended(_) => None,
        }
    }
}

impl QuillApp {
    /// The PIN field, made once with the window it lives in.
    pub(in crate::ui) fn passkey_pin_input(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        if let Some(input) = &self.privacy.passkeys.pin_input {
            return input.clone();
        }
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(tr(keys::PASSKEY_PIN_PLACEHOLDER))
                .masked(true)
        });
        cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.submit_passkey_pin(window, cx);
            }
        })
        .detach();
        self.privacy.passkeys.pin_input = Some(input.clone());
        input
    }

    /// "Create Passkey" in the "Protect your account" box (tdesktop
    /// `initRegistration` then `RegisterKey`): the options from the server,
    /// then the ceremony.
    pub(in crate::ui) fn start_passkey_creation(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.passkey_pin_input(window, cx);
        let Some(live) = self.live.as_mut() else {
            return;
        };
        if live.driver.begin_register().is_err() {
            return;
        }
        self.open_box(DialogKind::PasskeyCeremony);
        self.run_passkey_ticker(window, cx);
        cx.notify();
    }

    /// Whether "Create Passkey" is busy (options or the result on their
    /// way, or the ceremony open).
    pub(in crate::ui) fn passkey_creation_busy(&self) -> bool {
        self.privacy.passkeys.ceremony().is_some()
            || self
                .live
                .as_ref()
                .is_some_and(|live| live.driver.session.settings.passkeys.add_busy())
    }

    pub(in crate::ui) fn run_passkey_ticker(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.privacy.passkeys.ticking() || self.refused_while_locked() {
            return;
        }
        let ticker = self.spawn_screen_in(window, cx, async move |this, cx| {
            loop {
                // Taken before the pass, so news that lands during it ends
                // the next rest at once.
                let seen = (quill::wake::APPLIED.epoch(), quill::wake::PASSKEY.epoch());
                let keep = this
                    .update_in(cx, |this, window, cx| {
                        this.tick_passkey_ceremony(window, cx)
                    })
                    .unwrap_or(false);
                if !keep {
                    break;
                }
                rest_until_passkey_news(seen).await;
            }
            let _ = this.update(cx, |this, _| this.privacy.passkeys.stop_ticker());
        });
        self.privacy.passkeys.set_ticker(ticker);
    }

    /// One tick: start the ceremony once the options are there, follow
    /// the worker, send its result, close the box once the passkey is
    /// added. `false` when there is nothing left to follow.
    pub(in crate::ui) fn tick_passkey_ceremony(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        // Locked: no ceremony starts or goes on. Options that land after
        // the lock are dropped as the lock closed the box they were for
        // (the registration's options stay for the next Create), and the
        // ticker ends; tdesktop's lock closes its boxes the same way.
        if self.refused_while_locked() {
            self.close_passkey_ceremony_for_lock();
            return false;
        }
        let mut changed = false;
        let (start, start_login, added) = match self.live.as_mut() {
            Some(live) => {
                let state = &mut live.driver.session.settings.passkeys;
                (
                    state.take_starting(),
                    state.login.take_ceremony(),
                    state.take_added(),
                )
            }
            None => (None, None, false),
        };
        if let Some(options) = start {
            // box-host: opened by an answer; start_passkey_creation named the window
            self.privacy.passkeys.show_ceremony(CeremonyUi::new(
                CeremonyMode::Register,
                first_sheet(),
                Some(webauthn::start_register(options, parent_window(window))),
            ));
            changed = true;
        }
        if let Some(options) = start_login {
            // box-host: opened by an answer; start_passkey_login named the window
            self.privacy.passkeys.show_ceremony(CeremonyUi::new(
                CeremonyMode::Login,
                first_sheet(),
                Some(webauthn::start_login(options, parent_window(window))),
            ));
            changed = true;
        }
        changed |= self.close_login_ceremony_if_signed_in();
        let events = self
            .privacy
            .passkeys
            .ceremony_mut()
            .and_then(|ui| ui.worker_mut())
            .map(Ceremony::poll)
            .unwrap_or_default();
        for event in events {
            changed = true;
            let asks_pin = matches!(event, CeremonyEvent::NeedPin { .. });
            self.apply_ceremony_event(event);
            if asks_pin {
                self.focus_passkey_pin(window, cx);
            }
        }
        if added {
            // tdesktop closes the box once `registerPasskey` is done.
            self.privacy.passkeys.close_about();
            changed = true;
        }
        if changed {
            cx.notify();
        }
        self.passkey_creation_busy() || self.passkey_login_busy()
    }

    /// The PIN sheet just opened: the field takes focus.
    fn focus_passkey_pin(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self
            .privacy
            .passkeys
            .ceremony()
            .is_some_and(|ui| matches!(ui.sheet(), CeremonyView::Sheet(CeremonySheet::Pin { .. })))
        {
            return;
        }
        if let Some(input) = self.privacy.passkeys.pin_input.clone() {
            input.update(cx, |input, cx| input.focus(window, cx));
        }
    }

    fn apply_ceremony_event(&mut self, event: CeremonyEvent) {
        match event {
            CeremonyEvent::NeedPin { invalid } => {
                if let Some(ui) = self.privacy.passkeys.ceremony_mut() {
                    ui.ask_pin(invalid);
                }
            }
            CeremonyEvent::Hybrid { qr } => {
                if let Some(ui) = self.privacy.passkeys.ceremony_mut() {
                    ui.show_choose(qr.and_then(|qr| hybrid::render_cable_qr(&qr.0)));
                }
            }
            CeremonyEvent::PhoneConnecting => self.set_ceremony_view(CeremonyView::Connecting),
            CeremonyEvent::PhoneContinue => self.set_ceremony_view(CeremonySheet::Continue.into()),
            // tdesktop closes the phone box and shows the touch box.
            CeremonyEvent::UseKey => self.set_ceremony_view(CeremonySheet::Key.into()),
            CeremonyEvent::NoKey => {
                // tdesktop `ShowCableToast`; the QR stays.
                self.set_note(StatusNote::silent(tr(keys::PASSKEY_ERROR_NO_DEVICE)));
            }
            CeremonyEvent::Registered(registration) => {
                // The dialog closes; the server gets the result.
                let ui = self.privacy.passkeys.take_ceremony();
                if ui.is_some_and(|ui| ui.mode == CeremonyMode::Register)
                    && let Some(live) = self.live.as_mut()
                    && live.driver.finish_register(&registration).is_err()
                {
                    // The key was touched for nothing: say so.
                    self.set_note(StatusNote::error(tr(keys::PASSKEY_ERROR_REGISTER)));
                }
            }
            CeremonyEvent::SignedIn(assertion) => {
                // tdesktop closes the touch box and calls
                // `FinishPasskeyLogin`; the auth state takes it from there.
                let ui = self.privacy.passkeys.take_ceremony();
                if ui.is_some_and(|ui| ui.mode == CeremonyMode::Login)
                    && let Some(live) = self.live.as_mut()
                    && live.driver.finish_login(&assertion).is_err()
                {
                    self.set_note(StatusNote::error(tr(keys::PASSKEY_ERROR_LOGIN)));
                }
            }
            CeremonyEvent::Failed(CeremonyError::Cancelled) => {
                self.privacy.passkeys.take_ceremony();
            }
            CeremonyEvent::Failed(error)
                if self.privacy.passkeys.ceremony().is_some_and(|ui| {
                    matches!(
                        ui.sheet(),
                        CeremonyView::Sheet(CeremonySheet::System) | CeremonyView::Probing
                    )
                }) =>
            {
                // tdesktop `ShowNativeError`: no error sheet over the
                // system dialog, a toast with the generic reason; a phone
                // ceremony that fails before its box shows toasts its
                // reason (`cable_ceremony.cpp` `finish`).
                let Some(ui) = self.privacy.passkeys.take_ceremony() else {
                    return;
                };
                quill::webauthn::log(&format!("{:?} failed: {error:?}", ui.mode));
                self.set_note(StatusNote::error(tr(match (ui.sheet(), ui.mode) {
                    (CeremonyView::Probing, CeremonyMode::Register) => register_error(&error),
                    (CeremonyView::Probing, CeremonyMode::Login) => login_error(&error),
                    (_, CeremonyMode::Register) => keys::PASSKEY_ERROR_REGISTER,
                    (_, CeremonyMode::Login) => keys::PASSKEY_ERROR_LOGIN,
                })));
            }
            CeremonyEvent::Failed(error) => {
                if let Some(ui) = self.privacy.passkeys.ceremony_mut() {
                    let reason = match ui.mode {
                        CeremonyMode::Register => register_error(&error),
                        CeremonyMode::Login => login_error(&error),
                    };
                    quill::webauthn::log(&format!("{:?} failed: {error:?}", ui.mode));
                    ui.end(reason);
                }
            }
        }
    }

    fn set_ceremony_view(&mut self, view: CeremonyView) {
        if let Some(ui) = self.privacy.passkeys.ceremony_mut() {
            ui.show(view);
        }
    }

    /// Confirm on the PIN sheet: an empty PIN keeps the field focused
    /// (tdesktop `showError`), any other goes to the worker; the field is
    /// cleared either way so the PIN does not stay in the UI.
    pub(in crate::ui) fn submit_passkey_pin(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(input) = self.privacy.passkeys.pin_input.clone() else {
            return;
        };
        let Some(ui) = self.privacy.passkeys.ceremony_mut() else {
            return;
        };
        if !matches!(ui.sheet(), CeremonyView::Sheet(CeremonySheet::Pin { .. })) {
            return;
        }
        let pin = Zeroizing::new(input.read(cx).value().to_string());
        if pin.is_empty() {
            input.update(cx, |input, cx| input.focus(window, cx));
            return;
        }
        input.update(cx, |input, cx| input.set_value("", window, cx));
        if let Some(ceremony) = ui.worker() {
            ceremony.submit_pin(pin);
        }
        ui.show(CeremonySheet::Key);
        cx.notify();
    }

    /// Cancel, Close, Esc: the key gets CTAPHID_CANCEL and the dialog
    /// closes (tdesktop: the box closing cancels the ceremony).
    pub(in crate::ui) fn close_passkey_ceremony(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(ui) = self.privacy.passkeys.take_ceremony()
            && let Some(ceremony) = ui.into_worker()
        {
            ceremony.cancel();
        }
        if let Some(live) = self.live.as_mut() {
            live.driver.cancel_register();
        }
        if let Some(input) = self.privacy.passkeys.pin_input.clone() {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        cx.notify();
    }

    /// The lock closes the ceremony box, as tdesktop's lock closes its
    /// boxes: the key gets CTAPHID_CANCEL and a PIN sheet on its way never
    /// takes focus from the passcode field. The PIN field goes with any
    /// digits in it; the next ceremony makes a new one.
    /// The ticker stops with it, and options already queued for a
    /// ceremony are dropped, so none starts under the lock.
    pub(in crate::ui) fn close_passkey_ceremony_for_lock(&mut self) {
        if let Some(ui) = self.privacy.passkeys.take_ceremony()
            && let Some(ceremony) = ui.into_worker()
        {
            ceremony.cancel();
        }
        self.privacy.passkeys.pin_input = None;
        self.privacy.passkeys.stop_ticker();
        if let Some(live) = self.live.as_mut() {
            let state = &mut live.driver.session.settings.passkeys;
            state.cancel_register();
            state.login.leave_sign_in();
        }
    }

    fn build_passkey_ceremony_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close = QuillShell::on_close_kind(
            app,
            shell,
            DialogKind::PasskeyCeremony,
            |this, window, cx| {
                this.close_passkey_ceremony(window, cx);
            },
        );
        app.update(cx, |this, cx| {
            let (mode, view) = this.privacy.passkeys.ceremony().map_or(
                (
                    CeremonyMode::Register,
                    CeremonyView::Sheet(CeremonySheet::Key),
                ),
                |ui| (ui.mode, ui.sheet()),
            );
            let pin_input = this.privacy.passkeys.pin_input.clone();
            let bottom = hybrid::bottom_button(view, cx);
            if let Some((title, body)) = this.cable_sheet(mode, view, cx) {
                let footer = div().flex().w_full().children(bottom);
                return Self::ceremony_dialog(dialog, title, body, footer, on_close);
            }
            // The two waits never reach here: Probing shows no dialog and
            // Connecting is a phone sheet.
            let sheet = match view {
                CeremonyView::Sheet(sheet) => sheet,
                CeremonyView::Probing | CeremonyView::Connecting => CeremonySheet::System,
            };
            let (title, about) = match sheet {
                CeremonySheet::Key => (keys::PASSKEY_TOUCH, keys::PASSKEY_INSERT),
                CeremonySheet::Pin { invalid: false } => {
                    (keys::PASSKEY_PIN_TITLE, keys::PASSKEY_PIN_ABOUT)
                }
                CeremonySheet::Pin { invalid: true } => {
                    (keys::PASSKEY_PIN_TITLE, keys::PASSKEY_PIN_INVALID)
                }
                CeremonySheet::Error(reason) => (keys::PASSKEY_CABLE_ERROR_TITLE, reason),
                // The phone sheets were built above.
                CeremonySheet::Choose { .. } | CeremonySheet::Continue | CeremonySheet::System => (
                    match mode {
                        CeremonyMode::Register => keys::PASSKEY_CABLE_REGISTER_TITLE,
                        CeremonyMode::Login => keys::PASSKEY_CABLE_LOGIN_TITLE,
                    },
                    keys::Q_PASSKEY_WINDOWS_WAITING,
                ),
            };
            let mut body = div().flex().flex_col().gap_3();
            body = if sheet == CeremonySheet::System {
                // One row: the spinner and what Quill waits for.
                body.child(
                    div()
                        .id("passkey-ceremony-about")
                        .test_support()
                        .role(Role::Status)
                        .aria_label(tr(about))
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_sm()
                        .text_color(cx.theme().foreground)
                        .child(Spinner::new().small())
                        .child(tr(about)),
                )
            } else {
                body.child(
                    div()
                        .id("passkey-ceremony-about")
                        .test_support()
                        .role(Role::Status)
                        .aria_label(tr(about))
                        .text_sm()
                        .text_color(if sheet == (CeremonySheet::Pin { invalid: true }) {
                            crate::ui::chat_theme::danger().into()
                        } else {
                            cx.theme().foreground
                        })
                        .child(tr(about)),
                )
            };
            let is_pin = matches!(sheet, CeremonySheet::Pin { .. });
            if is_pin && let Some(input) = &pin_input {
                body = body.child(
                    Input::new(input)
                        .id("passkey-pin")
                        .aria_label(tr(keys::PASSKEY_PIN_PLACEHOLDER)),
                );
            }
            if bottom.is_some() {
                let body = body.into_any_element();
                let footer = div().flex().w_full().children(bottom);
                return Self::ceremony_dialog(dialog, title, body, footer, on_close);
            }
            let mut footer = div().flex().justify_end().gap_2();
            {
                {
                    footer = footer.child(
                        Button::new("passkey-ceremony-cancel")
                            .label(tr(keys::CANCEL))
                            .ghost()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_passkey_ceremony(window, cx);
                                this.close_kit_dialog_if_done(
                                    DialogKind::PasskeyCeremony,
                                    window,
                                    cx,
                                );
                            })),
                    );
                    if is_pin {
                        footer = footer.child(
                            Button::new("passkey-pin-confirm")
                                .label(tr(keys::PASSKEY_CONFIRM))
                                .primary()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.submit_passkey_pin(window, cx);
                                })),
                        );
                    }
                }
            }
            Self::ceremony_dialog(dialog, title, body.into_any_element(), footer, on_close)
        })
    }

    fn ceremony_dialog(
        dialog: Dialog,
        title: Key,
        body: AnyElement,
        footer: impl IntoElement,
        on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Dialog {
        dialog
            .title(crate::ui::shell::dialog_title(tr(title)))
            .width(px(360.))
            .content(crate::ui::shell::scrollable_dialog_content(
                super::passkeys::once(body),
            ))
            .footer(footer)
            .on_close(on_close)
    }
}

crate::ui::shell::register_dialogs! {
    /// The ceremony over the "Protect your account" box.
    PasskeyCeremony => DialogSpec::new(
        605,
        |app| app
            .privacy
            .passkeys
            .ceremony()
            .is_some_and(|ui| ui.sheet() != CeremonyView::Probing),
        QuillApp::build_passkey_ceremony_dialog,
    )
    .close(|app, window, cx| app.close_passkey_ceremony(window, cx)),
}

crate::ui::screenshot_demo::register_demos![
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-passkey-ceremony")
        .setup(QuillApp::demo_passkey_ceremony),
];

impl QuillApp {
    /// `ready-passkey-ceremony`: the ceremony dialog over the box, with
    /// `QUILL_DEMO_PASSKEY_CEREMONY` set to `key` (default), `pin`,
    /// `pin-invalid`, `error`, `windows` (the Windows Security waiting
    /// row), or the phone sheets `choose`, `bluetooth-off`, `connecting`,
    /// `continue` and `phone-error`. No worker runs.
    fn demo_passkey_ceremony(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.seed_demo_passkeys();
        let _ = self.passkey_pin_input(window, cx);
        self.open_passkeys(cx);
        self.open_passkeys_about(cx);
        let view: CeremonyView = match std::env::var("QUILL_DEMO_PASSKEY_CEREMONY")
            .unwrap_or_default()
            .as_str()
        {
            "pin" => CeremonySheet::Pin { invalid: false }.into(),
            "pin-invalid" => CeremonySheet::Pin { invalid: true }.into(),
            "error" => CeremonySheet::Error(keys::PASSKEY_ERROR_NO_DEVICE).into(),
            "windows" => CeremonySheet::System.into(),
            "choose" => CeremonySheet::Choose { bluetooth: true }.into(),
            "bluetooth-off" => CeremonySheet::Choose { bluetooth: false }.into(),
            "connecting" => CeremonyView::Connecting,
            "continue" => CeremonySheet::Continue.into(),
            "phone-error" => CeremonySheet::Error(keys::PASSKEY_CABLE_ERROR).into(),
            _ => CeremonySheet::Key.into(),
        };
        let mut ui = CeremonyUi::new(CeremonyMode::Register, view, None);
        if view == CeremonyView::Sheet(CeremonySheet::Choose { bluetooth: true }) {
            ui.show_choose(hybrid::render_cable_qr(&hybrid::demo_qr()));
        }
        // box-host: demo
        self.privacy.passkeys.show_ceremony(ui);
        cx.notify();
    }

    /// Tests: show a sheet without a worker.
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(in crate::ui) fn show_passkey_sheet(
        &mut self,
        sheet: impl Into<CeremonyView>,
        cx: &mut Context<Self>,
    ) {
        let mode = self
            .privacy
            .passkeys
            .ceremony()
            .map_or(CeremonyMode::Register, |ui| ui.mode);
        // box-host: a step of the open ceremony; it stays where it shows
        self.privacy
            .passkeys
            .show_ceremony(CeremonyUi::new(mode, sheet.into(), None));
        cx.notify();
    }

    /// Tests: feed a worker event as if the ceremony sent it.
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(in crate::ui) fn feed_ceremony_event(
        &mut self,
        event: CeremonyEvent,
        cx: &mut Context<Self>,
    ) {
        self.apply_ceremony_event(event);
        cx.notify();
    }
}

#[path = "passkey_hybrid.rs"]
mod hybrid;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "passkey_ceremony_tests.rs"]
mod tests;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "passkey_hybrid_tests.rs"]
mod hybrid_tests;
