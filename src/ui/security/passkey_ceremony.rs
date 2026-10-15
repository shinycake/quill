//! The passkey ceremony dialog: creating a passkey on a USB security key
//! (tdesktop `webauthn/webauthn_common.cpp` `ShowTouchBox` / `ShowPinBox`,
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
//! ticker started by "Create Passkey" moves the options from the server
//! to the worker, the worker's events to the sheets, and the result to
//! `addLoginPasskey`.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::TestSupportExt;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{Key, keys, tr};
use quill::webauthn::ceremony::UsbKeys;
use quill::webauthn::{Ceremony, CeremonyError, CeremonyEvent};
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
}

/// The open ceremony dialog.
pub(in crate::ui) struct CeremonyUi {
    pub(in crate::ui) sheet: CeremonySheet,
    /// The running ceremony; `None` once it ended (and in demos).
    ceremony: Option<Ceremony>,
    /// Focus the PIN field on the next render.
    focus_pin: bool,
}

impl std::fmt::Debug for CeremonyUi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CeremonyUi")
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
        CeremonyError::Cancelled | CeremonyError::NoCredentials | CeremonyError::Other(_) => {
            keys::PASSKEY_ERROR_REGISTER
        }
    }
}

impl QuillApp {
    /// The PIN field, made once with the window it lives in.
    fn passkey_pin_input(
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

    fn run_passkey_ticker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        let (start, added) = match self.live.as_mut() {
            Some(live) => {
                let state = &mut live.driver.session.settings.passkeys;
                (state.ceremony.take(), std::mem::take(&mut state.added))
            }
            None => (None, false),
        };
        if let Some(options) = start {
            self.privacy.passkeys.ceremony = Some(CeremonyUi {
                sheet: CeremonySheet::Key,
                ceremony: Some(Ceremony::register(options, Box::new(UsbKeys))),
                focus_pin: false,
            });
            changed = true;
        }
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
        self.passkey_creation_busy()
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
            CeremonyEvent::Registered(registration) => {
                // The dialog closes; the server gets the result.
                self.privacy.passkeys.ceremony = None;
                if let Some(live) = self.live.as_mut() {
                    let _ = live.driver.finish_register(&registration);
                }
            }
            CeremonyEvent::Failed(CeremonyError::Cancelled) => {
                self.privacy.passkeys.ceremony = None;
            }
            CeremonyEvent::Failed(error) => {
                quill::webauthn::log(&format!("registration failed: {error:?}"));
                if let Some(ui) = self.privacy.passkeys.ceremony.as_mut() {
                    ui.sheet = CeremonySheet::Error(register_error(&error));
                    ui.ceremony = None;
                }
            }
            // Sign-in is not started from Settings.
            CeremonyEvent::SignedIn(_) => {}
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
            let sheet = this
                .privacy
                .passkeys
                .ceremony
                .as_ref()
                .map_or(CeremonySheet::Key, |ui| ui.sheet);
            let pin_input = this.privacy.passkeys.pin_input.clone();
            let (title, about) = match sheet {
                CeremonySheet::Key => (keys::PASSKEY_TOUCH, keys::PASSKEY_INSERT),
                CeremonySheet::Pin { invalid: false } => {
                    (keys::PASSKEY_PIN_TITLE, keys::PASSKEY_PIN_ABOUT)
                }
                CeremonySheet::Pin { invalid: true } => {
                    (keys::PASSKEY_PIN_TITLE, keys::PASSKEY_PIN_INVALID)
                }
                CeremonySheet::Error(reason) => (keys::PASSKEY_CABLE_ERROR_TITLE, reason),
            };
            let mut body = div().flex().flex_col().gap_3().child(
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
            );
            let is_pin = matches!(sheet, CeremonySheet::Pin { .. });
            if is_pin && let Some(input) = &pin_input {
                body = body.child(
                    Input::new(input)
                        .id("passkey-pin")
                        .aria_label(tr(keys::PASSKEY_PIN_PLACEHOLDER)),
                );
            }
            let mut footer = div().flex().justify_end().gap_2();
            match sheet {
                CeremonySheet::Error(_) => {
                    footer = footer.child(
                        Button::new("passkey-ceremony-close")
                            .label(tr(keys::CLOSE))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_passkey_ceremony(window, cx);
                                this.close_kit_dialog_if_done(
                                    DialogKind::PasskeyCeremony,
                                    window,
                                    cx,
                                );
                            })),
                    );
                }
                _ => {
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
            dialog
                .title(crate::ui::shell::dialog_title(tr(title)))
                .width(px(360.))
                .content(crate::ui::shell::scrollable_dialog_content(
                    super::passkeys::once(body.into_any_element()),
                ))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// The ceremony over the "Protect your account" box.
    PasskeyCeremony => DialogSpec::new(
        605,
        |app| app.privacy.passkeys.ceremony.is_some(),
        QuillApp::build_passkey_ceremony_dialog,
    ),
}

crate::ui::screenshot_demo::register_demos![
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-passkey-ceremony")
        .setup(QuillApp::demo_passkey_ceremony),
];

impl QuillApp {
    /// `ready-passkey-ceremony`: the ceremony dialog over the box, with
    /// `QUILL_DEMO_PASSKEY_CEREMONY` set to `key` (default), `pin`,
    /// `pin-invalid` or `error`. No worker runs.
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
            _ => CeremonySheet::Key,
        };
        self.privacy.passkeys.ceremony = Some(CeremonyUi {
            sheet,
            ceremony: None,
            focus_pin: false,
        });
        cx.notify();
    }

    /// Tests: show a sheet without a worker.
    #[cfg(all(test, feature = "demo-capture"))]
    pub(in crate::ui) fn show_passkey_sheet(
        &mut self,
        sheet: CeremonySheet,
        cx: &mut Context<Self>,
    ) {
        self.privacy.passkeys.ceremony = Some(CeremonyUi {
            sheet,
            ceremony: None,
            focus_pin: false,
        });
        cx.notify();
    }

    /// Tests: feed a worker event as if the ceremony sent it.
    #[cfg(all(test, feature = "demo-capture"))]
    pub(in crate::ui) fn feed_ceremony_event(
        &mut self,
        event: CeremonyEvent,
        cx: &mut Context<Self>,
    ) {
        self.apply_ceremony_event(event);
        cx.notify();
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "passkey_ceremony_tests.rs"]
mod tests;
