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
use std::time::Duration;
use zeroize::Zeroizing;

/// How often the ticker looks at the server's answers and the worker.
const TICK: Duration = Duration::from_millis(100);

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
    /// A phone ceremony probes Bluetooth before anything shows (tdesktop
    /// waits up to 1.5 s for the scanner before `ShowCableBox`).
    Probing,
    /// The QR code and "Use a security key"; `bluetooth: false` says
    /// Bluetooth is unavailable instead of the code.
    Choose { bluetooth: bool },
    /// The phone was seen; the tunnel opens.
    Connecting,
    /// The request is on the phone.
    Continue,
}

/// What the ceremony is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum CeremonyMode {
    /// "Create Passkey" in Settings.
    Register,
    /// "Log in using passkey" on the QR sign-in screen.
    Login,
}

/// The open ceremony dialog.
pub(in crate::ui) struct CeremonyUi {
    pub(in crate::ui) mode: CeremonyMode,
    pub(in crate::ui) sheet: CeremonySheet,
    /// The running ceremony; `None` once it ended (and in demos).
    ceremony: Option<Ceremony>,
    /// Focus the PIN field on the next render.
    focus_pin: bool,
    /// The phone sheet's QR code, rendered once.
    qr: Option<hybrid::CableQr>,
}

impl std::fmt::Debug for CeremonyUi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CeremonyUi")
            .field("mode", &self.mode)
            .field("sheet", &self.sheet)
            .field("running", &self.ceremony.is_some())
            .finish()
    }
}

/// What the user is told for a failed registration (tdesktop
/// `AttemptRegister`).
fn register_error(error: &CeremonyError) -> Key {
    match error {
        CeremonyError::NoDevice => keys::PASSKEY_ERROR_NO_DEVICE,
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
        CeremonyError::NoCredentials => keys::PASSKEY_ERROR_NO_CREDENTIALS,
        CeremonyError::PinBlocked => keys::PASSKEY_PIN_BLOCKED,
        CeremonyError::Phone => keys::PASSKEY_CABLE_ERROR,
        CeremonyError::Cancelled | CeremonyError::Other(_) => keys::PASSKEY_ERROR_LOGIN,
    }
}

/// The sheet a new ceremony opens on: Windows Security's own dialog, the
/// Bluetooth probe of a phone ceremony, or the security key sheet.
fn first_sheet() -> CeremonySheet {
    if webauthn::system_dialog() {
        CeremonySheet::System
    } else if webauthn::starts_with_phone() {
        CeremonySheet::Probing
    } else {
        CeremonySheet::Key
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
        sheet: CeremonySheet,
        ceremony: Option<Ceremony>,
    ) -> Self {
        Self {
            mode,
            sheet,
            ceremony,
            focus_pin: false,
            qr: None,
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
        self.run_passkey_ticker(window, cx);
        cx.notify();
    }

    /// Whether "Create Passkey" is busy (options or the result on their
    /// way, or the ceremony open).
    pub(in crate::ui) fn passkey_creation_busy(&self) -> bool {
        self.privacy.passkeys.ceremony.is_some()
            || self.live.as_ref().is_some_and(|live| {
                live.driver.passkey_parameters_pending()
                    || live.driver.session.settings.passkeys.adding
            })
    }

    pub(in crate::ui) fn run_passkey_ticker(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.privacy.passkeys.ticking {
            return;
        }
        self.privacy.passkeys.ticking = true;
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                let keep = this
                    .update_in(cx, |this, window, cx| {
                        this.tick_passkey_ceremony(window, cx)
                    })
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
            let _ = this.update(cx, |this, _| this.privacy.passkeys.ticking = false);
        })
        .detach();
    }

    /// One tick: start the ceremony once the options are there, follow
    /// the worker, send its result, close the box once the passkey is
    /// added. `false` when there is nothing left to follow.
    pub(in crate::ui) fn tick_passkey_ceremony(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut changed = false;
        let (start, start_login, added) = match self.live.as_mut() {
            Some(live) => {
                let state = &mut live.driver.session.settings.passkeys;
                (
                    state.ceremony.take(),
                    state.login.ceremony.take(),
                    std::mem::take(&mut state.added),
                )
            }
            None => (None, None, false),
        };
        if let Some(options) = start {
            self.privacy.passkeys.ceremony = Some(CeremonyUi::new(
                CeremonyMode::Register,
                first_sheet(),
                Some(webauthn::start_register(options, parent_window(window))),
            ));
            changed = true;
        }
        if let Some(options) = start_login {
            self.privacy.passkeys.ceremony = Some(CeremonyUi::new(
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
            .ceremony
            .as_mut()
            .and_then(|ui| ui.ceremony.as_mut())
            .map(Ceremony::poll)
            .unwrap_or_default();
        for event in events {
            changed = true;
            self.apply_ceremony_event(event);
        }
        self.focus_passkey_pin_if_asked(window, cx);
        if added {
            // tdesktop closes the box once `registerPasskey` is done.
            self.privacy.passkeys.about_open = false;
            changed = true;
        }
        if changed {
            cx.notify();
        }
        self.passkey_creation_busy() || self.passkey_login_busy()
    }

    /// The PIN sheet just opened: the field takes focus.
    fn focus_passkey_pin_if_asked(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let asked = self
            .privacy
            .passkeys
            .ceremony
            .as_mut()
            .is_some_and(|ui| std::mem::take(&mut ui.focus_pin));
        if asked && let Some(input) = self.privacy.passkeys.pin_input.clone() {
            input.update(cx, |input, cx| input.focus(window, cx));
        }
    }

    fn apply_ceremony_event(&mut self, event: CeremonyEvent) {
        match event {
            CeremonyEvent::NeedPin { invalid } => {
                if let Some(ui) = self.privacy.passkeys.ceremony.as_mut() {
                    ui.sheet = CeremonySheet::Pin { invalid };
                    ui.focus_pin = true;
                }
            }
            CeremonyEvent::Hybrid { qr } => {
                if let Some(ui) = self.privacy.passkeys.ceremony.as_mut() {
                    ui.qr = qr.and_then(|qr| hybrid::render_cable_qr(&qr.0));
                    ui.sheet = CeremonySheet::Choose {
                        bluetooth: ui.qr.is_some(),
                    };
                }
            }
            CeremonyEvent::PhoneConnecting => self.set_ceremony_sheet(CeremonySheet::Connecting),
            CeremonyEvent::PhoneContinue => self.set_ceremony_sheet(CeremonySheet::Continue),
            // tdesktop closes the phone box and shows the touch box.
            CeremonyEvent::UseKey => self.set_ceremony_sheet(CeremonySheet::Key),
            CeremonyEvent::NoKey => {
                // tdesktop `ShowCableToast`; the QR stays.
                self.connection
                    .set_note(StatusNote::silent(tr(keys::PASSKEY_ERROR_NO_DEVICE)));
            }
            CeremonyEvent::Registered(registration) => {
                // The dialog closes; the server gets the result.
                let ui = self.privacy.passkeys.ceremony.take();
                if ui.is_some_and(|ui| ui.mode == CeremonyMode::Register)
                    && let Some(live) = self.live.as_mut()
                {
                    let _ = live.driver.finish_register(&registration);
                }
            }
            CeremonyEvent::SignedIn(assertion) => {
                // tdesktop closes the touch box and calls
                // `FinishPasskeyLogin`; the auth state takes it from there.
                let ui = self.privacy.passkeys.ceremony.take();
                if ui.is_some_and(|ui| ui.mode == CeremonyMode::Login)
                    && let Some(live) = self.live.as_mut()
                {
                    let _ = live.driver.finish_login(&assertion);
                }
            }
            CeremonyEvent::Failed(CeremonyError::Cancelled) => {
                self.privacy.passkeys.ceremony = None;
            }
            CeremonyEvent::Failed(error)
                if self.privacy.passkeys.ceremony.as_ref().is_some_and(|ui| {
                    matches!(ui.sheet, CeremonySheet::System | CeremonySheet::Probing)
                }) =>
            {
                // tdesktop `ShowNativeError`: no error sheet over the
                // system dialog, a toast with the generic reason; a phone
                // ceremony that fails before its box shows toasts its
                // reason (`cable_ceremony.cpp` `finish`).
                let Some(ui) = self.privacy.passkeys.ceremony.take() else {
                    return;
                };
                quill::webauthn::log(&format!("{:?} failed: {error:?}", ui.mode));
                self.connection
                    .set_note(StatusNote::error(tr(match (ui.sheet, ui.mode) {
                        (CeremonySheet::Probing, CeremonyMode::Register) => register_error(&error),
                        (CeremonySheet::Probing, CeremonyMode::Login) => login_error(&error),
                        (_, CeremonyMode::Register) => keys::PASSKEY_ERROR_REGISTER,
                        (_, CeremonyMode::Login) => keys::PASSKEY_ERROR_LOGIN,
                    })));
            }
            CeremonyEvent::Failed(error) => {
                if let Some(ui) = self.privacy.passkeys.ceremony.as_mut() {
                    let reason = match ui.mode {
                        CeremonyMode::Register => register_error(&error),
                        CeremonyMode::Login => login_error(&error),
                    };
                    quill::webauthn::log(&format!("{:?} failed: {error:?}", ui.mode));
                    ui.sheet = CeremonySheet::Error(reason);
                    ui.ceremony = None;
                    ui.qr = None;
                }
            }
        }
    }

    fn set_ceremony_sheet(&mut self, sheet: CeremonySheet) {
        if let Some(ui) = self.privacy.passkeys.ceremony.as_mut() {
            ui.sheet = sheet;
            ui.qr = None;
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
        let Some(ui) = self.privacy.passkeys.ceremony.as_mut() else {
            return;
        };
        if !matches!(ui.sheet, CeremonySheet::Pin { .. }) {
            return;
        }
        let pin = Zeroizing::new(input.read(cx).value().to_string());
        if pin.is_empty() {
            input.update(cx, |input, cx| input.focus(window, cx));
            return;
        }
        input.update(cx, |input, cx| input.set_value("", window, cx));
        if let Some(ceremony) = ui.ceremony.as_ref() {
            ceremony.submit_pin(pin);
        }
        ui.sheet = CeremonySheet::Key;
        cx.notify();
    }

    /// Cancel, Close, Esc: the key gets CTAPHID_CANCEL and the dialog
    /// closes (tdesktop: the box closing cancels the ceremony).
    pub(in crate::ui) fn close_passkey_ceremony(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(ui) = self.privacy.passkeys.ceremony.take()
            && let Some(ceremony) = ui.ceremony
        {
            ceremony.cancel();
        }
        if let Some(input) = self.privacy.passkeys.pin_input.clone() {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        cx.notify();
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
            let (mode, sheet) = this
                .privacy
                .passkeys
                .ceremony
                .as_ref()
                .map_or((CeremonyMode::Register, CeremonySheet::Key), |ui| {
                    (ui.mode, ui.sheet)
                });
            let pin_input = this.privacy.passkeys.pin_input.clone();
            let bottom = hybrid::bottom_button(sheet, cx);
            if let Some((title, body)) = this.cable_sheet(mode, sheet, cx) {
                let footer = div().flex().w_full().children(bottom);
                return Self::ceremony_dialog(dialog, title, body, footer, on_close);
            }
            let (title, about) = match sheet {
                CeremonySheet::Key => (keys::PASSKEY_TOUCH, keys::PASSKEY_INSERT),
                CeremonySheet::Pin { invalid: false } => {
                    (keys::PASSKEY_PIN_TITLE, keys::PASSKEY_PIN_ABOUT)
                }
                CeremonySheet::Pin { invalid: true } => {
                    (keys::PASSKEY_PIN_TITLE, keys::PASSKEY_PIN_INVALID)
                }
                CeremonySheet::Error(reason) => (keys::PASSKEY_CABLE_ERROR_TITLE, reason),
                // The phone sheets were built above; Probing never shows.
                CeremonySheet::Probing
                | CeremonySheet::Choose { .. }
                | CeremonySheet::Connecting
                | CeremonySheet::Continue
                | CeremonySheet::System => (
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
            .ceremony
            .as_ref()
            .is_some_and(|ui| ui.sheet != CeremonySheet::Probing),
        QuillApp::build_passkey_ceremony_dialog,
    )
    .closes(|app, window, cx| app.close_passkey_ceremony(window, cx)),
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
        let sheet = match std::env::var("QUILL_DEMO_PASSKEY_CEREMONY")
            .unwrap_or_default()
            .as_str()
        {
            "pin" => CeremonySheet::Pin { invalid: false },
            "pin-invalid" => CeremonySheet::Pin { invalid: true },
            "error" => CeremonySheet::Error(keys::PASSKEY_ERROR_NO_DEVICE),
            "windows" => CeremonySheet::System,
            "choose" => CeremonySheet::Choose { bluetooth: true },
            "bluetooth-off" => CeremonySheet::Choose { bluetooth: false },
            "connecting" => CeremonySheet::Connecting,
            "continue" => CeremonySheet::Continue,
            "phone-error" => CeremonySheet::Error(keys::PASSKEY_CABLE_ERROR),
            _ => CeremonySheet::Key,
        };
        let mut ui = CeremonyUi::new(CeremonyMode::Register, sheet, None);
        if sheet == (CeremonySheet::Choose { bluetooth: true }) {
            ui.qr = hybrid::render_cable_qr(&hybrid::demo_qr());
        }
        self.privacy.passkeys.ceremony = Some(ui);
        cx.notify();
    }

    /// Tests: show a sheet without a worker.
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(in crate::ui) fn show_passkey_sheet(
        &mut self,
        sheet: CeremonySheet,
        cx: &mut Context<Self>,
    ) {
        let mode = self
            .privacy
            .passkeys
            .ceremony
            .as_ref()
            .map_or(CeremonyMode::Register, |ui| ui.mode);
        self.privacy.passkeys.ceremony = Some(CeremonyUi::new(mode, sheet, None));
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
