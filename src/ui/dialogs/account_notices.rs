//! Batch 4: what the server tells the user about the account — the
//! new-login alert (`updateUnconfirmedSession`, tdesktop's
//! "Someone just got access to your messages!" suggestion and its
//! "New Login Prevented" box), service notification popups
//! (`updateServiceNotification`) and the terms of service prompt
//! (`updateTermsOfService`, tdesktop `TermsBox`).

use super::super::app::QuillApp;
use super::super::shell::{DialogKind, QuillShell};
use super::super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with};
use quill::state::{LoginReview, prevented_login_message, unconfirmed_login_message};
use std::cell::RefCell;
use std::rc::Rc;

/// Where the terms prompt is (tdesktop: the box, its "sorry" confirm and
/// the final "delete" warning).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TermsStep {
    #[default]
    Terms,
    DeclineSorry,
    DeleteWarning,
}

/// Which notice the dialog shows now, most important first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AccountNotice {
    Terms,
    Service,
    LoginPrevented,
    Frozen,
    AgeVerify,
}

fn terms_sorry() -> &'static str {
    tr(keys::TERMS_UPDATE_SORRY)
}
fn terms_delete_warning() -> &'static str {
    tr(keys::Q_LEFTOVERS_TERMS_DELETE_WARNING)
}
fn prevented_warning() -> &'static str {
    tr(keys::UNCONFIRMED_AUTH_DENIED_WARNING)
}

impl QuillApp {
    /// The notice to show, if any.
    pub(crate) fn account_notice(&self) -> Option<AccountNotice> {
        let session = self.session()?;
        if session.settings.notices.terms.is_some() {
            Some(AccountNotice::Terms)
        } else if !session.settings.notices.service.is_empty() {
            Some(AccountNotice::Service)
        } else if self.auth_ui.login_prevented.is_some() {
            Some(AccountNotice::LoginPrevented)
        } else if self.account.freeze_info_open && session.sync.freeze.is_some() {
            Some(AccountNotice::Frozen)
        } else if self.account.age_verify_open && session.sync.age_verification.is_some() {
            Some(AccountNotice::AgeVerify)
        } else {
            None
        }
    }

    /// Take the finished new-login review out of the reducer: "Yes" is a
    /// toast, "No" a box (tdesktop `ShowAuthToast`).
    pub(crate) fn drain_account_notices(&mut self) {
        let outcome = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.settings.notices.review_outcome.take());
        match outcome {
            Some(LoginReview::Allowed) => {
                self.connection.set_note(StatusNote::silent(tr(
                    keys::Q_OPS_NEW_LOGIN_ALLOWED_YOU_CAN,
                )));
            }
            Some(LoginReview::Prevented { places }) => self.auth_ui.login_prevented = Some(places),
            None => {}
        }
    }

    /// "Someone just got access to your messages!" strip with the two
    /// answers, above the chat list (tdesktop's top-bar suggestion).
    pub(crate) fn unconfirmed_login_banner(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let notices = &self.session()?.settings.notices;
        if notices.unconfirmed_count == 0 || notices.unconfirmed_entries.is_empty() {
            return None;
        }
        self.unconfirmed_login_banner_body(cx)
    }

    /// `unconfirmed_login_banner` past its cheap check. A debug build reserves this
    /// builder's whole frame on entry, so frames that show nothing skip
    /// it (docs/decisions/main-thread-stack.md).
    /// The outer function repeats this body's first check; keep the two in step.
    #[inline(never)]
    fn unconfirmed_login_banner_body(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let notices = &session.settings.notices;
        if notices.unconfirmed_count == 0 || notices.unconfirmed_entries.is_empty() {
            return None;
        }
        let message = unconfirmed_login_message(&notices.unconfirmed_entries);
        let busy = notices.review_pending > 0;
        let error = notices.review_error.clone();
        Some(
            div()
                .id("unconfirmed-login-banner")
                .role(Role::Group)
                .aria_label(tr(keys::Q_OPS_NEW_LOGIN))
                .w_full()
                .flex_none()
                .flex()
                .flex_col()
                .items_center()
                .gap_2()
                .px_3()
                .py_3()
                .border_b_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().warning.opacity(0.1))
                .child(
                    div()
                        .font_semibold()
                        .text_sm()
                        .child(tr(keys::UNCONFIRMED_AUTH_TITLE)),
                )
                .child(div().text_sm().text_center().max_w(px(520.)).child(message))
                .when_some(error, |this, error| {
                    this.child(div().text_xs().text_color(danger()).child(error))
                })
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("unconfirmed-login-yes")
                                .label(tr(keys::UNCONFIRMED_AUTH_CONFIRM))
                                .primary()
                                .disabled(busy)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.review_new_login(true, cx);
                                })),
                        )
                        .child(
                            Button::new("unconfirmed-login-no")
                                .label(tr(keys::UNCONFIRMED_AUTH_DENY))
                                .outline()
                                .disabled(busy)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.review_new_login(false, cx);
                                })),
                        ),
                )
                .into_any_element(),
        )
    }

    fn review_new_login(&mut self, confirmed: bool, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.review_unconfirmed_sessions(confirmed);
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.settings.notices.unconfirmed_count = 0;
            demo.settings.notices.unconfirmed_entries.clear();
            if !confirmed {
                self.auth_ui.login_prevented =
                    Some(vec![tr(keys::Q_OPS_BERLIN_GERMANY_PIXEL_9).into()]);
            }
        }
        cx.notify();
    }

    /// One dialog for the three notices; the front one is shown.
    pub(crate) fn build_account_notice_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::AccountNotice, |this, _, cx| {
                // Esc / backdrop: the dismissible notices only.
                match this.account_notice() {
                    Some(AccountNotice::Service) => {
                        if let Some(live) = this.live.as_mut() {
                            live.driver.session.dismiss_service_notice();
                        } else if let Some(demo) = this.demo_session.as_mut() {
                            demo.dismiss_service_notice();
                        }
                    }
                    Some(AccountNotice::LoginPrevented) => this.auth_ui.login_prevented = None,
                    Some(AccountNotice::Frozen) => this.account.freeze_info_open = false,
                    Some(AccountNotice::AgeVerify) => this.account.age_verify_open = false,
                    _ => {}
                }
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let notice = this.account_notice();
            let (title, body, footer, locked) = match notice {
                Some(AccountNotice::Terms) => this.terms_dialog_parts(cx),
                Some(AccountNotice::Service) => this.service_dialog_parts(cx),
                Some(AccountNotice::LoginPrevented) => this.prevented_dialog_parts(cx),
                Some(AccountNotice::Frozen) => this.frozen_dialog_parts(cx),
                Some(AccountNotice::AgeVerify) => this.age_verify_dialog_parts(cx),
                None => (
                    "".into(),
                    div().into_any_element(),
                    div().into_any_element(),
                    false,
                ),
            };
            let body = Rc::new(RefCell::new(Some(body)));
            dialog
                .overlay(true)
                .when(locked, |d| {
                    d.close_button(false)
                        .overlay_closable(false)
                        .keyboard(false)
                })
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

    fn terms_dialog_parts(
        &self,
        cx: &mut Context<Self>,
    ) -> (SharedString, AnyElement, AnyElement, bool) {
        let session = self.session();
        let terms = session
            .as_ref()
            .and_then(|s| s.settings.notices.terms.clone());
        let Some(terms) = terms else {
            return (
                "".into(),
                div().into_any_element(),
                div().into_any_element(),
                true,
            );
        };
        let in_flight = session
            .as_ref()
            .is_some_and(|s| s.settings.notices.terms_in_flight);
        let error = session
            .as_ref()
            .and_then(|s| s.settings.notices.terms_error.clone());
        let busy_delete = session
            .as_ref()
            .is_some_and(|s| s.settings.account_mutating);
        let account_error = session
            .as_ref()
            .and_then(|s| s.settings.account_error.clone());
        match self.auth_ui.terms_step {
            TermsStep::Terms => {
                let min_age = terms.min_user_age;
                let mut body = div().flex().flex_col().gap_3().child(
                    div()
                        .id("terms-text")
                        .text_sm()
                        .max_h(px(320.))
                        .overflow_y_scroll()
                        .child(terms.text.clone()),
                );
                if min_age > 0 {
                    body = body.child(
                        Checkbox::new("terms-age")
                            .label(tr_with(
                                keys::Q_OPS_I_CONFIRM_THAT_I_AM,
                                &[("min_age", &min_age.to_string())],
                            ))
                            .checked(self.auth_ui.terms_age_ok)
                            .on_click(cx.listener(|this, &on: &bool, _, cx| {
                                this.auth_ui.terms_age_ok = on;
                                this.auth_ui.terms_age_error = false;
                                cx.notify();
                            })),
                    );
                    if self.auth_ui.terms_age_error {
                        body = body.child(
                            div()
                                .text_xs()
                                .text_color(danger())
                                .child(tr(keys::Q_OPS_CONFIRM_YOUR_AGE_TO_CONTINUE)),
                        );
                    }
                }
                if let Some(error) = error {
                    body = body.child(div().text_xs().text_color(danger()).child(error));
                }
                let footer = div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("terms-decline")
                            .label(tr(keys::CALL_DECLINE))
                            .ghost()
                            .disabled(in_flight)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.auth_ui.terms_step = TermsStep::DeclineSorry;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("terms-agree")
                            .label(if in_flight {
                                tr(keys::Q_OPS_WORKING)
                            } else {
                                tr(keys::Q_OPS_AGREE_CONTINUE)
                            })
                            .primary()
                            .disabled(in_flight)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if min_age > 0 && !this.auth_ui.terms_age_ok {
                                    this.auth_ui.terms_age_error = true;
                                } else if let Some(live) = this.live.as_mut() {
                                    let _ = live.driver.accept_terms();
                                } else if let Some(demo) = this.demo_session.as_mut() {
                                    demo.settings.notices.terms = None;
                                }
                                cx.notify();
                            })),
                    );
                (
                    tr(keys::Q_OPS_TERMS_OF_SERVICE).into(),
                    body.into_any_element(),
                    footer.into_any_element(),
                    true,
                )
            }
            TermsStep::DeclineSorry => {
                let body = div().text_sm().child(terms_sorry());
                let footer = div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("terms-sorry-back")
                            .label(tr(keys::Q_OPS_BACK))
                            .primary()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.auth_ui.terms_step = TermsStep::Terms;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("terms-sorry-delete")
                            .label(tr(keys::Q_OPS_DECLINE_DELETE))
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.auth_ui.terms_step = TermsStep::DeleteWarning;
                                cx.notify();
                            })),
                    );
                (
                    tr(keys::Q_OPS_TERMS_OF_SERVICE).into(),
                    body.into_any_element(),
                    footer.into_any_element(),
                    true,
                )
            }
            TermsStep::DeleteWarning => {
                let mut body = div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().text_sm().child(terms_delete_warning()));
                if let Some(error) = account_error {
                    body = body.child(div().text_xs().text_color(danger()).child(error));
                }
                let footer = div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("terms-delete-back")
                            .label(tr(keys::Q_OPS_BACK))
                            .primary()
                            .disabled(busy_delete)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.auth_ui.terms_step = TermsStep::DeclineSorry;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("terms-delete-now")
                            .label(if busy_delete {
                                tr(keys::Q_OPS_DELETING)
                            } else {
                                tr(keys::Q_OPS_DELETE_NOW)
                            })
                            .danger()
                            .disabled(busy_delete)
                            .on_click(cx.listener(|this, _, _, cx| {
                                // TDLib: declined terms must call
                                // `deleteAccount` with this reason.
                                if let Some(live) = this.live.as_mut() {
                                    let _ = live
                                        .driver
                                        .delete_account(tr(keys::Q_OPS_DECLINE_TOS_UPDATE), "");
                                }
                                cx.notify();
                            })),
                    );
                (
                    tr(keys::Q_OPS_TERMS_OF_SERVICE).into(),
                    body.into_any_element(),
                    footer.into_any_element(),
                    true,
                )
            }
        }
    }

    fn service_dialog_parts(
        &self,
        cx: &mut Context<Self>,
    ) -> (SharedString, AnyElement, AnyElement, bool) {
        let notice = self
            .session()
            .and_then(|s| s.settings.notices.service.front().cloned());
        let Some(notice) = notice else {
            return (
                "".into(),
                div().into_any_element(),
                div().into_any_element(),
                false,
            );
        };
        let force_logout = notice.is_force_logout();
        let body = div()
            .id("service-notice-text")
            .text_sm()
            .child(notice.text.clone());
        let footer = if force_logout {
            div().flex().justify_end().child(
                Button::new("service-notice-logout")
                    .label(tr(keys::Q_OPS_LOG_OUT))
                    .primary()
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(live) = this.live.as_mut() {
                            live.driver.session.dismiss_service_notice();
                            let _ = live.driver.request_logout();
                        }
                        cx.notify();
                    })),
            )
        } else {
            div().flex().justify_end().child(
                Button::new("service-notice-ok")
                    .label(tr(keys::BOX_OK))
                    .primary()
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(live) = this.live.as_mut() {
                            live.driver.session.dismiss_service_notice();
                        } else if let Some(demo) = this.demo_session.as_mut() {
                            demo.dismiss_service_notice();
                        }
                        cx.notify();
                    })),
            )
        };
        (
            tr(keys::Q_OPS_TELEGRAM).into(),
            body.into_any_element(),
            footer.into_any_element(),
            force_logout,
        )
    }

    fn prevented_dialog_parts(
        &self,
        cx: &mut Context<Self>,
    ) -> (SharedString, AnyElement, AnyElement, bool) {
        let places = self.auth_ui.login_prevented.clone().unwrap_or_default();
        let title = if places.len() == 1 {
            tr(keys::Q_OPS_NEW_LOGIN_PREVENTED)
        } else {
            tr(keys::Q_OPS_NEW_LOGINS_PREVENTED)
        };
        let body = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_sm().child(prevented_login_message(&places)))
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(danger())
                    .child(prevented_warning()),
            );
        let footer = div().flex().justify_end().child(
            Button::new("login-prevented-ok")
                .label(tr(keys::BOX_OK))
                .primary()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.auth_ui.login_prevented = None;
                    cx.notify();
                })),
        );
        (
            title.into(),
            body.into_any_element(),
            footer.into_any_element(),
            false,
        )
    }
}

crate::ui::shell::register_dialogs! {
    /// Batch 4: terms of service, server service popups and the
    /// "New Login Prevented" follow-up.
    AccountNotice => DialogSpec::new(
        // Batch 4: what the server says about the account comes first.
        100,
        |app| app.account_notice().is_some(),
        QuillApp::build_account_notice_dialog,
    ),
}
