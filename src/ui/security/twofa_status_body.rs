//! Methods moved out of `security.rs` to keep files under 1000 lines.

use super::*;

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
                .child(div().font_semibold().text_sm().child("Status"))
                .child(
                    div()
                        .text_sm()
                        .child(if state.has_password { "On" } else { "Off" }),
                ),
        );
        if state.has_password && !state.password_hint.is_empty() {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("Hint: {}", state.password_hint)),
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
                .child(div().font_semibold().text_sm().child("Recovery email"))
                .child(div().text_sm().child(if state.has_recovery_email_address {
                    "Set"
                } else {
                    "Not set"
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
                    .label("Change Password")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.goto_twofa_view(TwofaView::Change);
                        cx.notify();
                    })),
            );
            actions = actions.child(
                Button::new("twofa-goto-disable")
                    .label("Turn off")
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
                        "Change Recovery Email"
                    } else {
                        "Set Recovery Email"
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
                    .label("Set additional password")
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
                    .child("Working…"),
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
                .child("Set additional password"),
        )
        .child(div().mt_1().font_semibold().text_sm().child("New password"))
        .child(
            Input::new(&self.twofa.new_password)
                .aria_label("New two-step verification password")
                .content_type(InputContentType::Password)
                .h(px(40.)),
        )
        .child(
            div()
                .mt_1()
                .font_semibold()
                .text_sm()
                .child("Hint (optional)"),
        )
        .child(
            Textarea::new(&self.twofa.hint)
                .aria_label("Password hint")
                .h(px(40.)),
        )
        .child(
            div()
                .mt_1()
                .font_semibold()
                .text_sm()
                .child("Recovery email (optional)"),
        )
        .child(
            Textarea::new(&self.twofa.email)
                .aria_label("Recovery email address")
                .h(px(40.)),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("Sent securely to Telegram."),
        )
        .child(self.twofa_form_buttons(cx, TwofaView::Enable, "Set password"))
    }

    /// Slice A2: change form — current + new password + hint.
    pub(in crate::ui) fn twofa_change_body(&self, cx: &mut Context<Self>, body: Div) -> Div {
        body.child(div().font_semibold().text_sm().child("Change Password"))
            .child(
                div()
                    .mt_1()
                    .font_semibold()
                    .text_sm()
                    .child("Current password"),
            )
            .child(
                Input::new(&self.twofa.current_password)
                    .aria_label("Current two-step verification password")
                    .content_type(InputContentType::Password)
                    .h(px(40.)),
            )
            .child(div().mt_1().font_semibold().text_sm().child("New password"))
            .child(
                Input::new(&self.twofa.new_password)
                    .aria_label("New two-step verification password")
                    .content_type(InputContentType::Password)
                    .h(px(40.)),
            )
            .child(
                div()
                    .mt_1()
                    .font_semibold()
                    .text_sm()
                    .child("Hint (optional)"),
            )
            .child(
                Textarea::new(&self.twofa.hint)
                    .aria_label("Password hint")
                    .h(px(40.)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Sent securely to Telegram."),
            )
            .child(self.twofa_forgot_link(cx))
            .child(self.twofa_form_buttons(cx, TwofaView::Change, "Change password"))
    }

    /// Slice A2: disable form — current password, `setPassword` with an
    /// empty new password (schema 1.8.67, line 11434).
    pub(in crate::ui) fn twofa_disable_body(&self, cx: &mut Context<Self>, body: Div) -> Div {
        body.child(
            div()
                .font_semibold()
                .text_sm()
                .child("Turn off two-step verification"),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("You will no longer be asked for an extra password when signing in."),
        )
        .child(
            div()
                .mt_1()
                .font_semibold()
                .text_sm()
                .child("Current password"),
        )
        .child(
            Input::new(&self.twofa.current_password)
                .aria_label("Current two-step verification password")
                .content_type(InputContentType::Password)
                .h(px(40.)),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("Sent securely to Telegram."),
        )
        .child(self.twofa_forgot_link(cx))
        .child(self.twofa_form_buttons(cx, TwofaView::Disable, "Turn off"))
    }

    /// Slice A2: recovery-email form — current password + new address
    /// (`setRecoveryEmailAddress`, schema 1.8.67, line 11458).
    pub(in crate::ui) fn twofa_email_body(&self, cx: &mut Context<Self>, body: Div) -> Div {
        body.child(div().font_semibold().text_sm().child("Recovery email"))
            .child(
                div()
                    .mt_1()
                    .font_semibold()
                    .text_sm()
                    .child("Current password"),
            )
            .child(
                Input::new(&self.twofa.current_password)
                    .aria_label("Current two-step verification password")
                    .content_type(InputContentType::Password)
                    .h(px(40.)),
            )
            .child(
                div()
                    .mt_1()
                    .font_semibold()
                    .text_sm()
                    .child("New recovery email"),
            )
            .child(
                Textarea::new(&self.twofa.email)
                    .aria_label("Recovery email address")
                    .h(px(40.)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("The change stays pending until the new address is confirmed."),
            )
            .child(self.twofa_forgot_link(cx))
            .child(self.twofa_form_buttons(cx, TwofaView::Email, "Save"))
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
            TwofaView::Status | TwofaView::Recover | TwofaView::LoginEmail => {
                unreachable!("this view has its own buttons")
            }
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
                            .label("Back")
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
                Ok(_) => self.connection.status_note = "Updating session setting…".into(),
                Err(_) => {
                    self.connection.status_note = "Could not update the session setting.".into()
                }
            }
        }
        cx.notify();
    }

    /// Slice A4: flip `toggleSessionCanAcceptCalls` for one session — the
    /// `toggle_session_secret_chats` twin.
    pub(in crate::ui) fn toggle_session_calls(&mut self, session_id: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            match live.driver.toggle_session_can_accept_calls(session_id) {
                Ok(_) => self.connection.status_note = "Updating session setting…".into(),
                Err(_) => {
                    self.connection.status_note = "Could not update the session setting.".into()
                }
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
            } => "Terminate this login attempt?",
            SessionsConfirm::TerminateOne { .. } => "Terminate this session?",
            SessionsConfirm::TerminateAll => {
                "Are you sure you want to terminate all other sessions?"
            }
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
                            .label("Cancel")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cancel_sessions_confirm(cx);
                            })),
                    )
                    .child(
                        Button::new("sessions-confirm-terminate")
                            .label(if mutating { "Working…" } else { "Terminate" })
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
        meta.push(format!(
            "Last active: {}",
            format_session_last_active(s.last_active_date)
        ));
        sub.push(meta.join(" · "));
        let title = if s.device_model.trim().is_empty() {
            "Unknown device".to_string()
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
                                        .child("This Device"),
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
                        .label("Secret Chats")
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
                        .label("Calls")
                        .checked(calls_on)
                        .disabled(mutating)
                        .on_click(cx.listener(move |this, &on, _, cx| {
                            if on != calls_on {
                                this.toggle_session_calls(session_id, cx);
                            }
                        })),
                );
        }
        // TGX never offers to terminate the current session.
        if !s.is_current {
            actions = actions.child(
                Button::new(format!("terminate-session-{session_id}"))
                    .label("Terminate")
                    .small()
                    .custom(quiet_danger(cx))
                    .disabled(mutating)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.begin_terminate_session(session_id, incomplete, cx);
                    })),
            );
        }
        actions = actions.child(
            Button::new(format!("session-details-{session_id}"))
                .label("Details")
                .small()
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.privacy.extra.session_details = Some(session_id);
                    cx.notify();
                })),
        );
        row = row.child(actions);
        row.into_any_element()
    }

    /// Slice A4: open the Connected Websites overlay. Live: guarded fetch
    /// of the authoritative `getConnectedWebsites` answer (cached state
    /// reused, in-flight fetch deduped). Demo: the fixture is already
    /// injected.
    pub(in crate::ui) fn open_websites(&mut self, cx: &mut Context<Self>) {
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
            self.connection.status_note = match result {
                Ok(()) => "Disconnecting website…".into(),
                Err(_) => "Could not disconnect the website.".into(),
            };
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
                        "Disconnect this website?".to_string()
                    } else {
                        format!("Disconnect {}?", w.domain_name.trim())
                    }
                })
                .unwrap_or_else(|| "Disconnect this website?".to_string()),
            WebsitesConfirm::DisconnectAll => {
                "Are you sure you want to disconnect all websites?".to_string()
            }
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
                            .label("Cancel")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cancel_websites_confirm(cx);
                            })),
                    )
                    .child(
                        Button::new("websites-confirm-disconnect")
                            .label(if mutating { "Working…" } else { "Disconnect" })
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
            meta.push(format!(
                "Logged in: {}",
                format_session_last_active(w.log_in_date)
            ));
        }
        if !w.ip_address.is_empty() {
            meta.push(w.ip_address.clone());
        }
        if !w.location.is_empty() {
            meta.push(w.location.clone());
        }
        meta.push(format!(
            "Last active: {}",
            format_session_last_active(w.last_active_date)
        ));
        sub.push(meta.join(" · "));
        let title = if w.domain_name.trim().is_empty() {
            "Unknown website".to_string()
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
                        .label("Disconnect")
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
