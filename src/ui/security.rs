//! security.

use super::app::QuillApp;
use super::demo::{demo_sessions, demo_storage_stats, demo_websites};
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::table::{Table, TableBody, TableRow};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::key_fingerprint;
use quill::telegram::envelope::{ParsedSecretChat, ParsedSession, ParsedWebsite, PasswordState};
use std::cell::RefCell;
use std::rc::Rc;
use zeroize::Zeroize;
impl QuillApp {
    /// kit Phase 2 (redo): two-step verification hosted in a kit `Dialog`
    /// via `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    pub(super) fn build_twofa_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::TwoFa, |this, window, cx| {
                this.close_twofa(window, cx);
            });
        app.update(cx, |this, cx| {
            let session = this.session();
            let state = session.as_ref().and_then(|s| s.password_state.clone());
            let loading = session.is_some_and(|s| s.password_state_loading);
            let error = session.as_ref().and_then(|s| s.password_op_error.clone());
            let mut body = div().flex().flex_col().gap_2();
            if let Some(line) = error {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(danger())
                        .child(format!("Error: {line}")),
                );
            }
            body = match (this.twofa_view, state) {
                (TwofaView::Status, None) => body.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(if loading {
                            "Loading…"
                        } else {
                            "No two-step verification data yet."
                        }),
                ),
                (TwofaView::Status, Some(state)) => {
                    this.twofa_status_body(cx, body, &state, loading)
                }
                (TwofaView::Enable, _) => this.twofa_enable_body(cx, body),
                (TwofaView::Change, _) => this.twofa_change_body(cx, body),
                (TwofaView::Disable, _) => this.twofa_disable_body(cx, body),
                (TwofaView::Email, _) => this.twofa_email_body(cx, body),
            };
            let footer = div().flex().justify_end().child(
                Button::new("close-twofa")
                    .label("Close")
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_twofa(window, cx);
                        this.close_kit_dialog_if_done(DialogKind::TwoFa, window, cx);
                    })),
            );
            dialog
                .overlay(true)
                .title("Two-Step Verification")
                .content({
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
                })
                .footer(footer)
                .on_close(on_close)
        })
    }

    /// kit Phase 2 (redo): sessions hosted in a kit `Dialog` via
    /// `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    pub(super) fn build_sessions_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::Sessions, |this, _, cx| {
                this.close_sessions(cx);
            });
        app.update(cx, |this, cx| {
            let session = this.session();
            let sessions = session
                .as_ref()
                .and_then(|s| s.sessions.clone())
                .unwrap_or_default();
            let loading = session.is_some_and(|s| s.sessions_loading);
            let mutating = session.is_some_and(|s| s.sessions_mutating);
            let stale = session.is_some_and(|s| s.sessions_stale);
            let error = session.as_ref().and_then(|s| s.sessions_error.clone());
            let current = sessions.iter().find(|s| s.is_current);
            let mut incomplete: Vec<&ParsedSession> = sessions
                .iter()
                .filter(|s| !s.is_current && s.is_password_pending)
                .collect();
            incomplete.sort_by(|a, b| b.last_active_date.cmp(&a.last_active_date));
            let mut others: Vec<&ParsedSession> = sessions
                .iter()
                .filter(|s| !s.is_current && !s.is_password_pending)
                .collect();
            others.sort_by(|a, b| b.last_active_date.cmp(&a.last_active_date));

            let mut body = div().flex().flex_col().gap_2();
            if let Some(line) = error {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(danger())
                        .child(format!("Error: {line}")),
                );
            }
            if let Some(confirm) = this.sessions_confirm {
                body = body.child(this.sessions_confirm_banner(confirm, mutating, cx));
            }
            if sessions.is_empty() {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(if loading {
                            "Loading…"
                        } else {
                            "No session data yet."
                        }),
                );
            } else {
                // A terminate just landed: the old list stays visible while
                // the authoritative refetch is in flight (never an optimistic
                // delete).
                if stale && loading {
                    body = body.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Refreshing…"),
                    );
                }
                // Phase 6: each session group is a one-column kit Table
                // hosting the rich session rows (was: bare rows).
                if let Some(current) = current {
                    body = body
                        .child(
                            div()
                                .text_xs()
                                .font_medium()
                                .text_color(cx.theme().muted_foreground)
                                .child("Current session"),
                        )
                        .child(Table::new().w_full().child(TableBody::new().child(
                            TableRow::new().child(Self::table_cell(
                                this.session_row(current, mutating, true, cx),
                            )),
                        )));
                }
                if !incomplete.is_empty() {
                    body = body
                        .child(
                            div()
                                .text_xs()
                                .font_medium()
                                .text_color(cx.theme().muted_foreground)
                                .child("Incomplete Login Attempts"),
                        )
                        .child(
                            div().text_xs().text_color(cx.theme().muted_foreground).child(
                                "The devices above have no access to your messages. The code was entered correctly, but no correct password was given.",
                            ),
                        );
                    let mut incomplete_body = TableBody::new();
                    for s in incomplete {
                        incomplete_body = incomplete_body.child(
                            TableRow::new().child(Self::table_cell(
                                this.session_row(s, mutating, false, cx),
                            )),
                        );
                    }
                    body = body.child(Table::new().w_full().child(incomplete_body));
                }
                if !others.is_empty() {
                    body = body.child(
                        div()
                            .text_xs()
                            .font_medium()
                            .text_color(cx.theme().muted_foreground)
                            .child("Other sessions"),
                    );
                    let mut others_body = TableBody::new();
                    for s in others {
                        others_body = others_body.child(
                            TableRow::new().child(Self::table_cell(
                                this.session_row(s, mutating, true, cx),
                            )),
                        );
                    }
                    body = body.child(Table::new().w_full().child(others_body));
                }
                let any_other = sessions.iter().any(|s| !s.is_current);
                body = body.child(
                    div().flex().justify_end().child(
                        Button::new("terminate-all-sessions")
                            .label("Terminate All Other Sessions")
                            .danger()
                            .disabled(!any_other || mutating)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.begin_terminate_all_sessions(cx);
                                this.close_kit_dialog_if_done(DialogKind::Sessions, window, cx);
                            })),
                    ),
                );
            }
            let footer = div().flex().justify_end().gap_2().child(
                Button::new("sessions-refresh")
                    .label("Refresh")
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.refresh_sessions(cx);
                        this.close_kit_dialog_if_done(DialogKind::Sessions, window, cx);
                    })),
            ).child(
                Button::new("close-sessions")
                    .label("Close")
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_sessions(cx);
                        this.close_kit_dialog_if_done(DialogKind::Sessions, window, cx);
                    })),
            );
            dialog
                .overlay(true)
                .title("Active Sessions")
                .content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body.borrow_mut().take().unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                })
                .footer(footer)
                .on_close(on_close)
        })
    }

    /// kit Phase 2 (redo): connected websites hosted in a kit `Dialog`
    /// via `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    pub(super) fn build_websites_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::Websites, |this, _, cx| {
                this.close_websites(cx);
            });
        app.update(cx, |this, cx| {
            let session = this.session();
            let has_websites = session
                .as_ref()
                .is_some_and(|s| s.connected_websites.is_some());
            let mut websites = session
                .as_ref()
                .and_then(|s| s.connected_websites.clone())
                .unwrap_or_default();
            websites.sort_by(|a, b| b.last_active_date.cmp(&a.last_active_date));
            let loading = session.is_some_and(|s| s.connected_websites_loading);
            let mutating = session.is_some_and(|s| s.websites_mutating);
            let stale = session.is_some_and(|s| s.websites_stale);
            let error = session.as_ref().and_then(|s| s.websites_error.clone());

            let mut body = div().flex().flex_col().gap_2();
            if let Some(line) = error {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(danger())
                        .child(format!("Error: {line}")),
                );
            }
            if let Some(confirm) = this.websites_confirm {
                body = body.child(this.websites_confirm_banner(confirm, &websites, mutating, cx));
            }
            if websites.is_empty() {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(if loading {
                            "Loading…"
                        } else if has_websites {
                            "No active logins — you can log in on websites that support signing in with Telegram."
                        } else {
                            "No website data yet."
                        }),
                );
            } else {
                if stale && loading {
                    body = body.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Refreshing…"),
                    );
                }
                body = body
                    .child(
                        div().flex().justify_end().child(
                            Button::new("disconnect-all-websites")
                                .label("Disconnect All Websites")
                                .danger()
                                .disabled(mutating)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.begin_disconnect_all_websites(cx);
                                    this.close_kit_dialog_if_done(DialogKind::Websites, window, cx);
                                })),
                        ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("You can log in on websites that support signing in with Telegram."),
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_medium()
                            .text_color(cx.theme().muted_foreground)
                            .child("Connected Websites"),
                    );
                // Phase 6: a one-column kit Table hosting the rich
                // website rows (was: bare rows).
                let mut websites_body = TableBody::new();
                for w in &websites {
                    websites_body = websites_body.child(
                        TableRow::new()
                            .child(Self::table_cell(this.website_row(w, mutating, cx))),
                    );
                }
                body = body.child(Table::new().w_full().child(websites_body));
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Tap to disconnect from your Telegram account."),
                );
            }
            let footer = div().flex().justify_end().gap_2().child(
                Button::new("websites-refresh")
                    .label("Refresh")
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.refresh_websites(cx);
                        this.close_kit_dialog_if_done(DialogKind::Websites, window, cx);
                    })),
            ).child(
                Button::new("close-websites")
                    .label("Close")
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_websites(cx);
                        this.close_kit_dialog_if_done(DialogKind::Websites, window, cx);
                    })),
            );
            dialog
                .overlay(true)
                // TGX `WebSessionsTitle`, verbatim.
                .title("Logged In with Telegram")
                .content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body.borrow_mut().take().unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                })
                .footer(footer)
                .on_close(on_close)
        })
    }

    /// info panel — the 12×12 fingerprint grid from `secretChat.key_hash`
    /// (schema 1.8.67 lines 2812–2813: 36 little-endian bytes → 144
    /// two-bit pixels in FFFFFF / D5E6F3 / 2D5775 / 2F99C9) with
    /// Telegram-style verification copy. A Ready record whose hash isn't
    /// 36 bytes yet (still resolving) shows a loading note instead of
    /// the grid — graceful missing-key handling.
    ///
    /// Security: `record.key_hash` bytes are passed straight into
    /// `key_fingerprint::key_hash_pixels`; only pixel indices / colors
    /// enter the element tree — raw key bytes never leave `Session`.
    pub(super) fn encryption_key_section(
        &self,
        record: &ParsedSecretChat,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        const CELL_PX: f32 = 18.0;
        let pixels = key_fingerprint::key_hash_pixels(&record.key_hash);
        let mut body = div()
            .flex()
            .flex_col()
            .w_full()
            .gap_2()
            .pt_2()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .text_color(cx.theme().muted_foreground)
                    .child("Encryption key"),
            );
        match pixels {
            Some(pixels) => {
                let mut grid = div().flex().flex_col();
                for row in pixels.chunks(key_fingerprint::KEY_GRID_SIZE) {
                    let mut line = div().flex().flex_row();
                    for pixel in row {
                        line = line.child(
                            div()
                                .w(px(CELL_PX))
                                .h(px(CELL_PX))
                                .bg(rgb(key_fingerprint::key_pixel_color(*pixel))),
                        );
                    }
                    grid = grid.child(line);
                }
                body = body
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .child(grid.border_1().border_color(cx.theme().border)),
                    )
                    .child({
                        // Phase S1: TGX `EncryptionKeyDescription`, verbatim,
                        // with the peer's display name.
                        let name =
                            Self::secret_peer_name(self.session(), record.user_id, "your contact");
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "This image and text were derived from the encryption key for this \
                                 secret chat with {name}.\n\nIf they look the same on {name}'s \
                                 device, end-to-end encryption is guaranteed."
                            ))
                    });
            }
            None => {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Encryption key · still loading…"),
                );
            }
        }
        body.into_any_element()
    }

    /// Phase S2: refetch the storage stats. Live: drop the cache so the
    /// guarded fetch fires again. Demo: re-inject the fixture stats.
    pub(super) fn refresh_storage_usage(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.refresh_storage_statistics();
            let _ = live.driver.maybe_fetch_storage_statistics();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.storage_stats = Some(demo_storage_stats());
            session.storage_stats_loading = false;
        }
        cx.notify();
    }

    /// Slice A2: open the two-step verification overlay. Live: guarded
    /// fetch of the authoritative `passwordState` (cached state reused,
    /// in-flight fetch deduped). Demo: the fixture is already injected.
    pub(super) fn open_twofa(&mut self, cx: &mut Context<Self>) {
        self.twofa_open = true;
        self.goto_twofa_view(TwofaView::Status);
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.fetch_password_state();
        }
        cx.notify();
    }

    /// Slice A3: open the Active Sessions overlay. Live: guarded fetch of
    /// the authoritative `getActiveSessions` answer (cached state reused,
    /// in-flight fetch deduped). Demo: the fixture is already injected.
    pub(super) fn open_sessions(&mut self, cx: &mut Context<Self>) {
        self.sessions_open = true;
        self.sessions_confirm = None;
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.maybe_fetch_active_sessions();
        }
        cx.notify();
    }

    /// Slice A2: close the overlay and clear every 2FA input — passwords
    /// must not linger in the form after the dialog is gone.
    pub(super) fn close_twofa(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.twofa_open = false;
        self.goto_twofa_view(TwofaView::Status);
        for input in [
            &self.twofa_current_password,
            &self.twofa_new_password,
            &self.twofa_hint,
            &self.twofa_email,
        ] {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
    }

    /// Slice A3: close the overlay and drop any pending terminate
    /// confirmation.
    pub(super) fn close_sessions(&mut self, cx: &mut Context<Self>) {
        self.sessions_open = false;
        self.sessions_confirm = None;
        cx.notify();
    }

    /// Slice A3: refresh the list — live drops the cache so the guarded
    /// fetch refires; demo re-injects the fixture.
    pub(super) fn refresh_sessions(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.sessions = None;
            live.driver.session.sessions_stale = false;
            let _ = live.driver.maybe_fetch_active_sessions();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.sessions = Some(demo_sessions());
            session.sessions_loading = false;
            session.sessions_error = None;
        }
        cx.notify();
    }

    /// Slice A2: switch the 2FA view; a fresh form starts with no local
    /// notice line.
    pub(super) fn goto_twofa_view(&mut self, view: TwofaView) {
        self.twofa_view = view;
        self.twofa_notice = None;
    }

    /// Slice A2: one `setPassword` round-trip — enable (empty current),
    /// change, or disable (empty new). A recovery email rides along only
    /// on the enable form (TGX `PasswordController.java:1990`); on
    /// change/disable a leftover email would put the request into
    /// pending-confirmation and silently not apply (schema 1.8.67, line
    /// 11434). The fields are cleared and the password strings zeroized
    /// immediately after the send; the status view shows TDLib's
    /// authoritative answer when it arrives.
    pub(super) fn submit_twofa_password(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        include_email: bool,
    ) {
        let mut current = self.twofa_current_password.read(cx).value().to_string();
        let mut new = self.twofa_new_password.read(cx).value().to_string();
        let hint = self.twofa_hint.read(cx).value().to_string();
        let email = self.twofa_email.read(cx).value().trim().to_string();
        // Slice A2 fixup: never fire a doomed request — the driver
        // rejects it silently and the old code cleared the form first, so
        // the submit vanished with no feedback.
        let missing: Option<&str> = if include_email && new.is_empty() {
            Some("enter a new password")
        } else if !include_email && current.is_empty() {
            Some("enter your current password")
        } else {
            None
        };
        if let Some(note) = missing {
            self.twofa_notice = Some(note.to_string());
            current.zeroize();
            new.zeroize();
            cx.notify();
            return;
        }
        self.twofa_notice = None;
        for input in [
            &self.twofa_current_password,
            &self.twofa_new_password,
            &self.twofa_hint,
            &self.twofa_email,
        ] {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        if let Some(live) = self.live.as_mut() {
            let email_opt = if include_email && !email.is_empty() {
                Some(email.as_str())
            } else {
                None
            };
            let _ = live
                .driver
                .set_two_step_password(&current, &new, &hint, email_opt);
        }
        current.zeroize();
        new.zeroize();
        self.goto_twofa_view(TwofaView::Status);
        cx.notify();
    }

    /// Slice A2: one `setRecoveryEmailAddress` round-trip. Requires the
    /// current two-step password; the change stays pending until the new
    /// address is confirmed.
    pub(super) fn submit_twofa_email(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut current = self.twofa_current_password.read(cx).value().to_string();
        let email = self.twofa_email.read(cx).value().trim().to_string();
        // Slice A2 fixup: the driver rejects an empty password or email
        // silently — say so locally instead of clearing the form.
        let missing: Option<&str> = if current.is_empty() {
            Some("enter your current password")
        } else if email.is_empty() {
            Some("enter the new recovery email")
        } else {
            None
        };
        if let Some(note) = missing {
            self.twofa_notice = Some(note.to_string());
            current.zeroize();
            cx.notify();
            return;
        }
        self.twofa_notice = None;
        for input in [&self.twofa_current_password, &self.twofa_email] {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.set_recovery_email(&current, &email);
        }
        current.zeroize();
        self.goto_twofa_view(TwofaView::Status);
        cx.notify();
    }

    /// Slice A2: resend the pending recovery-email confirmation code.
    /// TDLib enforces its own server-side cooldown — no local countdown
    /// is invented.
    pub(super) fn resend_twofa_code(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.resend_recovery_email_code();
        }
        cx.notify();
    }

    /// Slice A3: arm the terminate confirmation for one session
    /// (TGX `TerminateSessionQuestion` /
    /// `TerminateIncompleteSessionQuestion`).
    pub(super) fn begin_terminate_session(
        &mut self,
        session_id: i64,
        incomplete: bool,
        cx: &mut Context<Self>,
    ) {
        self.sessions_confirm = Some(SessionsConfirm::TerminateOne {
            session_id,
            incomplete,
        });
        cx.notify();
    }

    /// Slice A3: arm the "terminate all other sessions" confirmation
    /// (TGX `AreYouSureSessions`).
    pub(super) fn begin_terminate_all_sessions(&mut self, cx: &mut Context<Self>) {
        self.sessions_confirm = Some(SessionsConfirm::TerminateAll);
        cx.notify();
    }

    /// Slice A3: drop the pending terminate confirmation.
    pub(super) fn cancel_sessions_confirm(&mut self, cx: &mut Context<Self>) {
        self.sessions_confirm = None;
        cx.notify();
    }

    /// Slice A3: send the confirmed terminate. Live only — the demo has
    /// no TDLib; the list refreshes from the authoritative `ok` answer,
    /// never optimistically.
    pub(super) fn confirm_sessions_terminate(&mut self, cx: &mut Context<Self>) {
        let confirm = self.sessions_confirm.take();
        if let (Some(live), Some(confirm)) = (self.live.as_mut(), confirm) {
            let result = match confirm {
                SessionsConfirm::TerminateOne { session_id, .. } => {
                    live.driver.terminate_session(session_id).map(|_| ())
                }
                SessionsConfirm::TerminateAll => {
                    live.driver.terminate_all_other_sessions().map(|_| ())
                }
            };
            self.status_note = match result {
                Ok(()) => "Terminating session…".into(),
                Err(_) => "Could not terminate the session.".into(),
            };
        }
        cx.notify();
    }

    /// Slice A2: the status screen — current state plus the pending
    /// confirmation card (TGX `PendingEmailText`) when a recovery email
    /// is awaiting confirmation.
    pub(super) fn twofa_status_body(
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
            body = body
                .child(div().text_xs().child(format!(
                    "Your recovery email {pattern} is not yet active and pending confirmation."
                )))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("twofa-resend-code")
                                .label("Resend code")
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.resend_twofa_code(cx);
                                })),
                        )
                        .child(
                            // TGX `AbortRecoveryEmail`, verbatim. The abort
                            // is confirmed like TGX's
                            // `AbortRecoveryEmailConfirm` — one tap opens
                            // the shared confirm dialog. No chat is
                            // involved, so the dialog's chat id is a dummy.
                            Button::new("twofa-abort-email")
                                .label("Abort recovery email setup")
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.open_group_confirm(
                                        ChatId(0),
                                        GroupConfirmAction::AbortRecoveryEmailSetup,
                                        cx,
                                    );
                                })),
                        ),
                );
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
    pub(super) fn twofa_enable_body(&self, cx: &mut Context<Self>, body: Div) -> Div {
        body.child(
            div()
                .font_semibold()
                .text_sm()
                .child("Set additional password"),
        )
        .child(div().mt_1().font_semibold().text_sm().child("New password"))
        .child(Textarea::new(&self.twofa_new_password).h(px(40.)))
        .child(
            div()
                .mt_1()
                .font_semibold()
                .text_sm()
                .child("Hint (optional)"),
        )
        .child(Textarea::new(&self.twofa_hint).h(px(40.)))
        .child(
            div()
                .mt_1()
                .font_semibold()
                .text_sm()
                .child("Recovery email (optional)"),
        )
        .child(Textarea::new(&self.twofa_email).h(px(40.)))
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("Sent to TDLib only — never logged"),
        )
        .child(self.twofa_form_buttons(cx, TwofaView::Enable, "Set password"))
    }

    /// Slice A2: change form — current + new password + hint.
    pub(super) fn twofa_change_body(&self, cx: &mut Context<Self>, body: Div) -> Div {
        body.child(div().font_semibold().text_sm().child("Change Password"))
            .child(
                div()
                    .mt_1()
                    .font_semibold()
                    .text_sm()
                    .child("Current password"),
            )
            .child(Textarea::new(&self.twofa_current_password).h(px(40.)))
            .child(div().mt_1().font_semibold().text_sm().child("New password"))
            .child(Textarea::new(&self.twofa_new_password).h(px(40.)))
            .child(
                div()
                    .mt_1()
                    .font_semibold()
                    .text_sm()
                    .child("Hint (optional)"),
            )
            .child(Textarea::new(&self.twofa_hint).h(px(40.)))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Sent to TDLib only — never logged"),
            )
            .child(self.twofa_form_buttons(cx, TwofaView::Change, "Change password"))
    }

    /// Slice A2: disable form — current password, `setPassword` with an
    /// empty new password (schema 1.8.67, line 11434).
    pub(super) fn twofa_disable_body(&self, cx: &mut Context<Self>, body: Div) -> Div {
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
        .child(Textarea::new(&self.twofa_current_password).h(px(40.)))
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("Sent to TDLib only — never logged"),
        )
        .child(self.twofa_form_buttons(cx, TwofaView::Disable, "Turn off"))
    }

    /// Slice A2: recovery-email form — current password + new address
    /// (`setRecoveryEmailAddress`, schema 1.8.67, line 11458).
    pub(super) fn twofa_email_body(&self, cx: &mut Context<Self>, body: Div) -> Div {
        body.child(div().font_semibold().text_sm().child("Recovery email"))
            .child(
                div()
                    .mt_1()
                    .font_semibold()
                    .text_sm()
                    .child("Current password"),
            )
            .child(Textarea::new(&self.twofa_current_password).h(px(40.)))
            .child(
                div()
                    .mt_1()
                    .font_semibold()
                    .text_sm()
                    .child("New recovery email"),
            )
            .child(Textarea::new(&self.twofa_email).h(px(40.)))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("The change stays pending until the new address is confirmed."),
            )
            .child(self.twofa_form_buttons(cx, TwofaView::Email, "Save"))
    }

    /// Slice A2: the shared submit/back row for the 2FA forms. Submit
    /// dispatches to the password or email round-trip; back returns to
    /// the status screen without sending anything. A local validation
    /// notice renders above the buttons when a submit was refused.
    pub(super) fn twofa_form_buttons(
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
            TwofaView::Status => unreachable!("status view has no submit buttons"),
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .when_some(self.twofa_notice.clone(), |this, note| {
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
    pub(super) fn toggle_session_secret_chats(&mut self, session_id: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            match live
                .driver
                .toggle_session_can_accept_secret_chats(session_id)
            {
                Ok(_) => self.status_note = "Updating session setting…".into(),
                Err(_) => self.status_note = "Could not update the session setting.".into(),
            }
        }
        cx.notify();
    }

    /// Slice A4: flip `toggleSessionCanAcceptCalls` for one session — the
    /// `toggle_session_secret_chats` twin.
    pub(super) fn toggle_session_calls(&mut self, session_id: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            match live.driver.toggle_session_can_accept_calls(session_id) {
                Ok(_) => self.status_note = "Updating session setting…".into(),
                Err(_) => self.status_note = "Could not update the session setting.".into(),
            }
        }
        cx.notify();
    }

    /// Slice A3: the terminate confirmation banner (the
    /// `delete_confirm_banner` pattern) — TGX `TerminateSessionQuestion` /
    /// `TerminateIncompleteSessionQuestion` / `AreYouSureSessions`,
    /// verbatim.
    pub(super) fn sessions_confirm_banner(
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
    pub(super) fn session_row(
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
                    .danger()
                    .disabled(mutating)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.begin_terminate_session(session_id, incomplete, cx);
                    })),
            );
        }
        if show_toggles || !s.is_current {
            row = row.child(actions);
        }
        row.into_any_element()
    }

    /// Slice A4: open the Connected Websites overlay. Live: guarded fetch
    /// of the authoritative `getConnectedWebsites` answer (cached state
    /// reused, in-flight fetch deduped). Demo: the fixture is already
    /// injected.
    pub(super) fn open_websites(&mut self, cx: &mut Context<Self>) {
        self.websites_open = true;
        self.websites_confirm = None;
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.maybe_fetch_connected_websites();
        }
        cx.notify();
    }

    /// Slice A4: close the overlay and drop any pending disconnect
    /// confirmation.
    pub(super) fn close_websites(&mut self, cx: &mut Context<Self>) {
        self.websites_open = false;
        self.websites_confirm = None;
        cx.notify();
    }

    /// Slice A4: refresh the list — live drops the cache so the guarded
    /// fetch refires; demo re-injects the fixture.
    pub(super) fn refresh_websites(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.connected_websites = None;
            live.driver.session.websites_stale = false;
            let _ = live.driver.maybe_fetch_connected_websites();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.connected_websites = Some(demo_websites());
            session.connected_websites_loading = false;
            session.websites_error = None;
        }
        cx.notify();
    }

    /// Slice A4: arm the disconnect confirmation for one website (TGX
    /// `TerminateWebSessionQuestion` "Disconnect %1$s?").
    pub(super) fn begin_disconnect_website(&mut self, website_id: i64, cx: &mut Context<Self>) {
        self.websites_confirm = Some(WebsitesConfirm::DisconnectOne { website_id });
        cx.notify();
    }

    /// Slice A4: arm the "disconnect all websites" confirmation (TGX
    /// `DisconnectAllWebsitesHint` "Are you sure you want to disconnect
    /// all websites?").
    pub(super) fn begin_disconnect_all_websites(&mut self, cx: &mut Context<Self>) {
        self.websites_confirm = Some(WebsitesConfirm::DisconnectAll);
        cx.notify();
    }

    /// Slice A4: drop the pending disconnect confirmation.
    pub(super) fn cancel_websites_confirm(&mut self, cx: &mut Context<Self>) {
        self.websites_confirm = None;
        cx.notify();
    }

    /// Slice A4: send the confirmed disconnect. Live only — the demo has
    /// no TDLib; the list refreshes from the authoritative `ok` answer,
    /// never optimistically.
    pub(super) fn confirm_websites_disconnect(&mut self, cx: &mut Context<Self>) {
        let confirm = self.websites_confirm.take();
        if let (Some(live), Some(confirm)) = (self.live.as_mut(), confirm) {
            let result = match confirm {
                WebsitesConfirm::DisconnectOne { website_id } => {
                    live.driver.disconnect_website(website_id).map(|_| ())
                }
                WebsitesConfirm::DisconnectAll => live.driver.disconnect_all_websites().map(|_| ()),
            };
            self.status_note = match result {
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
    pub(super) fn websites_confirm_banner(
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
    pub(super) fn website_row(
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
                        .danger()
                        .disabled(mutating)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.begin_disconnect_website(website_id, cx);
                        })),
                ),
            )
            .into_any_element()
    }
}
