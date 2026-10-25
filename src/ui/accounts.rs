//! Slice parity:auth-multi-account (UI half): the Accounts dialog — list
//! accounts, switch between them, add a new one, remove a non-active one.
//! The registry backend lives in `quill::settings` (slice, PR #214). A
//! switch hands the outgoing client to the background
//! (`background_accounts`, `parity:notify-all-accounts`), persists the new
//! active account, and brings that account's background client forward or,
//! when none runs, starts one; the existing auth UI renders whatever TDLib
//! reports for it (fresh account → the phone/QR/code flow; returning
//! account → session restore). Hosted in a kit `Dialog` via `window.open_dialog` (the kit
//! Phase 2 redo pattern). Removing the active account is not offered in
//! the UI — switch away from it first.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::AccountKey;
use quill::lang::{keys, tr, tr_with};
use quill::settings::{
    AccountRecord, active_account, add_account, list_accounts, safe_app_root, set_active_account,
};
use std::cell::RefCell;
use std::rc::Rc;

/// Slice parity:auth-multi-account: working state for the Accounts dialog.
pub(crate) struct AccountsUiState {
    pub(crate) open: bool,
    /// Non-active account awaiting inline remove confirmation — no second
    /// `DialogKind` for what is a two-button inline question.
    pub(crate) remove_confirm: Option<AccountKey>,
    pub(crate) add_name: Entity<TextareaState>,
    /// Add/remove failure text, shown inside the dialog. Switch failures
    /// close the dialog and go to the status-note/notification path.
    pub(crate) error: Option<String>,
}

impl AccountsUiState {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        let add_name = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_OPS_NAME_FOR_THE_NEW_ACCOUNT))
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        Self {
            open: false,
            remove_confirm: None,
            add_name,
            error: None,
        }
    }
}

impl QuillApp {
    /// Slice parity:auth-multi-account: open the Accounts dialog. The
    /// registry reads are local and synchronous — no TDLib round-trip.
    pub(crate) fn open_accounts(&mut self, cx: &mut Context<Self>) {
        if quill::agent::ui_driver::guard::refuse_hard_if_injecting(
            quill::agent::ui_driver::guard::HardSurface::Account,
        ) {
            return;
        }
        self.account.accounts.open = true;
        self.account.accounts.remove_confirm = None;
        self.account.accounts.error = None;
        cx.notify();
    }

    /// Slice parity:auth-multi-account: close the dialog and drop its
    /// working state (pending remove confirm, error, add-name input).
    pub(crate) fn close_accounts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.account.accounts.open = false;
        self.account.accounts.remove_confirm = None;
        self.account.accounts.error = None;
        self.account
            .accounts
            .add_name
            .update(cx, |input, cx| input.set_value("", window, cx));
    }

    /// Slice parity:auth-multi-account: switch failure → the
    /// status line.
    fn fail_account_switch(
        &mut self,
        message: impl Into<String>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.connection.set_note(StatusNote::error(message));
        cx.notify();
    }

    /// Slice parity:auth-multi-account: the documented switch flow —
    /// shutdown the current client, persist the new active account, start
    /// a fresh connect for it. The dialog closes first so the main UI
    /// (then the auth UI for the new account) is what the user sees;
    /// errors surface via the status line.
    pub(crate) fn switch_account(
        &mut self,
        key: &AccountKey,
        _display_name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The lock-screen log out is deleting every account's data; the
        // connection starts once it is gone.
        if self.background.wiping {
            return;
        }
        let Some(root) = safe_app_root() else {
            self.fail_account_switch(tr(keys::Q_OPS_COULD_NOT_SWITCH_ACCOUNT_NO), window, cx);
            return;
        };
        let Some(_) = self.credentials.as_ref() else {
            self.fail_account_switch(
                tr(keys::Q_OPS_COULD_NOT_SWITCH_ACCOUNT_TELEGRAM),
                window,
                cx,
            );
            return;
        };
        self.clear_device_qr();
        self.close_accounts(window, cx);
        self.auth_ui.accepted_registration_terms = None;
        self.auth_ui.registration_notify_contacts = false;
        self.auth_ui
            .registration_first_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.auth_ui
            .registration_last_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.auth_ui
            .email_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.auth_ui
            .code_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.pickers
            .emoji_set_search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.pickers.media_panel.open = false;
        self.payments.marketplace_open = false;
        self.payments.marketplace_private = true;
        self.payments.marketplace_error = None;
        self.payments
            .marketplace_name_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.payments
            .marketplace_comment_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.pickers
            .emoji_search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.pickers
            .gif_search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.settings.sticker_settings_open = false;
        self.pickers
            .sticker_search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.stop_sticker_playback();
        // Chat windows draw the account on screen: the outgoing account's
        // close (and come back when it is switched to again).
        self.close_chat_windows_for_switch(cx);
        self.history.scroll_memory.clear();
        // Quill's notification windows point at the account on screen.
        self.close_all_notify_windows(cx);
        // `parity:notify-all-accounts`: the outgoing account stays connected
        // in the background, and an account already running there comes
        // forward as it is instead of restarting.
        self.retire_live_to_background(cx);
        if let Err(e) = set_active_account(&root, key) {
            self.fail_account_switch(
                tr_with(
                    keys::Q_OPS_COULD_NOT_SWITCH_ACCOUNT,
                    &[("e", &e.to_string())],
                ),
                window,
                cx,
            );
            return;
        }
        quill::icon_badge::invalidate_icon_badge();
        match self.take_background_live(key, cx) {
            Some(live) => self.adopt_live(live, cx),
            None => self.start_connection(cx),
        }
        cx.notify();
    }

    /// Slice parity:auth-multi-account: add a named account, then switch
    /// straight into it — the existing auth UI renders the phone/QR/code
    /// flow for the new account.
    pub(crate) fn add_account_named(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self
            .account
            .accounts
            .add_name
            .read(cx)
            .value()
            .trim()
            .to_string();
        if name.is_empty() {
            self.account.accounts.error = Some(tr(keys::Q_OPS_GIVE_THE_NEW_ACCOUNT_A).into());
            cx.notify();
            return;
        }
        let Some(root) = safe_app_root() else {
            self.account.accounts.error =
                Some(tr(keys::Q_OPS_ACCOUNTS_ARE_UNAVAILABLE_NO_APP).into());
            cx.notify();
            return;
        };
        // tdesktop `Domain::maxAccounts`: three accounts, one more per
        // Premium account (this build only knows the connected one).
        let premium =
            usize::from(self.session().and_then(|s| s.payments.premium_option) == Some(true));
        if let Some(note) = quill::signin::add_account_blocker(list_accounts(&root).len(), premium)
        {
            self.account.accounts.error = Some(note);
            cx.notify();
            return;
        }
        match add_account(&root, &name) {
            Ok(key) => {
                self.account
                    .accounts
                    .add_name
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.switch_account(&key, &name, window, cx);
            }
            Err(e) => {
                self.account.accounts.error = Some(tr_with(
                    keys::Q_OPS_COULD_NOT_ADD_THE_ACCOUNT,
                    &[("e", &e.to_string())],
                ));
                cx.notify();
            }
        }
    }

    /// Slice parity:auth-multi-account: remove a non-active account (the
    /// UI never offers the active one — switch away from it first).
    pub(crate) fn remove_account_key(&mut self, key: &AccountKey, cx: &mut Context<Self>) {
        let Some(root) = self.accounts_root() else {
            self.account.accounts.error =
                Some(tr(keys::Q_OPS_ACCOUNTS_ARE_UNAVAILABLE_NO_APP).into());
            cx.notify();
            return;
        };
        self.remove_account_in(root, key, cx);
    }

    /// One account row: name, the active badge or Switch/Remove actions,
    /// and the inline remove confirm.
    fn account_row(
        &self,
        record: &AccountRecord,
        is_active: bool,
        confirming_remove: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = if record.display_name.trim().is_empty() {
            record.key.to_string()
        } else {
            record.display_name.clone()
        };
        let mut row = div().flex().flex_col().gap_1();
        let actions: AnyElement = if is_active {
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(tr(keys::Q_OPS_ACTIVE_2))
                .into_any_element()
        } else {
            let key_switch = record.key.clone();
            let name_c = name.clone();
            let key_remove_arm = record.key.clone();
            div()
                .flex()
                .gap_2()
                .child(
                    Button::new(format!("accounts-switch-{key_switch}"))
                        .label(tr(keys::Q_OPS_SWITCH))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.switch_account(&key_switch, &name_c, window, cx);
                        })),
                )
                .child(
                    Button::new(format!("accounts-remove-{key_remove_arm}"))
                        .label(tr(keys::BOX_REMOVE))
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.account.accounts.remove_confirm = Some(key_remove_arm.clone());
                            this.account.accounts.error = None;
                            cx.notify();
                        })),
                )
                .into_any_element()
        };
        // Another account's unread counter or status
        // (`parity:notify-all-accounts`).
        let badge = (!is_active)
            .then(|| self.background_account_badge(&record.key, cx))
            .flatten();
        row = row.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .min_w_0()
                        .child(
                            div()
                                .font_semibold()
                                .text_sm()
                                .truncate()
                                .child(super::bidi_line::one_line_plain(name.clone())),
                        )
                        .children(badge),
                )
                .child(actions),
        );
        if confirming_remove {
            let key_confirm = record.key.clone();
            let key_cancel = record.key.clone();
            row = row.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "Remove “{name}”? This deletes its local data. Telegram-side data is untouched."
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                Button::new(format!("accounts-remove-confirm-{key_confirm}"))
                                    .label(tr(keys::BOX_REMOVE))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.remove_account_key(&key_confirm, cx);
                                    })),
                            )
                            .child(
                                Button::new(format!("accounts-remove-cancel-{key_cancel}"))
                                    .label(tr(keys::CANCEL))
                                    .ghost()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.account.accounts.remove_confirm = None;
                                        cx.notify();
                                    })),
                            ),
                    ),
            );
        }
        row.into_any_element()
    }

    /// Slice parity:auth-multi-account: the Accounts dialog, hosted in a
    /// kit `Dialog` via `window.open_dialog` (the kit Phase 2 redo
    /// pattern). Esc / backdrop / ✕ clear state via `on_close`.
    pub(crate) fn build_accounts_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::Accounts, |this, window, cx| {
                this.close_accounts(window, cx);
            });
        app.update(cx, |this, cx| {
            let root = safe_app_root();
            let accounts: Vec<AccountRecord> = root
                .as_ref()
                .map(|root| list_accounts(root))
                .unwrap_or_default();
            let active: Option<AccountKey> = root.as_ref().map(|root| active_account(root));
            let error = this.account.accounts.error.clone();
            let remove_confirm = this.account.accounts.remove_confirm.clone();
            let mut body = div().flex().flex_col().gap_3();
            if let Some(line) = error {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(danger())
                        .child(tr_with(keys::Q_OPS_ERROR_2, &[("line", &line.to_string())])),
                );
            }
            for record in &accounts {
                let is_active = active.as_ref().is_some_and(|a| a == &record.key);
                body = body.child(this.account_row(
                    record,
                    is_active,
                    remove_confirm.as_ref() == Some(&record.key),
                    cx,
                ));
            }
            body = body
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(keys::Q_OPS_ACCOUNTS_YOU_ARE_NOT_USING)),
                )
                .child(
                    div()
                        .font_semibold()
                        .text_sm()
                        .child(tr(keys::Q_OPS_ADD_ANOTHER_ACCOUNT)),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr_with(
                            keys::Q_LEFTOVERS_ACCOUNT_LIMIT_ABOUT,
                            &[
                                ("limit", &(quill::signin::MAX_ACCOUNTS).to_string()),
                                (
                                    "premium_limit",
                                    &(quill::signin::MAX_ACCOUNTS_PREMIUM).to_string(),
                                ),
                            ],
                        )),
                )
                .child(
                    Textarea::new(&this.account.accounts.add_name)
                        .aria_label(tr(keys::Q_OPS_ACCOUNT_DISPLAY_NAME))
                        .h(px(40.)),
                )
                .child(
                    Button::new("accounts-add")
                        .label(tr(keys::Q_OPS_ADD_ACCOUNT))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.add_account_named(window, cx);
                        })),
                );
            let footer = div().flex().justify_end().child(
                Button::new("close-accounts")
                    .label(tr(keys::CLOSE))
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_accounts(window, cx);
                        this.close_kit_dialog_if_done(DialogKind::Accounts, window, cx);
                    })),
            );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(keys::Q_OPS_ACCOUNTS)))
                .content(crate::ui::shell::scrollable_dialog_content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
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
}

crate::ui::shell::register_dialogs! {
    /// Slice parity:auth-multi-account (UI): the Accounts dialog (list /
    /// switch / add / remove accounts).
    Accounts => DialogSpec::new(
        // Slice parity:auth-multi-account (UI): accounts sit with the
        // other settings-level dialogs (lowest priority band).
        7000,
        |app| app.account.accounts.open,
        QuillApp::build_accounts_dialog,
    ),
}
