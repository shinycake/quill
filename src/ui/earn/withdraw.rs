//! Withdrawals (tdesktop `Settings::AddWithdrawalWidget` and
//! `Api::HandleWithdrawalButton`): the Stars amount, the Withdraw and
//! Buy Ads buttons, and the password and security-check boxes. A
//! withdrawal only asks TDLib for a Fragment URL and opens it.

use super::revenue::{learn_more_band, star_amount};
use super::*;
use crate::ui::shell::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Input, InputContentType};
use gpui_kit::prelude::FluentBuilder;
use quill::earn::*;
use quill::lang::{keys, tr, tr_with};
use quill::premium_hub::group_digits;
use std::cell::RefCell;
use std::rc::Rc;

/// tdesktop `lng_bot_earn_balance_about_url`.
const STARS_TOS_URL: &str = "https://telegram.org/tos/stars";

/// The most Stars one withdrawal may take, from the option.
fn withdraw_max(limits: &EarnLimits, available: i64) -> i64 {
    match limits.star_withdrawal_max {
        Some(max) if max > 0 => available.min(max),
        _ => available,
    }
}

/// The amount typed in the field; empty or junk reads as 0.
fn typed_amount(text: &str) -> i64 {
    text.trim().parse::<i64>().unwrap_or(0).max(0)
}

impl QuillApp {
    /// Put the withdrawable Stars in the amount field once per balance
    /// (tdesktop sets the field whenever the balance changes).
    pub(super) fn prefill_withdraw_amount(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((peer, value)) = self.earn_state().and_then(|earn| {
            let page = earn.page.as_ref()?;
            let stats = page.stars.ready()?;
            Some((
                page.peer,
                withdraw_max(&earn.limits, stats.status.available.stars.max(0)),
            ))
        }) else {
            return;
        };
        if self.earn.prefilled == Some((peer, value)) {
            return;
        }
        self.earn.prefilled = Some((peer, value));
        self.earn.amount.update(cx, |input, cx| {
            input.set_value(value.to_string(), window, cx)
        });
    }

    /// Collect or Withdraw.
    pub(super) fn earn_withdraw(&mut self, target: WithdrawTarget, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            self.connection
                .set_note(StatusNote::info(tr(keys::Q_MONEY_DEMO_MODE_THIS_NEEDS_A)));
            cx.notify();
            return;
        };
        // The password box shows the hint and "Forgot password?" from the
        // cloud password state (tdesktop `cloudPassword().reload()`).
        let _ = live.driver.fetch_password_state();
        if live.driver.start_withdrawal(target).is_err() {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_MONEY_COULDNT_WITHDRAW_RIGHT_NOW,
            )));
        }
        cx.notify();
    }

    fn earn_withdraw_stars(&mut self, owner: EarnPeer, cx: &mut Context<Self>) {
        let typed = typed_amount(&self.earn.amount.read(cx).value());
        let min = self
            .earn_state()
            .map_or(1000, |earn| earn.limits.withdrawal_min());
        if typed < min {
            // tdesktop `lng_bot_earn_credits_out_minimal`.
            self.connection.set_note(StatusNote::error(tr_with(
                keys::Q_LEFTOVERS_WITHDRAW_MINIMUM,
                &[
                    ("amount", &(group_digits(min)).to_string()),
                    ("currency", (if min == 1 { "star" } else { "stars" })),
                ],
            )));
            cx.notify();
            return;
        }
        self.earn_withdraw(
            WithdrawTarget::Stars {
                owner,
                count: typed,
            },
            cx,
        );
    }

    /// The balance, the amount field and the buttons.
    pub(super) fn stars_withdrawal(
        &self,
        session: &Session,
        page: &EarnPage,
        stats: &StarRevenueStats,
        cx: &mut Context<Self>,
    ) -> Div {
        let peer = page.peer;
        let limits = &session.payments.earn.limits;
        let available = stats.status.available;
        let enabled = stats.status.withdrawal_enabled;
        let max = withdraw_max(limits, available.stars.max(0));
        let ad_url = page.ad_account_url.clone();
        let now = quill::local_time::now_unix();
        let left = (stats.status.next_withdrawal_at - now).max(0);
        let locked = stats.status.next_withdrawal_at != 0 && left > 0;
        if locked {
            // tdesktop redraws the countdown every 250 ms.
            self.request_animation_tick(4, cx);
        }
        let typed = typed_amount(&self.earn.amount.read(cx).value());
        let label = if typed >= available.stars {
            tr(keys::BOT_EARN_BALANCE_BUTTON_ALL).to_string()
        } else {
            tr_with(
                keys::Q_MONEY_WITHDRAW,
                &[("group_digits", &(group_digits(typed)).to_string())],
            )
        };
        let mut block = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .justify_center()
                    .child(star_amount(available, 26.)),
            )
            .when(
                available != quill::premium_hub::StarAmount::default(),
                |d| {
                    d.child(div().flex().justify_center().child(about(stars_to_usd(
                        available,
                        stats.usd_rate,
                        // tdesktop: the channel page rounds to cents,
                        // the bot page keeps three digits.
                        if matches!(page.peer, EarnPeer::Chat(_)) {
                            0
                        } else {
                            3
                        },
                    ))))
                },
            );
        if enabled || ad_url.is_some() {
            let withdraw = Button::new("earn-withdraw")
                .label(if locked {
                    tr_with(
                        keys::Q_MONEY_WITHDRAW_2,
                        &[("format_time_left", &(format_time_left(left)).to_string())],
                    )
                } else {
                    label
                })
                .primary()
                .flex_1()
                .disabled(!enabled || locked)
                .on_click(cx.listener(move |this, _, _, cx| this.earn_withdraw_stars(peer, cx)));
            let mut buttons = div().flex().gap_2().child(withdraw);
            if let Some(url) = ad_url {
                buttons = buttons.child(
                    Button::new("earn-buy-ads")
                        .label(tr(keys::BOT_EARN_BALANCE_BUTTON_BUY_ADS))
                        .primary()
                        .flex_1()
                        .on_click(move |_, _, cx| cx.open_url(&url)),
                );
            }
            block = block
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(star_icon(16.))
                        .child(
                            div()
                                .flex_1()
                                .child(Input::new(&self.earn.amount).aria_label(tr_with(
                                    keys::BOT_EARN_OUT_PH_MAX,
                                    &[("amount", &max.to_string())],
                                ))),
                        ),
                )
                .child(buttons);
        }
        let is_self = matches!(peer, EarnPeer::User(id) if session.my_user_id == Some(id));
        block.child(learn_more_band(
            "earn-learn-stars",
            if is_self {
                tr(keys::Q_MONEY_YOU_CAN_WITHDRAW_FROM_10)
            } else {
                tr(keys::Q_MONEY_YOU_CAN_WITHDRAW_STARS_USING)
            },
            |_, cx| cx.open_url(STARS_TOS_URL),
            cx,
        ))
    }

    /// Submit the password box; the field is emptied as the request goes.
    fn submit_withdraw_password(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let password = self.earn.password.read(cx).value().to_string();
        let sent = self
            .live
            .as_mut()
            .map(|live| live.driver.submit_withdrawal_password(&password));
        drop(password);
        self.earn
            .password
            .update(cx, |input, cx| input.set_value("", window, cx));
        if let Some(Err(_)) = sent {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_MONEY_COULDNT_WITHDRAW_RIGHT_NOW,
            )));
        }
        cx.notify();
    }

    /// "Forgot password?" (tdesktop `PasscodeBox::recoverByEmail`): with a
    /// recovery email the code is sent at once; without one the
    /// two-step verification page offers the reset.
    fn forgot_withdraw_password(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_withdraw(window, cx);
        self.close_kit_dialog_if_done(DialogKind::EarnWithdraw, window, cx);
        let has_email = self
            .session()
            .and_then(|s| s.auth_state.password_state.as_ref())
            .is_some_and(|state| state.has_recovery_email_address);
        self.open_twofa(cx);
        self.goto_twofa_view(crate::ui::dialogs::callback_password::TwofaView::Recover);
        if has_email
            && let Some(live) = self.live.as_mut()
            && live.driver.request_password_recovery().is_err()
        {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_MONEY_COULDNT_SEND_THE_RECOVERY_CODE,
            )));
        }
        cx.notify();
    }

    fn cancel_withdraw(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.cancel_withdrawal();
        } else if let Some(earn) = self.earn_mut() {
            earn.withdraw = None;
        }
        self.earn
            .password
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    /// The password box ("Two-step verification") or the security check
    /// (tdesktop `TransferPasswordError`).
    pub(super) fn build_earn_withdraw_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::EarnWithdraw, |this, window, cx| {
                this.cancel_withdraw(window, cx);
            });
        app.update(cx, |this, cx| {
            let Some(withdraw) = this.earn_state().and_then(|e| e.withdraw.clone()) else {
                return dialog.on_close(on_close);
            };
            let muted = text_muted();
            let (title, body, footer) = match withdraw.step {
                WithdrawStep::SecurityCheck { later } => {
                    let mut body = div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .text_sm()
                        .child(tr(keys::CHANNEL_EARN_OUT_CHECK_PASSWORD_ABOUT))
                        .child(tr(keys::Q_MONEY_ENABLED_TWO_STEP_VERIFICATION_MORE))
                        .child(tr(keys::Q_MONEY_LOGGED_IN_ON_THIS_DEVICE));
                    if later {
                        body = body.child(tr(keys::RIGHTS_TRANSFER_CHECK_LATER));
                    }
                    let mut footer = div().flex().justify_end().gap_2();
                    if later {
                        footer = footer.child(
                            Button::new("earn-check-ok")
                                .label(tr(keys::BOX_OK))
                                .primary()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.cancel_withdraw(window, cx);
                                    this.close_kit_dialog_if_done(
                                        DialogKind::EarnWithdraw,
                                        window,
                                        cx,
                                    );
                                })),
                        );
                    } else {
                        footer = footer
                            .child(
                                Button::new("earn-check-cancel")
                                    .label(tr(keys::CANCEL))
                                    .ghost()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.cancel_withdraw(window, cx);
                                        this.close_kit_dialog_if_done(
                                            DialogKind::EarnWithdraw,
                                            window,
                                            cx,
                                        );
                                    })),
                            )
                            .child(
                                Button::new("earn-check-set")
                                    .label(tr(keys::Q_MONEY_SET_PASSWORD))
                                    .primary()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.cancel_withdraw(window, cx);
                                        this.close_kit_dialog_if_done(
                                            DialogKind::EarnWithdraw,
                                            window,
                                            cx,
                                        );
                                        this.open_twofa(cx);
                                    })),
                            );
                    }
                    (tr(keys::Q_MONEY_SECURITY_CHECK), body, footer)
                }
                WithdrawStep::Password { sending, error } => {
                    let description = match withdraw.target {
                        WithdrawTarget::Ton { .. } | WithdrawTarget::Stars { .. } => {
                            tr(keys::BOT_EARN_BALANCE_PASSWORD_DESCRIPTION)
                        }
                    };
                    let hint = this
                        .session()
                        .and_then(|s| s.auth_state.password_state.as_ref())
                        .map(|state| state.password_hint.clone())
                        .filter(|hint| !hint.is_empty());
                    // tdesktop `PasscodeBox` in check mode: the field, the
                    // error or the hint under it, "Forgot password?", then
                    // the description.
                    let under = match (error, hint) {
                        (Some(error), _) => {
                            Some(div().text_xs().text_color(cx.theme().danger).child(error))
                        }
                        (None, Some(hint)) => Some(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .truncate()
                                .child(tr_with(keys::Q_MONEY_HINT, &[("hint", &hint.to_string())])),
                        ),
                        (None, None) => None,
                    };
                    let body = div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            Input::new(&this.earn.password)
                                .aria_label(tr(keys::Q_MONEY_ENTER_CURRENT_PASSWORD))
                                .content_type(InputContentType::Password),
                        )
                        .children(under)
                        .child(
                            div().child(
                                Button::new("earn-password-forgot")
                                    .debug_selector(|| "earn-password-forgot".into())
                                    .label(tr(keys::Q_MONEY_FORGOT_PASSWORD))
                                    .ghost()
                                    .small()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.forgot_withdraw_password(window, cx);
                                    })),
                            ),
                        )
                        .child(div().text_sm().text_color(muted).child(description));
                    let footer = div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("earn-password-cancel")
                                .label(tr(keys::CANCEL))
                                .ghost()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.cancel_withdraw(window, cx);
                                    this.close_kit_dialog_if_done(
                                        DialogKind::EarnWithdraw,
                                        window,
                                        cx,
                                    );
                                })),
                        )
                        .child(
                            Button::new("earn-password-submit")
                                .label(tr(keys::Q_MONEY_SUBMIT))
                                .primary()
                                .loading(sending)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.submit_withdraw_password(window, cx);
                                })),
                        );
                    (tr(keys::SETTINGS_CLOUD_PASSWORD_START_TITLE), body, footer)
                }
                WithdrawStep::Checking => {
                    return dialog.on_close(on_close);
                }
            };
            let body = Rc::new(RefCell::new(Some(body.into_any_element())));
            dialog
                .overlay(true)
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

crate::ui::shell::register_dialogs! {
    /// The withdrawal's password box or security check.
    EarnWithdraw => DialogSpec::new(
        7612,
        |app| {
            app.earn_state()
                .and_then(|e| e.withdraw.as_ref())
                .is_some_and(|w| w.step != WithdrawStep::Checking)
        },
        QuillApp::build_earn_withdraw_dialog,
    ),
}

#[cfg(test)]
mod unit {
    use super::{typed_amount, withdraw_max};
    use quill::earn::EarnLimits;

    #[test]
    fn the_field_takes_digits_and_the_option_caps_it() {
        assert_eq!(typed_amount(" 1500 "), 1500);
        assert_eq!(typed_amount("abc"), 0);
        assert_eq!(typed_amount("-5"), 0);
        let limits = EarnLimits {
            star_withdrawal_max: Some(1000),
            ..EarnLimits::default()
        };
        assert_eq!(withdraw_max(&limits, 5000), 1000);
        assert_eq!(withdraw_max(&EarnLimits::default(), 5000), 5000);
    }
}
