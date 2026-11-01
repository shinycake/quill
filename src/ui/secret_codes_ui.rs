//! The hidden options tdesktop runs from typed codes
//! (`settings/settings_codes.cpp`): `debugmode`, `viewlogs`, `testmode` and
//! `numberbuttons`. tdesktop reads the keys on the Settings list and on the
//! settings page of the sign-in screen; Quill reads them on the Settings list
//! and on the sign-in screen itself.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::*;
use quill::advanced::codes::Code;
use quill::advanced::{debug_mode, experimental, test_environment};
use quill::lang::{keys, tr, tr_with};
use quill::settings::{active_account, list_accounts, safe_app_root};
use quill::telegram::envelope::AuthorizationState;

/// Where the typed keys are being read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CodeScreen {
    SignIn,
    Settings,
}

impl QuillApp {
    /// A key was pressed anywhere in the window. Plain typed characters on
    /// the sign-in screen or the Settings list feed the code buffer.
    pub(super) fn feed_secret_code(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let modifiers = &keystroke.modifiers;
        if modifiers.control || modifiers.alt || modifiers.platform {
            return;
        }
        // Both screens are the main window's; a chat window's keys are its
        // own (the observer sees every window's).
        if self.window_scope_of(window).is_some() {
            return;
        }
        let Some(text) = keystroke.key_char.as_deref() else {
            return;
        };
        let Some(screen) = self.code_screen() else {
            return;
        };
        if let Some(code) = self.settings.advanced.codes.feed(text) {
            self.in_window_scope(None, cx, |this, cx| {
                this.run_secret_code(code, screen, window, cx)
            });
        }
    }

    fn code_screen(&self) -> Option<CodeScreen> {
        let signed_in = matches!(self.current_auth(), AuthorizationState::Ready);
        // A screenshot demo's sign-in screen (`demo_inputs`) takes them too,
        // with no connection behind it.
        if !signed_in && (self.live.is_some() || self.auth_ui.demo_inputs) {
            return Some(CodeScreen::SignIn);
        }
        (signed_in && self.settings.open && self.settings.sub.is_none())
            .then_some(CodeScreen::Settings)
    }

    fn run_secret_code(
        &mut self,
        code: Code,
        screen: CodeScreen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match code {
            Code::DebugMode => {
                self.settings.advanced.debug_confirm = Some(debug_mode::enabled());
                cx.notify();
            }
            Code::ViewLogs => self.show_log_folder(cx),
            Code::TestMode => {
                if screen == CodeScreen::SignIn {
                    self.switch_test_environment(window, cx);
                }
            }
            Code::NumberButtons => {
                let now = !experimental::fast_buttons_mode();
                self.set_experimental_option(experimental::FAST_BUTTONS_ID, now, cx);
                self.connection.set_note(StatusNote::info(if now {
                    experimental::FAST_BUTTONS_ENABLED_TOAST
                } else {
                    experimental::FAST_BUTTONS_DISABLED_TOAST
                }));
                cx.notify();
            }
        }
    }

    /// `viewlogs`: show the debug log in the file manager (tdesktop
    /// `File::ShowInFolder(log.txt)`). The file is created empty if the
    /// mode never wrote it, so there is always something to select.
    fn show_log_folder(&mut self, cx: &mut Context<Self>) {
        let Some(root) = safe_app_root() else {
            return;
        };
        let path = debug_mode::quill_log_path(&root);
        let _ = std::fs::create_dir_all(debug_mode::log_dir(&root));
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path);
        self.reveal_in_folder_or(&path, tr(keys::Q_OPS_COULD_NOT_OPEN_THE_LOGS), cx);
        cx.notify();
    }

    /// `testmode`: with one signed-out account, add one in the other
    /// environment and switch to it (tdesktop `Domain::addActivated`).
    fn switch_test_environment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.live.is_none() {
            self.demo_switch_test_environment(cx);
            return;
        }
        let Some(root) = safe_app_root() else {
            return;
        };
        let signed_out = !matches!(self.current_auth(), AuthorizationState::Ready);
        let active_is_test = test_environment::account_is_test(&root, &active_account(&root));
        let Some(action) = test_environment::plan_test_mode(
            list_accounts(&root).len(),
            signed_out,
            active_is_test,
        ) else {
            return;
        };
        if self.credentials.is_none() {
            return;
        }
        match test_environment::add_environment_account(&root, action) {
            Ok((key, toast)) => {
                let name = list_accounts(&root)
                    .into_iter()
                    .find(|record| record.key == key)
                    .map(|record| record.display_name)
                    .unwrap_or_default();
                self.switch_account(&key, &name, window, cx);
                self.connection.set_note(StatusNote::info(toast));
            }
            Err(error) => {
                self.connection.set_note(StatusNote::error(tr_with(
                    keys::Q_OPS_COULD_NOT_SWITCH_THE_ENVIRONMENT,
                    &[("error", &error.to_string())],
                )));
            }
        }
        cx.notify();
    }

    /// `testmode` on a screenshot demo's sign-in screen: the demo stands for
    /// one signed-out account, so the code always applies. Nothing is
    /// written and nothing connects; only what the screen shows changes.
    fn demo_switch_test_environment(&mut self, cx: &mut Context<Self>) {
        let signed_out = !matches!(self.current_auth(), AuthorizationState::Ready);
        let Some(action) = test_environment::plan_test_mode(
            1,
            signed_out,
            self.settings.advanced.test_environment(),
        ) else {
            return;
        };
        let to_test = action == test_environment::TestModeAction::AddTest;
        self.settings.advanced.set_demo_test_environment(to_test);
        self.connection.set_note(StatusNote::info(if to_test {
            test_environment::SWITCHED_TO_TEST
        } else {
            test_environment::SWITCHED_TO_PRODUCTION
        }));
        cx.notify();
    }

    /// The confirmed `debugmode` question (tdesktop
    /// `Application::switchDebugMode`). Turning the logs on closes the
    /// Settings layer; turning them off stops the files at once, where
    /// tdesktop restarts.
    pub(super) fn switch_debug_mode(&mut self, cx: &mut Context<Self>) {
        let root = safe_app_root();
        let api = quill::telegram::hub::TdHub::shared_api();
        if debug_mode::enabled() {
            debug_mode::debug_log(tr(keys::Q_OPS_DEBUG_LOGS_STOPPED));
            debug_mode::set_enabled(false);
            if let Some(api) = api {
                let _ = debug_mode::stop_tdlib_log(&api);
            }
        } else {
            debug_mode::set_enabled(true);
            if let (Some(api), Some(root)) = (api, root.as_deref())
                && let Err(error) = debug_mode::start_tdlib_log(&api, root)
            {
                self.connection.set_note(StatusNote::error(tr_with(
                    keys::Q_OPS_TDLIB_DEBUG_LOG_DID_NOT,
                    &[("error", &error.to_string())],
                )));
            }
            debug_mode::debug_log(tr(keys::Q_OPS_DEBUG_LOGS_STARTED));
            self.close_settings();
        }
        if let Some(root) = root.as_deref()
            && let Err(error) = debug_mode::persist(root, debug_mode::enabled())
        {
            self.connection.set_note(StatusNote::error(tr_with(
                keys::Q_OPS_COULD_NOT_REMEMBER_THE_DEBUG,
                &[("error", &error.to_string())],
            )));
        }
        cx.notify();
    }

    pub(super) fn build_debug_mode_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::DebugModeConfirm, |this, _, cx| {
                this.settings.advanced.debug_confirm = None;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let text = debug_mode::confirm_text(this.settings.advanced.debug_confirm == Some(true));
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("debug-mode-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.settings.advanced.debug_confirm = None;
                            this.close_kit_dialog_if_done(DialogKind::DebugModeConfirm, window, cx);
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("debug-mode-confirm")
                        .label(tr(keys::BOX_OK))
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.settings.advanced.debug_confirm = None;
                            this.switch_debug_mode(cx);
                            this.close_kit_dialog_if_done(DialogKind::DebugModeConfirm, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(keys::Q_OPS_DEBUG_LOGS)))
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |content, _, _| {
                        content.child(
                            div()
                                .id("debug-mode-text")
                                .text_sm()
                                .whitespace_normal()
                                .child(text),
                        )
                    },
                ))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// The `debugmode` code's question: turn the debug logs on or off.
    DebugModeConfirm => DialogSpec::new(
        1050,
        |app| app.settings.advanced.debug_confirm.is_some(),
        QuillApp::build_debug_mode_dialog,
    )
    .closes(|app, _, cx| { app.settings.advanced.debug_confirm = None; cx.notify(); }),
}
