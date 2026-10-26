//! Collect in the Gram withdrawal section (tdesktop
//! `BuildCurrencyWithdrawalSection` → `Api::HandleWithdrawalButton`): the
//! "Two-step verification" box, or the security check, over the Grams
//! screen; Submit asks `getGramWithdrawalUrl` for Fragment's page, which
//! opens in the browser through the link policy.
//!
//! Only Submit sends a withdrawal request. The password is read from the
//! masked field once, handed to that request and the field is emptied at
//! once; it is never stored. The box itself is the shared
//! `fragment_password` one the gift export uses.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::fragment_password::{PasswordBox, SecurityCheck, keep_password_field};
use crate::ui::shell::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::gift_manage::ExportError;
use quill::lang::{keys, tr};
use quill::premium_hub::ton::GramWithdraw;
use std::cell::RefCell;
use std::rc::Rc;

crate::ui::shell::register_dialogs! {
    /// Collect's password box or security check, above the Grams screen.
    GramsWithdraw => DialogSpec::new(
        3804,
        |app| {
            app.session().is_some_and(|s| {
                s.payments.hub.grams.open && s.payments.hub.grams.withdraw.is_some()
            })
        },
        QuillApp::build_grams_withdraw_dialog,
    )
    .closes(|app, window, cx| app.cancel_grams_withdraw(window, cx)),
}

/// The Collect button under the available amount (tdesktop
/// `creditsSettingsBigBalanceButton` with `lng_channel_earn_balance_button`;
/// always enabled, the server judges the amount).
pub(super) fn collect_button(cx: &mut Context<QuillApp>) -> Button {
    Button::new("grams-withdraw")
        .debug_selector(|| "grams-withdraw".to_string())
        .label(tr(keys::CHANNEL_EARN_BALANCE_BUTTON))
        .primary()
        .w_full()
        .on_click(cx.listener(|this, _, _, cx| this.start_grams_withdraw(cx)))
}

impl QuillApp {
    fn grams_withdraw(&self) -> Option<GramWithdraw> {
        self.session()?.payments.hub.grams.withdraw.clone()
    }

    fn clear_grams_withdraw_password(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.payments
            .grams_withdraw_password
            .update(cx, |input, cx| input.set_value("", window, cx));
    }

    /// Collect: the password box (or the security check). Nothing is
    /// requested from Fragment until the password is submitted.
    pub(in crate::ui) fn start_grams_withdraw(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if live.driver.start_gram_withdrawal().is_err() {
                self.connection
                    .set_note(StatusNote::error(tr(keys::Q_MONEY_COULD_NOT_OPEN_FRAGMENT)));
            }
        } else if let Some(grams) = self.grams_mut() {
            // The demo has no password state: the password box.
            grams.withdraw = Some(GramWithdraw::open(None));
        }
        cx.notify();
    }

    /// Submit in the password box. The field is read once and emptied as
    /// the request goes out.
    pub(in crate::ui) fn submit_grams_withdraw_password(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = self.payments.grams_withdraw_password.clone();
        // An empty field does nothing (tdesktop focuses it again).
        if input.read(cx).value().is_empty() {
            return;
        }
        let password = input.read(cx).value().to_string();
        let sent = self
            .live
            .as_mut()
            .map(|live| live.driver.submit_gram_withdrawal(&password));
        drop(password);
        input.update(cx, |input, cx| input.set_value("", window, cx));
        match sent {
            Some(Ok(_)) => {}
            // Refused before sending: say so under the field.
            Some(Err(_)) => {
                if let Some(withdraw) = self.grams_mut().and_then(|g| g.withdraw.as_mut())
                    && withdraw.can_submit()
                {
                    *withdraw = GramWithdraw::idle(Some(ExportError::Other(String::new())));
                }
            }
            None => self
                .connection
                .set_note(StatusNote::info(tr(keys::Q_MONEY_DEMO_MODE_THIS_NEEDS_A))),
        }
        cx.notify();
    }

    /// Cancel, OK or Escape: close the box and empty the field.
    pub(in crate::ui) fn cancel_grams_withdraw(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            live.driver.cancel_gram_withdrawal();
        } else if let Some(grams) = self.grams_mut() {
            grams.withdraw = None;
        }
        self.clear_grams_withdraw_password(window, cx);
        cx.notify();
    }

    fn forgot_grams_withdraw_password(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_grams_withdraw(window, cx);
        self.recover_cloud_password(cx);
    }

    fn set_password_for_grams_withdraw(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_grams_withdraw(window, cx);
        self.open_twofa(cx);
    }

    /// Per frame: open Fragment's withdrawal page, show the "too small"
    /// toast, and keep the password field focused while the box waits and
    /// empty whenever it is not on screen.
    pub(in crate::ui) fn grams_withdraw_frame_upkeep(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (url, too_small, showing, idle) = match self.grams_mut() {
            Some(grams) => {
                let showing =
                    grams.open && matches!(grams.withdraw, Some(GramWithdraw::Password { .. }));
                let idle = grams.open
                    && grams
                        .withdraw
                        .as_ref()
                        .is_some_and(GramWithdraw::can_submit);
                (
                    grams.withdraw_url.take(),
                    std::mem::take(&mut grams.withdraw_too_small),
                    showing,
                    idle,
                )
            }
            None => (None, false, false, false),
        };
        keep_password_field(
            &self.payments.grams_withdraw_password,
            &mut self.payments.grams_withdraw_password_focused,
            showing,
            idle,
            window,
            cx,
        );
        if let Some(url) = url {
            self.open_fragment_url(&url, cx);
            cx.notify();
        }
        if too_small {
            // tdesktop `showToast(lng_channel_earn_out_amount_too_small)`.
            cx.defer_in(window, |_, window, cx| {
                window.push_notification(
                    Notification::new().message(tr(keys::CHANNEL_EARN_OUT_AMOUNT_TOO_SMALL)),
                    cx,
                );
            });
            cx.notify();
        }
    }

    pub(in crate::ui) fn build_grams_withdraw_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::GramsWithdraw, |this, window, cx| {
                this.cancel_grams_withdraw(window, cx);
            });
        app.update(cx, |this, cx| {
            let dialog = dialog.overlay(true).w(px(420.));
            let (Some(session), Some(withdraw)) = (this.session(), this.grams_withdraw()) else {
                return dialog.on_close(on_close);
            };
            let (title, (body, footer)) = match withdraw {
                GramWithdraw::Password { request, error } => (
                    tr(keys::BOT_EARN_BALANCE_PASSWORD_TITLE),
                    PasswordBox {
                        id: "grams-withdraw",
                        input: &this.payments.grams_withdraw_password,
                        description: tr(keys::BOT_EARN_BALANCE_PASSWORD_DESCRIPTION),
                        sending: request.is_some(),
                        error: error.as_ref(),
                        on_submit: QuillApp::submit_grams_withdraw_password,
                        on_cancel: QuillApp::cancel_grams_withdraw,
                        on_forgot: QuillApp::forgot_grams_withdraw_password,
                    }
                    .render(session, cx),
                ),
                GramWithdraw::SecurityCheck { later } => (
                    tr(keys::Q_MONEY_SECURITY_CHECK),
                    SecurityCheck {
                        id: "grams-withdraw",
                        about: tr(keys::CHANNEL_EARN_OUT_CHECK_PASSWORD_ABOUT),
                        later,
                        on_close: QuillApp::cancel_grams_withdraw,
                        on_set_password: QuillApp::set_password_for_grams_withdraw,
                    }
                    .render(cx),
                ),
            };
            let body = Rc::new(RefCell::new(Some(body)));
            dialog
                .title(crate::ui::shell::dialog_title(title))
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    },
                ))
                .footer(footer)
                .on_close(on_close)
        })
    }
}
