//! security.

use super::app::QuillApp;
use super::demo::{demo_sessions, demo_storage_stats, demo_websites};
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::input::{Input, InputContentType};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::table::{Table, TableBody, TableRow};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
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
            let state = session
                .as_ref()
                .and_then(|s| s.auth_state.password_state.clone());
            let loading = session.is_some_and(|s| s.auth_state.password_state_loading);
            let error = session
                .as_ref()
                .and_then(|s| s.auth_state.password_op_error.clone());
            // A finished recovery / reset / login-email step reports on the
            // status screen.
            if this.twofa_step_finished()
                && matches!(this.twofa.view, TwofaView::Recover | TwofaView::LoginEmail)
            {
                this.twofa.view = TwofaView::Status;
                this.twofa.confirm = None;
            }
            let notice = this.twofa_notice_line();
            let mut body = div().flex().flex_col().gap_2();
            if let Some(line) = notice.filter(|_| this.twofa.view == TwofaView::Status) {
                body = body.child(
                    div()
                        .id("twofa-notice")
                        .role(Role::Status)
                        .aria_label(line.clone())
                        .text_sm()
                        .child(line),
                );
            }
            if let Some(line) = error {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(danger())
                        .child(format!("Error: {line}")),
                );
            }
            body = match (this.twofa.view, state) {
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
                (TwofaView::Recover, _) => this.twofa_recover_body(cx, body),
                (TwofaView::LoginEmail, _) => this.twofa_login_email_body(cx, body),
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
                .title(crate::ui::shell::dialog_title("Two-Step Verification"))
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
                .and_then(|s| s.settings.sessions.clone())
                .unwrap_or_default();
            let loading = session.is_some_and(|s| s.settings.sessions_loading);
            let mutating = session.is_some_and(|s| s.settings.sessions_mutating);
            let stale = session.is_some_and(|s| s.settings.sessions_stale);
            let error = session.as_ref().and_then(|s| s.settings.sessions_error.clone());
            let current = sessions.iter().find(|s| s.is_current);
            let mut incomplete: Vec<&ParsedSession> = sessions
                .iter()
                .filter(|s| !s.is_current && s.is_password_pending)
                .collect();
            incomplete.sort_by_key(|a| std::cmp::Reverse(a.last_active_date));
            let mut others: Vec<&ParsedSession> = sessions
                .iter()
                .filter(|s| !s.is_current && !s.is_password_pending)
                .collect();
            others.sort_by_key(|a| std::cmp::Reverse(a.last_active_date));

            let mut body = div().flex().flex_col().gap_2();
            if let Some(notice) = this.privacy.device_link_notice {
                body = body.child(div().id("device-link-notice").role(Role::Status).aria_label(notice).text_sm().child(notice));
            }
            if this.privacy.device_login_qr.is_some() {
                body = body.child(div().flex().flex_col().gap_2()
                    .child(div().id("device-login-consent").role(Role::Label)
                        .aria_label("Allow the device displaying this QR code to sign in to your Telegram account? It will have access to your cloud chats.")
                        .child("Allow the device displaying this QR code to sign in to your Telegram account? It will have access to your cloud chats."))
                    .child(div().flex().gap_2()
                        .child(Button::new("confirm-device-login").label("Link device").disabled(mutating)
                            .on_click(cx.listener(|this, _, _, cx| this.confirm_scanned_device(cx))))
                        .child(Button::new("cancel-device-login").label("Cancel").ghost()
                            .on_click(cx.listener(|this, _, _, cx| { this.clear_device_qr(); cx.notify(); })))));
            } else if this.privacy.device_qr_scanner.is_some() {
                body = body.child(Button::new("cancel-device-scan").label("Cancel camera scan").ghost()
                    .on_click(cx.listener(|this, _, _, cx| { this.clear_device_qr(); cx.notify(); })));
            } else {
                // Cross-platform: paste the `tg://login?token=...` link that
                // a QR reader shows for the other device's code.
                let mut row = div().flex().flex_wrap().gap_2();
                if cfg!(target_os = "macos") {
                    row = row.child(Button::new("scan-device-login").label("Link device with camera").disabled(mutating)
                        .on_click(cx.listener(|this, _, _, cx| this.scan_device_qr(cx))));
                }
                body = body.child(row.child(Button::new("paste-device-login").label("Link device from pasted link").ghost().disabled(mutating)
                    .on_click(cx.listener(|this, _, _, cx| this.paste_device_login_link(cx)))));
            }

            if let Some(line) = error {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(danger())
                        .child(format!("Error: {line}")),
                );
            }
            if let Some(confirm) = this.privacy.sessions_confirm {
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
                        .child(Table::new().with_ix(0).accessibility_label("Current session").w_full().child(TableBody::new().child(
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
                                "These devices have no access to your messages: the code was entered correctly, but not the password.",
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
                    body = body.child(Table::new().with_ix(1).accessibility_label("Incomplete login attempts").w_full().child(incomplete_body));
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
                    body = body.child(Table::new().with_ix(2).accessibility_label("Other sessions").w_full().child(others_body));
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
            // B13: a tapped session opens its details instead of the list.
            let details = this
                .privacy.extra
                .session_details
                .and_then(|id| sessions.iter().find(|s| s.id == id))
                .map(|s| this.session_details_body(s, mutating, cx));
            if details.is_none() {
                this.privacy.extra.session_details = None;
                body = body.child(this.sessions_ttl_section(cx));
            }
            let body = match details {
                Some(details) => div().flex().flex_col().gap_2().child(details),
                None => body,
            };
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
                .title(crate::ui::shell::dialog_title("Active Sessions"))
                .content(crate::ui::shell::scrollable_dialog_content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body.borrow_mut().take().unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
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
                .is_some_and(|s| s.settings.connected_websites.is_some());
            let mut websites = session
                .as_ref()
                .and_then(|s| s.settings.connected_websites.clone())
                .unwrap_or_default();
            websites.sort_by_key(|a| std::cmp::Reverse(a.last_active_date));
            let loading = session.is_some_and(|s| s.settings.connected_websites_loading);
            let mutating = session.is_some_and(|s| s.settings.websites_mutating);
            let stale = session.is_some_and(|s| s.settings.websites_stale);
            let error = session.as_ref().and_then(|s| s.settings.websites_error.clone());

            let mut body = div().flex().flex_col().gap_2();
            if let Some(line) = error {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(danger())
                        .child(format!("Error: {line}")),
                );
            }
            if let Some(confirm) = this.privacy.websites_confirm {
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
                body = body.child(Table::new().with_ix(3).accessibility_label("Connected websites").w_full().child(websites_body));
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
                .title(crate::ui::shell::dialog_title("Logged In with Telegram"))
                .content(crate::ui::shell::scrollable_dialog_content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body.borrow_mut().take().unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
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
            session.settings.storage_stats = Some(demo_storage_stats());
            session.settings.storage_stats_loading = false;
        }
        cx.notify();
    }

    /// Slice A2: open the two-step verification overlay. Live: guarded
    /// fetch of the authoritative `passwordState` (cached state reused,
    /// in-flight fetch deduped). Demo: the fixture is already injected.
    pub(super) fn open_twofa(&mut self, cx: &mut Context<Self>) {
        self.twofa.open = true;
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
        self.privacy.sessions_open = true;
        self.privacy.sessions_confirm = None;
        self.privacy.extra.session_details = None;
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.maybe_fetch_active_sessions();
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.settings
                .privacy_data
                .inactive_session_ttl_days
                .get_or_insert(180);
        }
        cx.notify();
    }

    /// Slice A2: close the overlay and clear every 2FA input — passwords
    /// must not linger in the form after the dialog is gone.
    pub(super) fn close_twofa(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.twofa.open = false;
        self.goto_twofa_view(TwofaView::Status);
        self.twofa
            .current_password
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.twofa
            .new_password
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.twofa
            .hint
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.twofa
            .email
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.twofa
            .code
            .update(cx, |input, cx| input.set_value("", window, cx));
    }

    /// Slice A3: close the overlay and drop any pending terminate
    /// confirmation.
    pub(super) fn close_sessions(&mut self, cx: &mut Context<Self>) {
        self.clear_device_qr();
        self.privacy.sessions_open = false;
        self.privacy.sessions_confirm = None;
        cx.notify();
    }

    /// Slice A3: refresh the list — live drops the cache so the guarded
    /// fetch refires; demo re-injects the fixture.
    pub(super) fn refresh_sessions(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.settings.sessions = None;
            live.driver.session.settings.sessions_stale = false;
            let _ = live.driver.maybe_fetch_active_sessions();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.settings.sessions = Some(demo_sessions());
            session.settings.sessions_loading = false;
            session.settings.sessions_error = None;
        }
        cx.notify();
    }

    /// Slice A2: switch the 2FA view; a fresh form starts with no local
    /// notice line.
    pub(super) fn goto_twofa_view(&mut self, view: TwofaView) {
        self.twofa.view = view;
        self.twofa.notice = None;
        self.twofa.confirm = None;
        if view == TwofaView::Status {
            self.clear_twofa_flow();
        } else if let Some(session) = self.live.as_mut().map(|l| &mut l.driver.session) {
            session.auth_state.twofa_flow.notice = None;
        } else if let Some(session) = self.demo_session.as_mut() {
            session.auth_state.twofa_flow.notice = None;
        }
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
        let mut current = self.twofa.current_password.read(cx).value().to_string();
        let mut new = self.twofa.new_password.read(cx).value().to_string();
        let hint = self.twofa.hint.read(cx).value().to_string();
        let email = self.twofa.email.read(cx).value().trim().to_string();
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
            self.twofa.notice = Some(note.to_string());
            current.zeroize();
            new.zeroize();
            cx.notify();
            return;
        }
        self.twofa.notice = None;
        self.twofa
            .current_password
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.twofa
            .new_password
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.twofa
            .hint
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.twofa
            .email
            .update(cx, |input, cx| input.set_value("", window, cx));
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
        let mut current = self.twofa.current_password.read(cx).value().to_string();
        let email = self.twofa.email.read(cx).value().trim().to_string();
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
            self.twofa.notice = Some(note.to_string());
            current.zeroize();
            cx.notify();
            return;
        }
        self.twofa.notice = None;
        self.twofa
            .current_password
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.twofa
            .email
            .update(cx, |input, cx| input.set_value("", window, cx));
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
        self.privacy.sessions_confirm = Some(SessionsConfirm::TerminateOne {
            session_id,
            incomplete,
        });
        cx.notify();
    }

    /// Slice A3: arm the "terminate all other sessions" confirmation
    /// (TGX `AreYouSureSessions`).
    pub(super) fn begin_terminate_all_sessions(&mut self, cx: &mut Context<Self>) {
        self.privacy.sessions_confirm = Some(SessionsConfirm::TerminateAll);
        cx.notify();
    }

    /// Slice A3: drop the pending terminate confirmation.
    pub(super) fn cancel_sessions_confirm(&mut self, cx: &mut Context<Self>) {
        self.privacy.sessions_confirm = None;
        cx.notify();
    }

    /// Slice A3: send the confirmed terminate. Live only — the demo has
    /// no TDLib; the list refreshes from the authoritative `ok` answer,
    /// never optimistically.
    pub(super) fn confirm_sessions_terminate(&mut self, cx: &mut Context<Self>) {
        let confirm = self.privacy.sessions_confirm.take();
        if let (Some(live), Some(confirm)) = (self.live.as_mut(), confirm) {
            let result = match confirm {
                SessionsConfirm::TerminateOne { session_id, .. } => {
                    live.driver.terminate_session(session_id).map(|_| ())
                }
                SessionsConfirm::TerminateAll => {
                    live.driver.terminate_all_other_sessions().map(|_| ())
                }
            };
            self.connection.status_note = match result {
                Ok(()) => "Terminating session…".into(),
                Err(_) => "Could not terminate the session.".into(),
            };
        }
        cx.notify();
    }
}

/// Per-row destructive action: danger-colored text without a filled
/// background (fills are reserved for the final confirmation).
pub(super) fn quiet_danger(cx: &App) -> ButtonCustomVariant {
    ButtonCustomVariant::new(cx)
        .color(gpui_kit::transparent_black())
        .foreground(cx.theme().danger)
        .hover(cx.theme().danger.opacity(0.12))
        .active(cx.theme().danger.opacity(0.2))
}

crate::ui::shell::register_dialogs! {
    Websites => DialogSpec::new(
        600,
        |app| app.privacy.websites_open,
        QuillApp::build_websites_dialog,
    ),

    Sessions => DialogSpec::new(
        700,
        |app| app.privacy.sessions_open,
        QuillApp::build_sessions_dialog,
    ),

    TwoFa => DialogSpec::new(
        800,
        |app| app.twofa.open,
        QuillApp::build_twofa_dialog,
    ),
}

mod twofa_status_body;
