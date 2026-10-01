//! Slice parity:auth-multi-account (UI half): the Accounts dialog — list
//! accounts, switch between them, add a new one, remove a non-active one.
//! The registry backend lives in `quill::settings` (slice, PR #214); the
//! switch flow is the documented one: `LiveConnect::shutdown` →
//! `set_active_account` → `start_live_connect_for_account`, after which the
//! existing auth UI renders whatever TDLib reports for the new account
//! (fresh account → the phone/QR/code flow; returning account → session
//! restore). Hosted in a kit `Dialog` via `window.open_dialog` (the kit
//! Phase 2 redo pattern). Removing the active account is not offered in
//! the UI — switch away from it first.

use super::app::QuillApp;
use super::connect_ui::ConnectUiStatus;
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::connect::{CLIENT_CLOSE_TIMEOUT, start_live_connect_for_account};
use quill::diagnostics::MemorySink;
use quill::ids::AccountKey;
use quill::platform::live_secret_store;
use quill::settings::{
    AccountRecord, active_account, add_account, list_accounts, remove_account, safe_app_root,
    set_active_account,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

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
                .placeholder("Name for the new account")
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
        self.accounts_ui.open = true;
        self.accounts_ui.remove_confirm = None;
        self.accounts_ui.error = None;
        cx.notify();
    }

    /// Slice parity:auth-multi-account: close the dialog and drop its
    /// working state (pending remove confirm, error, add-name input).
    pub(crate) fn close_accounts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.accounts_ui.open = false;
        self.accounts_ui.remove_confirm = None;
        self.accounts_ui.error = None;
        self.accounts_ui
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
        self.status_note = message.into();
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
        display_name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = safe_app_root() else {
            self.fail_account_switch(
                "Could not switch account: no app data directory",
                window,
                cx,
            );
            return;
        };
        let Some(credentials) = self.credentials.clone() else {
            self.fail_account_switch(
                "Could not switch account: Telegram credentials are not available",
                window,
                cx,
            );
            return;
        };
        self.close_accounts(window, cx);
        self.accepted_registration_terms = None;
        self.registration_notify_contacts = false;
        self.registration_first_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.registration_last_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.email_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.code_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.emoji_set_search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.emoji_picker_open = false;
        self.marketplace_open = false;
        self.marketplace_private = true;
        self.marketplace_error = None;
        self.marketplace_name_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.marketplace_comment_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.emoji_category = 1;
        self.emoji_visible_count = 120;
        self.emoji_search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.gif_search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.sticker_settings_open = false;
        self.sticker_search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.stop_sticker_playback();
        if let Some(mut live) = self.live.take() {
            live.shutdown(CLIENT_CLOSE_TIMEOUT);
        }
        if let Err(e) = set_active_account(&root, key) {
            self.fail_account_switch(format!("Could not switch account: {e}"), window, cx);
            return;
        }
        match start_live_connect_for_account(
            credentials,
            live_secret_store().as_ref(),
            Arc::new(MemorySink::new()),
            key.clone(),
        ) {
            Ok(live) => {
                self.live = Some(live);
                self.reload_account_keybindings(cx);
                // Clear any earlier restore-blocked label — the switch
                // succeeded and the connect is live again.
                self.connect_status = ConnectUiStatus::Live;
                self.status_note = format!("Switched to “{display_name}” — connecting…");
            }
            Err(blocker) => {
                self.connect_status = ConnectUiStatus::RestoreBlocked(blocker.user_message());
                self.status_note = format!(
                    "Could not start “{display_name}”: {}",
                    blocker.user_message()
                );
            }
        }
        cx.notify();
    }

    /// Slice parity:auth-multi-account: add a named account, then switch
    /// straight into it — the existing auth UI renders the phone/QR/code
    /// flow for the new account.
    pub(crate) fn add_account_named(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self
            .accounts_ui
            .add_name
            .read(cx)
            .value()
            .trim()
            .to_string();
        if name.is_empty() {
            self.accounts_ui.error = Some("Give the new account a name first.".into());
            cx.notify();
            return;
        }
        let Some(root) = safe_app_root() else {
            self.accounts_ui.error =
                Some("Accounts are unavailable: no app data directory.".into());
            cx.notify();
            return;
        };
        match add_account(&root, &name) {
            Ok(key) => {
                self.accounts_ui
                    .add_name
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.switch_account(&key, &name, window, cx);
            }
            Err(e) => {
                self.accounts_ui.error = Some(format!("Could not add the account: {e}"));
                cx.notify();
            }
        }
    }

    /// Slice parity:auth-multi-account: remove a non-active account (the
    /// UI never offers the active one — switch away from it first).
    pub(crate) fn remove_account_key(&mut self, key: &AccountKey, cx: &mut Context<Self>) {
        let Some(root) = safe_app_root() else {
            self.accounts_ui.error =
                Some("Accounts are unavailable: no app data directory.".into());
            cx.notify();
            return;
        };
        self.accounts_ui.remove_confirm = None;
        match remove_account(&root, key) {
            Ok(()) => {
                // Drop the account's keychain item too: otherwise the
                // lowest-free-id recycling in `add_account` hands a later
                // account the deleted account's stale DB key. Best-effort —
                // the account data is already gone; a locked keychain is
                // just an orphaned item, not a failure.
                let _ = live_secret_store().delete(key);
                self.accounts_ui.error = None;
            }
            Err(e) => {
                self.accounts_ui.error = Some(format!("Could not remove the account: {e}"));
            }
        }
        cx.notify();
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
                .child("✓ Active")
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
                        .label("Switch")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.switch_account(&key_switch, &name_c, window, cx);
                        })),
                )
                .child(
                    Button::new(format!("accounts-remove-{key_remove_arm}"))
                        .label("Remove")
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.accounts_ui.remove_confirm = Some(key_remove_arm.clone());
                            this.accounts_ui.error = None;
                            cx.notify();
                        })),
                )
                .into_any_element()
        };
        row = row.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(div().font_semibold().text_sm().child(name.clone()))
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
                                    .label("Remove")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.remove_account_key(&key_confirm, cx);
                                    })),
                            )
                            .child(
                                Button::new(format!("accounts-remove-cancel-{key_cancel}"))
                                    .label("Cancel")
                                    .ghost()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.accounts_ui.remove_confirm = None;
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
            let error = this.accounts_ui.error.clone();
            let remove_confirm = this.accounts_ui.remove_confirm.clone();
            let mut body = div().flex().flex_col().gap_3();
            if let Some(line) = error {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(danger())
                        .child(format!("Error: {line}")),
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
                        .child("Switching restarts the connection under the other account."),
                )
                .child(div().font_semibold().text_sm().child("Add another account"))
                .child(
                    Textarea::new(&this.accounts_ui.add_name)
                        .aria_label("Account display name")
                        .h(px(40.)),
                )
                .child(
                    Button::new("accounts-add")
                        .label("Add account")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.add_account_named(window, cx);
                        })),
                );
            let footer = div().flex().justify_end().child(
                Button::new("close-accounts")
                    .label("Close")
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_accounts(window, cx);
                        this.close_kit_dialog_if_done(DialogKind::Accounts, window, cx);
                    })),
            );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title("Accounts"))
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
