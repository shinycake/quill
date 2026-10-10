use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::*;
use gpui_kit::*;
use std::cell::RefCell;
use std::rc::Rc;
use zeroize::Zeroize;
/// Slice A9: account lifecycle UI — the A7 UI half. One "Account" dialog
/// with the self-destruct TTL picker and the delete-account danger zone.
/// TGX-verbatim copy (`SettingsPrivacyController`,
/// `TdlibUi::permanentlyDeleteAccount`); the backend (`delete_account`,
/// `get_account_ttl`, `set_account_ttl`) shipped in slice A7.
///
/// TGX TTL options (months → `setAccountTtl` days), verbatim from
/// `SettingsPrivacyController.onApplySettings`.
pub(crate) const ACCOUNT_TTL_OPTIONS: [(u8, i32); 6] =
    [(1, 31), (3, 91), (6, 181), (12, 366), (18, 546), (24, 730)];

/// TGX `buildAccountTtl` display rule, verbatim: under 30 days → days,
/// otherwise whole months, whole years when they divide evenly.
pub(crate) fn format_account_ttl(days: i32) -> String {
    if days < 30 {
        return if days == 1 {
            "1 day".to_string()
        } else {
            format!("{days} days")
        };
    }
    let months = days / 30;
    if months % 12 == 0 {
        let years = months / 12;
        if years == 1 {
            "1 year".to_string()
        } else {
            format!("{years} years")
        }
    } else if months == 1 {
        "1 month".to_string()
    } else {
        format!("{months} months")
    }
}

/// Slice auth-logout-warning: TGX `SignOutHint2` warning copy, verified
/// verbatim from translations.telegram.org (android_x/settings/SignOutHint2,
/// 2026-09-30): "Are you sure you want to log out as %1$s? Note that you
/// can seamlessly use Telegram on all your devices at once. Remember,
/// logging out kills all your Secret Chats. Downloaded media will be
/// erased from this device." Quill keeps the two consequence sentences
/// (the checklist item); the multi-device note is TGX-mobile context and
/// doesn't apply to this single-account client. `%1$s` (the account name)
/// is skipped — the dialog already sits in the account section.
pub(crate) const LOGOUT_WARNING_COPY: &str = "Remember, logging out kills all your Secret Chats. Downloaded media will be erased from this device.";

/// TGX option labels (`xMonths` plural): "1 month", "3 months", ….
fn ttl_option_label(months: u8) -> String {
    if months == 1 {
        "1 month".to_string()
    } else {
        format!("{months} months")
    }
}

/// Slice A9: working state for the Account dialog. Lives on `QuillApp`;
/// the inputs are cleared on submit/close — passwords never linger (the
/// A2 rule: auth secrets ride the request JSON only).
pub(crate) struct AccountLifecycleState {
    pub(crate) open: bool,
    pub(crate) confirm_delete: bool,
    /// Slice auth-logout-warning: the log-out confirm banner is armed
    /// (the `confirm_delete` pattern — an inline confirm, since kit
    /// dialogs never stack).
    pub(crate) confirm_logout: bool,
    pub(crate) reason: Entity<TextareaState>,
    pub(crate) password: Entity<TextareaState>,
}

impl AccountLifecycleState {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        let reason = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Reason (optional)")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let password = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Two-step verification password")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        Self {
            open: false,
            confirm_delete: false,
            confirm_logout: false,
            reason,
            password,
        }
    }
}

impl QuillApp {
    /// Slice A9: open the Account dialog. Live: guarded fetches of the
    /// authoritative `accountTtl` and `passwordState` (cached values
    /// reused, in-flight fetches deduped by the drivers). Demo: the
    /// fixtures are already injected.
    pub(crate) fn open_account_lifecycle(&mut self, cx: &mut Context<Self>) {
        self.account.lifecycle.open = true;
        self.account.lifecycle.confirm_delete = false;
        self.account.lifecycle.confirm_logout = false;
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.get_account_ttl();
            let _ = live.driver.get_default_auto_delete();
            let _ = live.driver.fetch_password_state();
        }
        cx.notify();
    }

    /// Slice A9: close the dialog and clear its inputs — the password
    /// must not linger after the dialog is gone (the A2 rule).
    pub(crate) fn close_account_lifecycle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.account.lifecycle.open = false;
        self.account.lifecycle.confirm_delete = false;
        self.account.lifecycle.confirm_logout = false;
        for input in [
            &self.account.lifecycle.reason,
            &self.account.lifecycle.password,
        ] {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
    }

    /// Slice A9: one `setAccountTtl` round-trip. The confirmed days land
    /// from the authoritative `ok` (never an optimistic write); the
    /// driver gates one mutation at a time and surfaces `account_error`.
    pub(crate) fn submit_account_ttl(&mut self, days: i32, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.set_account_ttl(days);
        }
        cx.notify();
    }

    /// One `setDefaultMessageAutoDeleteTime` round-trip (Settings →
    /// Privacy → Auto-delete messages). The confirmed value lands from the
    /// authoritative `ok`; the demo session applies it directly.
    pub(crate) fn submit_default_auto_delete(&mut self, seconds: i32, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.set_default_auto_delete(seconds);
        } else if let Some(session) = self.demo_session.as_mut() {
            session.settings.default_auto_delete_secs = Some(seconds);
        }
        cx.notify();
    }

    /// The default auto-delete timer section: tdesktop's Privacy page
    /// "Auto-delete messages" row, with 1 day / 1 week / 1 month presets
    /// (`setDefaultMessageAutoDeleteTime`).
    fn account_auto_delete_body(
        &self,
        cx: &mut Context<Self>,
        mut body: Div,
        secs: Option<i32>,
        busy: bool,
        error: Option<String>,
    ) -> Div {
        const PRESETS: [(&str, i32); 4] = [
            ("Off", 0),
            ("1 day", 86_400),
            ("1 week", 604_800),
            ("1 month", 2_678_400),
        ];
        body = body
            .child(
                div()
                    .font_semibold()
                    .text_sm()
                    .child("Auto-delete messages"),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Automatically delete new messages in all your new chats after a certain period of time. You can also set a timer for each chat in its menu."),
            )
            .child(div().text_sm().child(match secs {
                Some(secs) => format!("Currently: {}", quill::auto_delete::format_ttl(secs)),
                None if busy => "Loading\u{2026}".to_string(),
                None => "No data yet.".to_string(),
            }));
        if let Some(line) = error {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(danger())
                    .child(format!("Error: {line}")),
            );
        }
        let active_ix = secs.and_then(|secs| PRESETS.iter().position(|(_, s)| *s == secs));
        body.child(
            RadioGroup::horizontal("default-auto-delete-options")
                .selected_index(active_ix)
                .children(
                    PRESETS
                        .iter()
                        .map(|(label, _)| Radio::new(format!("default-ttl-{label}")).label(*label)),
                )
                .on_click(cx.listener(move |this, &ix: &usize, _, cx| {
                    this.submit_default_auto_delete(PRESETS[ix].1, cx);
                })),
        )
    }

    /// Slice A9: one `deleteAccount` round-trip. The password rides the
    /// request JSON only — it is zeroized and both inputs cleared before
    /// this returns (the A2 rule).
    pub(crate) fn submit_delete_account(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let reason = self
            .account
            .lifecycle
            .reason
            .read(cx)
            .value()
            .trim()
            .to_string();
        let mut password = self.account.lifecycle.password.read(cx).value().to_string();
        for input in [
            &self.account.lifecycle.reason,
            &self.account.lifecycle.password,
        ] {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        self.account.lifecycle.confirm_delete = false;
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.delete_account(&reason, &password);
        }
        password.zeroize();
        cx.notify();
    }

    /// Slice auth-logout-warning: send `logOut`; the driver guards on
    /// Ready. TDLib drives `authorizationStateLoggingOut → Closed`
    /// (`Session::set_auth`); `poll_live` restarts the live connection
    /// on Closed so the user lands back on the login screen. The
    /// Account dialog closes — the blocking "Signing out" auth view
    /// takes over.
    pub(crate) fn submit_logout(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut()
            && let Err(err) = live.driver.request_logout()
        {
            // N2 fix-up: a failed send must not silently close the
            // dialog — stay put and say so (near-impossible behind the
            // Ready guard).
            self.connection.status_note = format!("log out failed: {err:?}");
            cx.notify();
            return;
        }
        self.account.lifecycle.confirm_logout = false;
        self.account.lifecycle.open = false;
        cx.notify();
    }

    /// Slice A9: the Account dialog, hosted in a kit `Dialog` via
    /// `window.open_dialog` (the kit Phase 2 redo pattern). Esc /
    /// backdrop / ✕ clear state via `on_close`.
    pub(crate) fn build_account_lifecycle_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close = QuillShell::on_close_kind(
            app,
            shell,
            DialogKind::AccountLifecycle,
            |this, window, cx| {
                this.close_account_lifecycle(window, cx);
            },
        );
        app.update(cx, |this, cx| {
            let session = this.session();
            let ttl_days = session.as_ref().and_then(|s| s.settings.account_ttl_days);
            let ttl_loading = session.is_some_and(|s| s.settings.account_ttl_loading);
            let mutating = session.is_some_and(|s| s.settings.account_mutating);
            let error = session
                .as_ref()
                .and_then(|s| s.settings.account_error.clone());
            let has_password = session
                .as_ref()
                .and_then(|s| s.auth_state.password_state.as_ref())
                .is_some_and(|p| p.has_password);
            let pw_loading = session.is_some_and(|s| s.auth_state.password_state_loading);

            let mut body = div().flex().flex_col().gap_3();
            if let Some(line) = error {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(danger())
                        .child(format!("Error: {line}")),
                );
            }
            let auto_delete = session
                .as_ref()
                .and_then(|s| s.settings.default_auto_delete_secs);
            let auto_delete_busy = session.is_some_and(|s| s.settings.default_auto_delete_busy);
            let auto_delete_error = session
                .as_ref()
                .and_then(|s| s.settings.default_auto_delete_error.clone());
            body = this.account_auto_delete_body(
                cx,
                body,
                auto_delete,
                auto_delete_busy,
                auto_delete_error,
            );
            body = this.account_ttl_body(cx, body, ttl_days, ttl_loading, mutating);
            // Slice auth-logout-warning: Log out sits between the TTL
            // picker and the delete-account danger zone (least → most
            // destructive).
            body = this.account_logout_body(cx, body);
            body = this.account_delete_body(cx, body, has_password, pw_loading, mutating);

            let footer = div().flex().justify_end().child(
                Button::new("close-account-lifecycle")
                    .label("Close")
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_account_lifecycle(window, cx);
                        this.close_kit_dialog_if_done(DialogKind::AccountLifecycle, window, cx);
                    })),
            );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title("Account"))
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

    /// Slice A9: the self-destruct TTL section — TGX
    /// `DeleteAccountIfAwayFor2` + `DeleteAccountHelp`, verbatim, with
    /// the TGX option set as a kit `RadioGroup` (the chat-TTL picker
    /// pattern).
    fn account_ttl_body(
        &self,
        cx: &mut Context<Self>,
        mut body: Div,
        ttl_days: Option<i32>,
        ttl_loading: bool,
        mutating: bool,
    ) -> Div {
        body = body
            .child(
                div()
                    .font_semibold()
                    .text_sm()
                    .child("Delete my account if away for"),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("If you do not come online at least once within this period, your account will be deleted along with all messages and contacts."),
            )
            .child(div().text_sm().child(match ttl_days {
                Some(days) => format!("Currently: {}", format_account_ttl(days)),
                None => {
                    if ttl_loading {
                        "Loading…".to_string()
                    } else {
                        "No data yet.".to_string()
                    }
                }
            }));
        let months_now = ttl_days.map(|days| (days / 30) as u8);
        let active_ix = months_now.and_then(|months| {
            ACCOUNT_TTL_OPTIONS
                .iter()
                .position(|(option_months, _)| *option_months == months)
        });
        body = body.child(
            RadioGroup::horizontal("account-ttl-options")
                .selected_index(active_ix)
                .children(ACCOUNT_TTL_OPTIONS.iter().map(|(months, _)| {
                    Radio::new(format!("account-ttl-{months}m")).label(ttl_option_label(*months))
                }))
                .on_click(cx.listener(move |this, &ix: &usize, _, cx| {
                    this.submit_account_ttl(ACCOUNT_TTL_OPTIONS[ix].1, cx);
                })),
        );
        if mutating {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Working…"),
            );
        }
        body
    }

    /// Slice A9: the delete-account danger zone — TGX `DeleteMyAccount`
    /// / `DeleteMyAccountInfo` / `DeleteAccountInfo` / `DeleteAccountReason`
    /// / `DeleteAccountHelpHint`, verbatim. The password field appears
    /// only when the authoritative `passwordState` says the account has
    /// one (TGX `permanentlyDeleteAccount`).
    fn account_delete_body(
        &self,
        cx: &mut Context<Self>,
        mut body: Div,
        has_password: bool,
        pw_loading: bool,
        mutating: bool,
    ) -> Div {
        body = body
            .child(
                div()
                    .font_semibold()
                    .text_sm()
                    .text_color(danger())
                    .child("Delete my account now"),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Permanently delete the account, and all associated information from Telegram servers."),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Access to your chats will be lost forever. All existing chats will see you as Deleted Account. Using the same phone number will create a new account."),
            )
            .child(div().mt_1().font_semibold().text_sm().child("Reason"))
            .child(Textarea::new(&self.account.lifecycle.reason).aria_label("Reason for deleting account").h(px(40.)))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Tell us about any issues; after you delete the account, we won't be able to restore any data you lose in the process."),
            );
        if has_password {
            body = body
                .child(
                    div()
                        .mt_1()
                        .font_semibold()
                        .text_sm()
                        .child("Two-step verification password"),
                )
                .child(
                    Textarea::new(&self.account.lifecycle.password)
                        .aria_label("Two-step verification password")
                        .h(px(40.)),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Sent securely to Telegram."),
                );
        } else if pw_loading {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Checking two-step verification status…"),
            );
        }
        if self.account.lifecycle.confirm_delete {
            body = body.child(self.account_delete_confirm_banner(mutating, cx));
        } else {
            body = body.child(
                Button::new("account-delete")
                    .label("Permanently delete account")
                    .danger()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.account.lifecycle.confirm_delete = true;
                        cx.notify();
                    })),
            );
        }
        body
    }

    /// Slice A9: the final delete confirmation — TGX
    /// `DeleteAccountConfirmFinal` / `DeleteAccountConfirmFinalBtn`,
    /// verbatim (markdown markers stripped: the dialog renders plain
    /// text). The sessions terminate-confirm pattern.
    fn account_delete_confirm_banner(
        &self,
        mutating: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id("account-delete-confirm")
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(danger())
            .bg(danger_bg())
            .child(
                div().text_sm().text_color(danger()).child(
                    "Danger: this is the last confirmation prompt. Once you press the button below, all data will be erased from Telegram servers. Using the same phone number will create a new empty account.",
                ),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .justify_end()
                    .child(
                        Button::new("account-delete-cancel")
                            .label("Cancel")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.account.lifecycle.confirm_delete = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("account-delete-final")
                            .label(if mutating {
                                "Working…"
                            } else {
                                "Alright, delete account."
                            })
                            .danger()
                            .disabled(mutating)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.submit_delete_account(window, cx);
                            })),
                    ),
            )
    }

    /// Slice auth-logout-warning: the Log out section — it lives in the
    /// Account dialog (the account section; no new top-level section
    /// for one row). The button arms the inline confirm banner (the
    /// delete-account pattern — kit dialogs never stack, so the
    /// confirmation is inline); the banner carries the TGX `SignOutHint2`
    /// warning (`LOGOUT_WARNING_COPY`).
    fn account_logout_body(&self, cx: &mut Context<Self>, mut body: Div) -> Div {
        body = body
            .child(div().font_semibold().text_sm().child("Log out"))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Sign out of this Telegram account on this device."),
            );
        if self.account.lifecycle.confirm_logout {
            body = body.child(self.account_logout_confirm_banner(cx));
        } else {
            body = body.child(
                Button::new("account-logout")
                    .label("Log out")
                    .danger()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.account.lifecycle.confirm_logout = true;
                        cx.notify();
                    })),
            );
        }
        body
    }

    /// Slice auth-logout-warning: the logout confirmation banner — the
    /// sessions terminate-confirm / delete-account confirm pattern. The
    /// warning copy is TGX `SignOutHint2`, verified verbatim.
    fn account_logout_confirm_banner(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("account-logout-confirm")
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(danger())
            .bg(danger_bg())
            .child(
                div()
                    .text_sm()
                    .text_color(danger())
                    .child("Log out of Quill?"),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(LOGOUT_WARNING_COPY),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .justify_end()
                    .child(
                        Button::new("account-logout-cancel")
                            .label("Cancel")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.account.lifecycle.confirm_logout = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("account-logout-final")
                            .label("Log out")
                            .danger()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.submit_logout(cx);
                            })),
                    ),
            )
    }
}

crate::ui::shell::register_dialogs! {
    /// Slice A9: account lifecycle (delete account + self-destruct TTL).
    AccountLifecycle => DialogSpec::new(
        6700,
        |app| app.account.lifecycle.open,
        QuillApp::build_account_lifecycle_dialog,
    ),
}

#[cfg(test)]
mod tests {
    #[test]
    fn ttl_options_match_tgx_days() {
        assert_eq!(
            super::ACCOUNT_TTL_OPTIONS.map(|(_, days)| days),
            [31, 91, 181, 366, 546, 730]
        );
    }

    #[test]
    fn format_account_ttl_matches_tgx() {
        assert_eq!(super::format_account_ttl(1), "1 day");
        assert_eq!(super::format_account_ttl(29), "29 days");
        assert_eq!(super::format_account_ttl(31), "1 month");
        assert_eq!(super::format_account_ttl(91), "3 months");
        assert_eq!(super::format_account_ttl(180), "6 months");
        assert_eq!(super::format_account_ttl(366), "1 year");
        assert_eq!(super::format_account_ttl(730), "2 years");
    }

    #[test]
    fn logout_warning_carries_tgx_signouthint2_meaning() {
        // The checklist item is the warning copy: secret chats die,
        // downloaded media erased. Pin both sentences so a future edit
        // can't silently drop one.
        let copy = super::LOGOUT_WARNING_COPY;
        assert!(copy.contains("Secret Chats"));
        assert!(copy.contains("Downloaded media"));
        assert!(copy.contains("logging out"));
    }
}
