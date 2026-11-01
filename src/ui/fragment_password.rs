//! The 2-step password box and the security check in front of a Fragment
//! page (tdesktop `PasscodeBox` in check mode with a custom title,
//! description and Submit, and `PrePasswordErrorBox`), shared by the gift
//! export (`gift_transfer_view`) and the Gram withdrawal
//! (`premium_ui::grams_withdraw`), plus opening the Fragment page the
//! request returns.
//!
//! The password lives only in the masked field. Callers read it once on
//! Submit, hand it to the one request and empty the field at once
//! (`set_value` also drops the undo history); [`keep_password_field`]
//! empties it whenever the box is not on screen.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::entity_links::OpenLinkConfirm;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::input::{Input, InputContentType, InputState};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::gift_manage::ExportError;
use quill::lang::{keys, tr, tr_with};
use quill::link_policy::{OpenDecision, open_decision};
use quill::state::Session;
use quill::text::LinkTarget;

/// A button of a box: what it does to the app.
pub(super) type BoxAction = fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>);

fn footer() -> Div {
    div().flex().justify_end().gap_2()
}

/// tdesktop `PasscodeBox` in check mode: the field, the error or the hint
/// under it, "Forgot password?" and the description; Cancel and Submit.
pub(super) struct PasswordBox<'a> {
    /// Prefix of the element ids (`{id}-password-submit`, ...).
    pub(super) id: &'static str,
    pub(super) input: &'a Entity<InputState>,
    pub(super) description: &'static str,
    pub(super) sending: bool,
    pub(super) error: Option<&'a ExportError>,
    pub(super) on_submit: BoxAction,
    pub(super) on_cancel: BoxAction,
    pub(super) on_forgot: BoxAction,
}

impl PasswordBox<'_> {
    /// The body and the footer.
    pub(super) fn render(
        self,
        session: &Session,
        cx: &mut Context<QuillApp>,
    ) -> (AnyElement, AnyElement) {
        let Self {
            id,
            input,
            description,
            sending,
            error,
            on_submit,
            on_cancel,
            on_forgot,
        } = self;
        let muted = cx.theme().muted_foreground;
        let state = session.auth_state.password_state.as_ref();
        let hint = state
            .map(|state| state.password_hint.clone())
            .filter(|hint| !hint.is_empty());
        // tdesktop `_showRecoverLink`: a recovery email, or no reset pending.
        let show_forgot = state
            .is_none_or(|state| state.has_recovery_email_address || state.pending_reset_date == 0);
        let under = match (error, hint) {
            (Some(error), _) => Some(
                div()
                    .text_xs()
                    .text_color(cx.theme().danger)
                    .child(export_error_text(error)),
            ),
            (None, Some(hint)) => Some(
                div()
                    .text_xs()
                    .text_color(muted)
                    .truncate()
                    .child(tr_with(keys::Q_MONEY_HINT, &[("hint", &hint)])),
            ),
            (None, None) => None,
        };
        let body = v_flex()
            .gap_2()
            .child(
                Input::new(input)
                    .aria_label(tr(keys::Q_MONEY_ENTER_CURRENT_PASSWORD))
                    .content_type(InputContentType::Password)
                    .disabled(sending),
            )
            .children(under)
            .when(show_forgot, |body| {
                body.child(
                    div().child(
                        Button::new(format!("{id}-password-forgot"))
                            .label(tr(keys::Q_MONEY_FORGOT_PASSWORD))
                            .ghost()
                            .small()
                            .disabled(sending)
                            .on_click(
                                cx.listener(move |this, _, window, cx| on_forgot(this, window, cx)),
                            ),
                    ),
                )
            })
            .child(div().text_sm().text_color(muted).child(description));
        let footer = footer()
            .child(
                Button::new(format!("{id}-password-cancel"))
                    .label(tr(keys::CANCEL))
                    .ghost()
                    .disabled(sending)
                    .on_click(cx.listener(move |this, _, window, cx| on_cancel(this, window, cx))),
            )
            .child(
                Button::new(format!("{id}-password-submit"))
                    .label(tr(keys::PASSCODE_SUBMIT))
                    .primary()
                    .loading(sending)
                    .disabled(sending)
                    .on_click(cx.listener(move |this, _, window, cx| on_submit(this, window, cx))),
            );
        (body.into_any_element(), footer.into_any_element())
    }
}

/// tdesktop `PrePasswordErrorBox` (`TransferPasswordError`): what the
/// action needs, and "Set password" when there is none, or OK when it is
/// only too fresh.
pub(super) struct SecurityCheck {
    pub(super) id: &'static str,
    /// The first line: what the action needs ("You can withdraw only if
    /// you have:").
    pub(super) about: &'static str,
    pub(super) later: bool,
    pub(super) on_close: BoxAction,
    pub(super) on_set_password: BoxAction,
}

impl SecurityCheck {
    /// The body and the footer.
    pub(super) fn render(self, cx: &mut Context<QuillApp>) -> (AnyElement, AnyElement) {
        let Self {
            id,
            about,
            later,
            on_close,
            on_set_password,
        } = self;
        let mut body = v_flex()
            .gap_2()
            .text_sm()
            .child(about)
            .child(tr(keys::Q_MONEY_ENABLED_TWO_STEP_VERIFICATION_MORE))
            .child(tr(keys::Q_MONEY_LOGGED_IN_ON_THIS_DEVICE));
        if later {
            body = body.child(tr(keys::RIGHTS_TRANSFER_CHECK_LATER));
        }
        let footer = if later {
            footer().child(
                Button::new(format!("{id}-check-ok"))
                    .label(tr(keys::BOX_OK))
                    .primary()
                    .on_click(cx.listener(move |this, _, window, cx| on_close(this, window, cx))),
            )
        } else {
            footer()
                .child(
                    Button::new(format!("{id}-check-cancel"))
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(
                            cx.listener(move |this, _, window, cx| on_close(this, window, cx)),
                        ),
                )
                .child(
                    Button::new(format!("{id}-check-set"))
                        .label(tr(keys::RIGHTS_TRANSFER_SET_PASSWORD))
                        .primary()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            on_set_password(this, window, cx)
                        })),
                )
        };
        (body.into_any_element(), footer.into_any_element())
    }
}

/// The line under the password field for a failed try.
pub(super) fn export_error_text(error: &ExportError) -> SharedString {
    match error {
        ExportError::WrongPassword => tr(keys::CLOUD_PASSWORD_WRONG).into(),
        ExportError::Flood => tr(keys::FLOOD_ERROR).into(),
        ExportError::ServerError => tr(keys::Q_MONEY_SERVER_ERROR).into(),
        ExportError::Other(reason) if reason.is_empty() => {
            tr(keys::Q_MONEY_COULDNT_OPEN_FRAGMENT).into()
        }
        ExportError::Other(reason) => tr_with(
            keys::Q_MONEY_COULDNT_OPEN_FRAGMENT_REASON,
            &[("reason", reason)],
        )
        .into(),
    }
}

/// Per frame: the field takes the keyboard when the idle box opens and
/// after each answer (tdesktop `PasscodeBox` focuses the field), and is
/// emptied whenever the box is not on screen.
pub(super) fn keep_password_field(
    input: &Entity<InputState>,
    focused: &mut bool,
    showing: bool,
    idle: bool,
    window: &mut Window,
    cx: &mut App,
) {
    if idle && !*focused {
        *focused = true;
        input.update(cx, |input, cx| input.focus(window, cx));
    } else if !idle {
        *focused = false;
    }
    if !showing && !input.read(cx).value().is_empty() {
        input.update(cx, |input, cx| input.set_value("", window, cx));
    }
}

impl QuillApp {
    /// "Forgot password?" in a password box (tdesktop
    /// `PasscodeBox::recoverByEmail`): the two-step verification page's
    /// recovery; with a recovery email the code is sent at once. The
    /// caller closes its box first.
    pub(super) fn recover_cloud_password(&mut self, cx: &mut Context<Self>) {
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

    /// Open a Fragment page in the browser through the link policy: an
    /// https address with nothing suspicious opens at once; a look-alike
    /// address goes to the "Open this link?" box (`entity_links`), which
    /// opens it only if the user confirms. Anything but https is refused.
    pub(super) fn open_fragment_url(&mut self, url: &str, cx: &mut Context<Self>) {
        if !url.starts_with("https://") {
            self.connection
                .set_note(StatusNote::error(tr(keys::Q_MONEY_COULDNT_LOAD_THE_PAGE)));
            return;
        }
        let link = LinkTarget::Url {
            url: url.to_string(),
            label: None,
        };
        match open_decision(&link) {
            OpenDecision::Open => self.open_link(url, cx),
            OpenDecision::Confirm {
                url,
                shown,
                suspicious,
            } => {
                self.message_ui.open_link_confirm = Some(OpenLinkConfirm {
                    url,
                    shown,
                    suspicious,
                    from_poll_option: false,
                });
                cx.notify();
            }
        }
    }
}
