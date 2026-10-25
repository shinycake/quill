//! Methods moved out of `security.rs` to keep files under 1000 lines.

use super::*;
use crate::ui::status_note::StatusNote;
use quill::lang::{keys, tr, tr_with};

impl QuillApp {
    /// Slice A2: the status screen — current state plus the pending
    /// confirmation card (TGX `PendingEmailText`) when a recovery email
    /// is awaiting confirmation.
    pub(in crate::ui) fn twofa_status_body(
        &self,
        cx: &mut Context<Self>,
        mut body: Div,
        state: &PasswordState,
        loading: bool,
    ) -> Div {
        body = body.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .font_semibold()
                        .text_sm()
                        .child(tr(keys::PROXY_BOX_STATUS)),
                )
                .child(div().text_sm().child(if state.has_password {
                    tr(keys::SETTINGS_CLOUD_PASSWORD_ON)
                } else {
                    tr(keys::SETTINGS_CLOUD_PASSWORD_OFF)
                })),
        );
        if state.has_password && !state.password_hint.is_empty() {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr_with(
                        keys::Q_PREFS_HINT,
                        &[("password_hint", &state.password_hint.to_string())],
                    )),
            );
        }
        // B13: the server asked whether the user still remembers the
        // password (`suggestedActionCheckPassword`).
        if state.has_password
            && self
                .session()
                .is_some_and(|s| s.settings.privacy_data.check_password_suggested)
        {
            body = body.child(self.password_check_card(cx));
        }
        body = body.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .font_semibold()
                        .text_sm()
                        .child(tr(keys::SETTINGS_CLOUD_PASSWORD_EMAIL_TITLE)),
                )
                .child(div().text_sm().child(if state.has_recovery_email_address {
                    tr(keys::Q_PREFS_SET)
                } else {
                    tr(keys::Q_PREFS_NOT_SET)
                })),
        );
        if let Some(pattern) = &state.pending_email_pattern {
            body = body.child(self.twofa_pending_email_block(cx, pattern, loading));
        }
        if let Some(row) = self.twofa_login_email_row(cx, state) {
            body = body.child(row);
        }
        let mut actions = div().flex().gap_2().flex_wrap();
        if state.has_password {
            // TGX `ChangePassword`, verbatim.
            actions = actions.child(
                Button::new("twofa-goto-change")
                    .label(tr(keys::SETTINGS_CLOUD_PASSWORD_MANAGE_PASSWORD_CHANGE))
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.goto_twofa_view(TwofaView::Change);
                        cx.notify();
                    })),
            );
            actions = actions.child(
                Button::new("twofa-goto-disable")
                    .label(tr(keys::Q_PREFS_TURN_OFF))
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.goto_twofa_view(TwofaView::Disable);
                        cx.notify();
                    })),
            );
            actions = actions.child(
                Button::new("twofa-goto-email")
                    // TGX `SetRecoveryEmail` / `ChangeRecoveryEmail`.
                    .label(if state.has_recovery_email_address {
                        tr(keys::SETTINGS_CLOUD_PASSWORD_MANAGE_EMAIL_CHANGE)
                    } else {
                        tr(keys::SETTINGS_CLOUD_PASSWORD_MANAGE_EMAIL_NEW)
                    })
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.goto_twofa_view(TwofaView::Email);
                        cx.notify();
                    })),
            );
        } else {
            // TGX `SetAdditionalPassword`, verbatim.
            actions = actions.child(
                Button::new("twofa-goto-enable")
                    .label(tr(keys::Q_PREFS_SET_ADDITIONAL_PASSWORD))
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.goto_twofa_view(TwofaView::Enable);
                        cx.notify();
                    })),
            );
        }
        if loading {
            actions = actions.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::Q_PREFS_WORKING)),
            );
        }
        body.child(actions)
    }

    /// Slice A2: enable form — new password + hint + optional recovery
    /// email in one `setPassword` (the TGX `PasswordController`
    /// MODE_NEW convention).
    pub(in crate::ui) fn twofa_enable_body(&self, cx: &mut Context<Self>, body: Div) -> Div {
        body.child(
            div()
                .font_semibold()
                .text_sm()
                .child(tr(keys::Q_PREFS_SET_ADDITIONAL_PASSWORD)),
        )
        .child(
            div()
                .mt_1()
                .font_semibold()
                .text_sm()
                .child(tr(keys::Q_PREFS_NEW_PASSWORD)),
        )
        .child(
            Input::new(&self.twofa.new_password)
                .aria_label(tr(keys::Q_PREFS_NEW_TWO_STEP_VERIFICATION_PASSWORD))
                .content_type(InputContentType::Password)
                .h(px(40.)),
        )
        .child(
            div()
                .mt_1()
                .font_semibold()
                .text_sm()
                .child(tr(keys::Q_PREFS_HINT_OPTIONAL)),
        )
        .child(
            Textarea::new(&self.twofa.hint)
                .aria_label(tr(keys::SETTINGS_CLOUD_PASSWORD_HINT_TITLE))
                .h(px(40.)),
        )
        .child(
            div()
                .mt_1()
                .font_semibold()
                .text_sm()
                .child(tr(keys::Q_PREFS_RECOVERY_EMAIL_OPTIONAL)),
        )
        .child(
            Textarea::new(&self.twofa.email)
                .aria_label(tr(keys::Q_PREFS_RECOVERY_EMAIL_ADDRESS))
                .h(px(40.)),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(tr(keys::Q_PREFS_SENT_SECURELY_TO_TELEGRAM)),
        )
        .child(self.twofa_form_buttons(
            cx,
            TwofaView::Enable,
            tr(keys::RIGHTS_TRANSFER_SET_PASSWORD),
        ))
    }

    /// Slice A2: change form — current + new password + hint.
    pub(in crate::ui) fn twofa_change_body(&self, cx: &mut Context<Self>, body: Div) -> Div {
        body.child(
            div()
                .font_semibold()
                .text_sm()
                .child(tr(keys::SETTINGS_CLOUD_PASSWORD_MANAGE_PASSWORD_CHANGE)),
        )
        .child(
            div()
                .mt_1()
                .font_semibold()
                .text_sm()
                .child(tr(keys::Q_PREFS_CURRENT_PASSWORD)),
        )
        .child(
            Input::new(&self.twofa.current_password)
                .aria_label(tr(keys::Q_PREFS_CURRENT_TWO_STEP_VERIFICATION_PASSWORD))
                .content_type(InputContentType::Password)
                .h(px(40.)),
        )
        .child(
            div()
                .mt_1()
                .font_semibold()
                .text_sm()
                .child(tr(keys::Q_PREFS_NEW_PASSWORD)),
        )
        .child(
            Input::new(&self.twofa.new_password)
                .aria_label(tr(keys::Q_PREFS_NEW_TWO_STEP_VERIFICATION_PASSWORD))
                .content_type(InputContentType::Password)
                .h(px(40.)),
        )
        .child(
            div()
                .mt_1()
                .font_semibold()
                .text_sm()
                .child(tr(keys::Q_PREFS_HINT_OPTIONAL)),
        )
        .child(
            Textarea::new(&self.twofa.hint)
                .aria_label(tr(keys::SETTINGS_CLOUD_PASSWORD_HINT_TITLE))
                .h(px(40.)),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(tr(keys::Q_PREFS_SENT_SECURELY_TO_TELEGRAM)),
        )
        .child(self.twofa_forgot_link(cx))
        .child(self.twofa_form_buttons(
            cx,
            TwofaView::Change,
            tr(keys::SETTINGS_CLOUD_PASSWORD_MANAGE_PASSWORD_CHANGE),
        ))
    }

    /// Slice A2: disable form — current password, `setPassword` with an
    /// empty new password (schema 1.8.67, line 11434).
    pub(in crate::ui) fn twofa_disable_body(&self, cx: &mut Context<Self>, body: Div) -> Div {
        body.child(
            div()
                .font_semibold()
                .text_sm()
                .child(tr(keys::Q_PREFS_TURN_OFF_TWO_STEP_VERIFICATION)),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(tr(keys::Q_PREFS_YOU_WILL_NO_LONGER_BE)),
        )
        .child(
            div()
                .mt_1()
                .font_semibold()
                .text_sm()
                .child(tr(keys::Q_PREFS_CURRENT_PASSWORD)),
        )
        .child(
            Input::new(&self.twofa.current_password)
                .aria_label(tr(keys::Q_PREFS_CURRENT_TWO_STEP_VERIFICATION_PASSWORD))
                .content_type(InputContentType::Password)
                .h(px(40.)),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(tr(keys::Q_PREFS_SENT_SECURELY_TO_TELEGRAM)),
        )
        .child(self.twofa_forgot_link(cx))
        .child(self.twofa_form_buttons(cx, TwofaView::Disable, tr(keys::Q_PREFS_TURN_OFF)))
    }

    /// Slice A2: recovery-email form — current password + new address
    /// (`setRecoveryEmailAddress`, schema 1.8.67, line 11458).
    pub(in crate::ui) fn twofa_email_body(&self, cx: &mut Context<Self>, body: Div) -> Div {
        body.child(
            div()
                .font_semibold()
                .text_sm()
                .child(tr(keys::SETTINGS_CLOUD_PASSWORD_EMAIL_TITLE)),
        )
        .child(
            div()
                .mt_1()
                .font_semibold()
                .text_sm()
                .child(tr(keys::Q_PREFS_CURRENT_PASSWORD)),
        )
        .child(
            Input::new(&self.twofa.current_password)
                .aria_label(tr(keys::Q_PREFS_CURRENT_TWO_STEP_VERIFICATION_PASSWORD))
                .content_type(InputContentType::Password)
                .h(px(40.)),
        )
        .child(
            div()
                .mt_1()
                .font_semibold()
                .text_sm()
                .child(tr(keys::Q_PREFS_NEW_RECOVERY_EMAIL)),
        )
        .child(
            Textarea::new(&self.twofa.email)
                .aria_label(tr(keys::Q_PREFS_RECOVERY_EMAIL_ADDRESS))
                .h(px(40.)),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(tr(keys::Q_PREFS_THE_CHANGE_STAYS_PENDING_UNTIL)),
        )
        .child(self.twofa_forgot_link(cx))
        .child(self.twofa_form_buttons(cx, TwofaView::Email, tr(keys::SETTINGS_SAVE)))
    }

    /// Slice A2: the shared submit/back row for the 2FA forms. Submit
    /// dispatches to the password or email round-trip; back returns to
    /// the status screen without sending anything. A local validation
    /// notice renders above the buttons when a submit was refused.
    pub(in crate::ui) fn twofa_form_buttons(
        &self,
        cx: &mut Context<Self>,
        form: TwofaView,
        submit_label: &'static str,
    ) -> Div {
        let submit_id = match form {
            TwofaView::Enable => "twofa-submit-enable",
            TwofaView::Change => "twofa-submit-change",
            TwofaView::Disable => "twofa-submit-disable",
            TwofaView::Email => "twofa-submit-email",
            // These views draw their own buttons and never call this.
            TwofaView::Status | TwofaView::Recover | TwofaView::LoginEmail => return div(),
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .when_some(self.twofa.notice.clone(), |this, note| {
                this.child(div().text_xs().text_color(danger()).child(note))
            })
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(Button::new(submit_id).label(submit_label).ghost().on_click(
                        cx.listener(move |this, _, window, cx| {
                            if form == TwofaView::Email {
                                this.submit_twofa_email(window, cx);
                            } else {
                                this.submit_twofa_password(window, cx, form == TwofaView::Enable);
                            }
                            this.close_kit_dialog_if_done(DialogKind::TwoFa, window, cx);
                        }),
                    ))
                    .child(
                        Button::new("twofa-back")
                            .label(tr(keys::Q_PREFS_BACK))
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.goto_twofa_view(TwofaView::Status);
                                cx.notify();
                            })),
                    ),
            )
    }

    /// Slice A4: flip `toggleSessionCanAcceptSecretChats` for one session.
    /// Direct toggle — TGX's `EditSessionController` shows no
    /// confirmation — live only. The toggled value arrives in the
    /// authoritative list refetch; the row is never flipped
    /// optimistically.
    pub(in crate::ui) fn toggle_session_secret_chats(
        &mut self,
        session_id: i64,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            match live
                .driver
                .toggle_session_can_accept_secret_chats(session_id)
            {
                Ok(_) => self.connection.set_note(StatusNote::silent(tr(
                    keys::Q_PREFS_UPDATING_SESSION_SETTING,
                ))),
                Err(_) => self.connection.set_note(StatusNote::error(tr(
                    keys::Q_PREFS_COULD_NOT_UPDATE_THE_SESSION,
                ))),
            }
        }
        cx.notify();
    }

    /// Slice A4: flip `toggleSessionCanAcceptCalls` for one session — the
    /// `toggle_session_secret_chats` twin.
    pub(in crate::ui) fn toggle_session_calls(&mut self, session_id: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            match live.driver.toggle_session_can_accept_calls(session_id) {
                Ok(_) => self.connection.set_note(StatusNote::silent(tr(
                    keys::Q_PREFS_UPDATING_SESSION_SETTING,
                ))),
                Err(_) => self.connection.set_note(StatusNote::error(tr(
                    keys::Q_PREFS_COULD_NOT_UPDATE_THE_SESSION,
                ))),
            }
        }
        cx.notify();
    }

    /// Slice A3: the terminate confirmation banner (the
    /// `delete_confirm_banner` pattern) — TGX `TerminateSessionQuestion` /
    /// `TerminateIncompleteSessionQuestion` / `AreYouSureSessions`,
    /// verbatim.
    pub(in crate::ui) fn sessions_confirm_banner(
        &self,
        confirm: SessionsConfirm,
        mutating: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let question = match confirm {
            SessionsConfirm::TerminateOne {
                incomplete: true, ..
            } => tr(keys::Q_PREFS_TERMINATE_THIS_LOGIN_ATTEMPT),
            SessionsConfirm::TerminateOne { .. } => tr(keys::Q_PREFS_TERMINATE_THIS_SESSION),
            SessionsConfirm::TerminateAll => tr(keys::SETTINGS_RESET_SURE),
        };
        div()
            .id("sessions-confirm")
            .flex()
            .items_center()
            .justify_between()
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
                    .font_medium()
                    .text_color(danger())
                    .child(question),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("sessions-confirm-cancel")
                            .label(tr(keys::CANCEL))
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cancel_sessions_confirm(cx);
                            })),
                    )
                    .child(
                        Button::new("sessions-confirm-terminate")
                            .label(if mutating {
                                tr(keys::Q_PREFS_WORKING)
                            } else {
                                tr(keys::SETTINGS_RESET_BUTTON)
                            })
                            .danger()
                            .disabled(mutating)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.confirm_sessions_terminate(cx);
                            })),
                    ),
            )
    }

    /// Slice A3: one session row — device model title (+ "This device"
    /// chip), app + version, platform + version, IP + location, last
    /// active; a Terminate button for non-current sessions (TGX
    /// `SettingsSessionsController` row content).
    /// Slice A4: `show_toggles` (the current card, plus other
    /// non-incomplete sessions — like TGX's `EditSessionController`, whose
    /// `SessionAccepts` section is gated only on `!isPasswordPending`)
    /// adds the per-session "Secret Chats" / "Calls" accept/reject
    /// toggles (TGX `SessionSecretChats` / `SessionAcceptsCalls`,
    /// `SessionAccept` / `SessionReject`). They toggle directly — TGX
    /// shows no confirmation — and the value refreshes from the
    /// authoritative TDLib response, never optimistically.
    pub(in crate::ui) fn session_row(
        &self,
        s: &ParsedSession,
        mutating: bool,
        show_toggles: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let app_line = format!(
            "{} {}",
            s.application_name.trim(),
            s.application_version.trim()
        )
        .trim()
        .to_string();
        let platform_line = format!("{} {}", s.platform.trim(), s.system_version.trim())
            .trim()
            .to_string();
        let mut sub = Vec::new();
        if !app_line.is_empty() {
            sub.push(app_line);
        }
        if !platform_line.is_empty() {
            sub.push(platform_line);
        }
        let mut meta = Vec::new();
        if !s.ip_address.is_empty() {
            meta.push(s.ip_address.clone());
        }
        if !s.location.is_empty() {
            meta.push(s.location.clone());
        }
        meta.push(tr_with(
            keys::Q_PREFS_LAST_ACTIVE,
            &[(
                "last_active_date",
                &(format_session_last_active(s.last_active_date)).to_string(),
            )],
        ));
        sub.push(meta.join(" · "));
        let title = if s.device_model.trim().is_empty() {
            tr(keys::Q_PREFS_UNKNOWN_DEVICE).to_string()
        } else {
            s.device_model.clone()
        };
        let mut row = div()
            .id(format!("session-row-{}", s.id))
            .flex()
            .w_full()
            .items_center()
            .justify_between()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().text_sm().font_medium().child(title))
                            .when(s.is_current, |this| {
                                this.child(
                                    div()
                                        .text_xs()
                                        .px_2()
                                        .py(px(1.))
                                        .rounded_full()
                                        .bg(accent_light())
                                        .text_color(text_on_fill())
                                        .child(tr(keys::Q_PREFS_THIS_DEVICE)),
                                )
                            }),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(sub.join("\n")),
                    ),
            );
        // The toggles render on the current card too — TGX's
        // `EditSessionController` opens for the current session as well
        // (`SettingsSessionsController` `R.id.btn_currentSession`), and
        // its `SessionAccepts` section is gated only on
        // `!isPasswordPending`. The Terminate button stays
        // non-current-only.
        let session_id = s.id;
        let incomplete = s.is_password_pending;
        let mut actions = div().flex().flex_col().flex_shrink_0().items_end().gap_1();
        // Phase 6: kit Switches (were: "Accept/Reject" ghost buttons).
        // The flip handlers are unchanged (TGX toggles directly, the value
        // refreshes from the authoritative response); the switch only
        // forwards when the requested value differs from the rendered one.
        if show_toggles {
            let secret_on = s.can_accept_secret_chats;
            let calls_on = s.can_accept_calls;
            actions = actions
                .child(
                    Switch::new(format!("toggle-secret-chats-{session_id}"))
                        .label(tr(keys::Q_PREFS_SECRET_CHATS))
                        .checked(secret_on)
                        .disabled(mutating)
                        .on_click(cx.listener(move |this, &on, _, cx| {
                            if on != secret_on {
                                this.toggle_session_secret_chats(session_id, cx);
                            }
                        })),
                )
                .child(
                    Switch::new(format!("toggle-calls-{session_id}"))
                        .label(tr(keys::SETTINGS_CALLS))
                        .checked(calls_on)
                        .disabled(mutating)
                        .on_click(cx.listener(move |this, &on, _, cx| {
                            if on != calls_on {
                                this.toggle_session_calls(session_id, cx);
                            }
                        })),
                );
        }
        // Terminate and Details share one right-aligned row, so Details sits
        // in the same column on every card (TGX never offers to terminate
        // the current session: that card just has no Terminate).
        let mut buttons = div().flex().items_center().justify_end().gap_1();
        if !s.is_current {
            buttons = buttons.child(
                Button::new(format!("terminate-session-{session_id}"))
                    .label(tr(keys::SETTINGS_RESET_BUTTON))
                    .small()
                    .custom(quiet_danger(cx))
                    .disabled(mutating)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.begin_terminate_session(session_id, incomplete, cx);
                    })),
            );
        }
        buttons = buttons.child(
            Button::new(format!("session-details-{session_id}"))
                .label(tr(keys::Q_PREFS_DETAILS))
                .small()
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.privacy.extra.session_details = Some(session_id);
                    cx.notify();
                })),
        );
        actions = actions.child(buttons);
        row = row.child(actions);
        row.into_any_element()
    }

    /// Slice A4: open the Connected Websites overlay. Live: guarded fetch
    /// of the authoritative `getConnectedWebsites` answer (cached state
    /// reused, in-flight fetch deduped). Demo: the fixture is already
    /// injected.
    pub(in crate::ui) fn open_websites(&mut self, cx: &mut Context<Self>) {
        if quill::agent::ui_driver::guard::refuse_hard_if_injecting(
            quill::agent::ui_driver::guard::HardSurface::Security,
        ) {
            return;
        }
        self.privacy.websites_open = true;
        self.privacy.websites_confirm = None;
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.maybe_fetch_connected_websites();
        }
        cx.notify();
    }

    /// Slice A4: close the overlay and drop any pending disconnect
    /// confirmation.
    pub(in crate::ui) fn close_websites(&mut self, cx: &mut Context<Self>) {
        self.privacy.websites_open = false;
        self.privacy.websites_confirm = None;
        cx.notify();
    }

    /// Slice A4: refresh the list — live drops the cache so the guarded
    /// fetch refires; demo re-injects the fixture.
    pub(in crate::ui) fn refresh_websites(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.settings.connected_websites = None;
            live.driver.session.settings.websites_stale = false;
            let _ = live.driver.maybe_fetch_connected_websites();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.settings.connected_websites = Some(demo_websites());
            session.settings.connected_websites_loading = false;
            session.settings.websites_error = None;
        }
        cx.notify();
    }

    /// Slice A4: arm the disconnect confirmation for one website (TGX
    /// `TerminateWebSessionQuestion` "Disconnect %1$s?").
    pub(in crate::ui) fn begin_disconnect_website(
        &mut self,
        website_id: i64,
        cx: &mut Context<Self>,
    ) {
        self.privacy.websites_confirm = Some(WebsitesConfirm::DisconnectOne { website_id });
        cx.notify();
    }

    /// Slice A4: arm the "disconnect all websites" confirmation (TGX
    /// `DisconnectAllWebsitesHint` "Are you sure you want to disconnect
    /// all websites?").
    pub(in crate::ui) fn begin_disconnect_all_websites(&mut self, cx: &mut Context<Self>) {
        self.privacy.websites_confirm = Some(WebsitesConfirm::DisconnectAll);
        cx.notify();
    }

    /// Slice A4: drop the pending disconnect confirmation.
    pub(in crate::ui) fn cancel_websites_confirm(&mut self, cx: &mut Context<Self>) {
        self.privacy.websites_confirm = None;
        cx.notify();
    }

    /// Slice A4: send the confirmed disconnect. Live only — the demo has
    /// no TDLib; the list refreshes from the authoritative `ok` answer,
    /// never optimistically.
    pub(in crate::ui) fn confirm_websites_disconnect(&mut self, cx: &mut Context<Self>) {
        let confirm = self.privacy.websites_confirm.take();
        if let (Some(live), Some(confirm)) = (self.live.as_mut(), confirm) {
            let result = match confirm {
                WebsitesConfirm::DisconnectOne { website_id } => {
                    live.driver.disconnect_website(website_id).map(|_| ())
                }
                WebsitesConfirm::DisconnectAll => live.driver.disconnect_all_websites().map(|_| ()),
            };
            self.connection.set_note(match result {
                Ok(()) => StatusNote::silent(tr(keys::Q_PREFS_DISCONNECTING_WEBSITE)),
                Err(_) => StatusNote::error(tr(keys::Q_PREFS_COULD_NOT_DISCONNECT_THE_WEBSITE)),
            });
        }
        cx.notify();
    }

    /// Slice A4: the disconnect confirmation banner (the
    /// `sessions_confirm_banner` pattern) — TGX
    /// `TerminateWebSessionQuestion` "Disconnect %1$s?" /
    /// `DisconnectAllWebsitesHint` "Are you sure you want to disconnect
    /// all websites?", verbatim. (TGX's optional "Block %1$s" checkbox is
    /// out of this slice — see DECISIONS.md.)
    pub(in crate::ui) fn websites_confirm_banner(
        &self,
        confirm: WebsitesConfirm,
        websites: &[ParsedWebsite],
        mutating: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let question = match confirm {
            WebsitesConfirm::DisconnectOne { website_id } => websites
                .iter()
                .find(|w| w.id == website_id)
                .map(|w| {
                    if w.domain_name.trim().is_empty() {
                        tr(keys::Q_PREFS_DISCONNECT_THIS_WEBSITE).to_string()
                    } else {
                        tr_with(
                            keys::Q_PREFS_DISCONNECT,
                            &[("trim", (w.domain_name.trim()))],
                        )
                    }
                })
                .unwrap_or_else(|| tr(keys::Q_PREFS_DISCONNECT_THIS_WEBSITE).to_string()),
            WebsitesConfirm::DisconnectAll => tr(keys::Q_PREFS_ARE_YOU_SURE_YOU_WANT).to_string(),
        };
        div()
            .id("websites-confirm")
            .flex()
            .items_center()
            .justify_between()
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
                    .font_medium()
                    .text_color(danger())
                    .child(question),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("websites-confirm-cancel")
                            .label(tr(keys::CANCEL))
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cancel_websites_confirm(cx);
                            })),
                    )
                    .child(
                        Button::new("websites-confirm-disconnect")
                            .label(if mutating {
                                tr(keys::Q_PREFS_WORKING)
                            } else {
                                tr(keys::SETTINGS_DISCONNECT)
                            })
                            .danger()
                            .disabled(mutating)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.confirm_websites_disconnect(cx);
                            })),
                    ),
            )
    }

    /// Slice A4: one website row — domain title, browser · platform,
    /// logged-in date, IP · location · last active; a Disconnect button
    /// (TGX `SettingsWebsitesController` row content; the bot username
    /// subtext needs a users-cache lookup — out of this slice).
    pub(in crate::ui) fn website_row(
        &self,
        w: &ParsedWebsite,
        mutating: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut sub = Vec::new();
        if !w.browser.trim().is_empty() {
            sub.push(w.browser.clone());
        }
        if !w.platform.trim().is_empty() {
            sub.push(w.platform.clone());
        }
        let mut meta = Vec::new();
        if w.log_in_date > 0 {
            meta.push(tr_with(
                keys::Q_PREFS_LOGGED_IN_2,
                &[(
                    "log_in_date",
                    &(format_session_last_active(w.log_in_date)).to_string(),
                )],
            ));
        }
        if !w.ip_address.is_empty() {
            meta.push(w.ip_address.clone());
        }
        if !w.location.is_empty() {
            meta.push(w.location.clone());
        }
        meta.push(tr_with(
            keys::Q_PREFS_LAST_ACTIVE,
            &[(
                "last_active_date",
                &(format_session_last_active(w.last_active_date)).to_string(),
            )],
        ));
        sub.push(meta.join(" · "));
        let title = if w.domain_name.trim().is_empty() {
            tr(keys::Q_PREFS_UNKNOWN_WEBSITE).to_string()
        } else {
            w.domain_name.clone()
        };
        let website_id = w.id;
        div()
            .id(format!("website-row-{website_id}"))
            .flex()
            .w_full()
            .items_center()
            .justify_between()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(div().text_sm().font_medium().child(title))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(sub.join("\n")),
                    ),
            )
            .child(
                // TGX `DisconnectWebsiteAction` "Disconnect Website".
                div().flex_shrink_0().child(
                    Button::new(format!("disconnect-website-{website_id}"))
                        .label(tr(keys::SETTINGS_DISCONNECT))
                        .small()
                        .custom(quiet_danger(cx))
                        .disabled(mutating)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.begin_disconnect_website(website_id, cx);
                        })),
                ),
            )
            .into_any_element()
    }
}
