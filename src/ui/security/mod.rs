//! security.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::demo::{demo_sessions, demo_storage_stats, demo_websites};
use super::shell::{DialogKind, QuillShell};
use super::*;
use crate::ui::settings_window::rows::{danger_row, group};
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::input::{Input, InputContentType};
use gpui_kit::component::table::{Table, TableBody, TableRow};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::key_fingerprint;
use quill::lang::{keys, tr, tr_with};
use quill::state::fetch::Fetch;
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
            let loading = session.is_some_and(|s| s.auth_state.password_state_loading());
            let error = session
                .as_ref()
                .and_then(|s| s.auth_state.password_op_error.clone());
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
                        .child(tr_with(keys::Q_PREFS_ERROR, &[("line", &line.to_string())])),
                );
            }
            body = match (this.twofa.view, state) {
                (TwofaView::Status, None) => body.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(if loading {
                            tr(keys::Q_PREFS_LOADING)
                        } else {
                            tr(keys::Q_PREFS_NO_TWO_STEP_VERIFICATION_DATA)
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
                    .label(tr(keys::CLOSE))
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_twofa(window, cx);
                        this.close_kit_dialog_if_done(DialogKind::TwoFa, window, cx);
                    })),
            );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(
                    keys::SETTINGS_CLOUD_PASSWORD_START_TITLE,
                )))
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

    /// The Devices settings page: link a device, the sessions grouped like
    /// tdesktop (this device, incomplete logins, others), terminate all and
    /// the auto-terminate period; a tapped session shows its details.
    pub(crate) fn sessions_page(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let this = self;
        {
            let session = this.session();
            let sessions = session
                .as_ref()
                .and_then(|s| s.settings.sessions.data().cloned())
                .unwrap_or_default();
            let loading = session.is_some_and(|s| s.settings.sessions.is_loading());
            let mutating = session.is_some_and(|s| s.settings.sessions_mutating);
            let stale = session.is_some_and(|s| s.settings.sessions_stale);
            let error = session
                .as_ref()
                .and_then(|s| s.settings.sessions.error().map(str::to_string));
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

            let mut body = div().flex().flex_col().gap_4();
            if let Some(notice) = this.privacy.device_link_notice {
                body = body.child(
                    div()
                        .id("device-link-notice")
                        .role(Role::Status)
                        .aria_label(notice)
                        .text_sm()
                        .child(notice),
                );
            }
            if this.privacy.device_login_qr.is_some() {
                body = body.child(
                    this.setting_spot(
                        &["devices.link_device"],
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .id("device-login-consent")
                                    .role(Role::Label)
                                    .aria_label(tr(keys::Q_PREFS_ALLOW_THE_DEVICE_DISPLAYING_THIS))
                                    .child(tr(keys::Q_PREFS_ALLOW_THE_DEVICE_DISPLAYING_THIS)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(
                                        Button::new("confirm-device-login")
                                            .label(tr(keys::Q_PREFS_LINK_DEVICE))
                                            .disabled(mutating)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.confirm_scanned_device(cx)
                                            })),
                                    )
                                    .child(
                                        Button::new("cancel-device-login")
                                            .label(tr(keys::CANCEL))
                                            .ghost()
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.clear_device_qr();
                                                cx.notify();
                                            })),
                                    ),
                            ),
                        cx,
                    ),
                );
            } else if this.privacy.device_qr_scanner.is_some() {
                body = body.child(
                    this.setting_spot(
                        &["devices.link_device"],
                        Button::new("cancel-device-scan")
                            .label(tr(keys::Q_PREFS_CANCEL_CAMERA_SCAN))
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.clear_device_qr();
                                cx.notify();
                            })),
                        cx,
                    ),
                );
            } else {
                // Cross-platform: paste the `tg://login?token=...` link that
                // a QR reader shows for the other device's code.
                let mut row = div().flex().flex_wrap().gap_2();
                if cfg!(target_os = "macos") {
                    row = row.child(
                        Button::new("scan-device-login")
                            .label(tr(keys::Q_PREFS_LINK_DEVICE_WITH_CAMERA))
                            .disabled(mutating)
                            .on_click(cx.listener(|this, _, _, cx| this.scan_device_qr(cx))),
                    );
                }
                body = body.child(
                    this.setting_spot(
                        &["devices.link_device"],
                        row.child(
                            Button::new("paste-device-login")
                                .label(tr(keys::Q_PREFS_LINK_DEVICE_FROM_PASTED_LINK))
                                .ghost()
                                .disabled(mutating)
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.paste_device_login_link(cx)),
                                ),
                        ),
                        cx,
                    ),
                );
            }

            if let Some(line) = error {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(danger())
                        .child(tr_with(keys::Q_PREFS_ERROR, &[("line", &line.to_string())])),
                );
            }
            if sessions.is_empty() {
                body = body.child(
                    this.setting_spot(
                        &[
                            "devices.sessions",
                            "devices.current_session",
                            "devices.accept_secret_chats",
                            "devices.accept_calls",
                            "devices.rename_device",
                            "devices.incomplete_logins",
                            "devices.terminate_session",
                            "devices.terminate_others",
                        ],
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(if loading {
                                tr(keys::Q_PREFS_LOADING)
                            } else {
                                tr(keys::Q_PREFS_NO_SESSION_DATA_YET)
                            }),
                        cx,
                    ),
                );
            } else {
                // tdesktop `SessionsContent`: this device with "Terminate
                // all other sessions" under it, the incomplete login
                // attempts, then the other devices, each a group of rows.
                let mut list = div().flex().flex_col().gap_4();
                // A terminate just landed: the old list stays visible while
                // the authoritative refetch is in flight (never an optimistic
                // delete).
                if stale && loading {
                    list = list.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(keys::Q_PREFS_REFRESHING)),
                    );
                }
                let has_current = current.is_some();
                let any_other = sessions.iter().any(|s| !s.is_current);
                if let Some(current) = current {
                    let mut rows = vec![this.session_row(current, mutating, cx)];
                    if any_other {
                        rows.push(this.setting_spot(
                            &["devices.terminate_others"],
                            danger_row(
                                "terminate-all-sessions",
                                IconName::LogOut,
                                tr(keys::SESSIONS_TERMINATE_ALL),
                                cx.listener(|this, _, _, cx| this.begin_terminate_all_sessions(cx)),
                                cx,
                            ),
                            cx,
                        ));
                    }
                    list = list.child(this.setting_spot(
                        &[
                            "devices.current_session",
                            "devices.accept_secret_chats",
                            "devices.accept_calls",
                            "devices.rename_device",
                        ],
                        group(Some(tr(keys::Q_PREFS_CURRENT_SESSION)), rows),
                        cx,
                    ));
                }
                let has_incomplete = !incomplete.is_empty();
                if has_incomplete {
                    let rows: Vec<AnyElement> = incomplete
                        .into_iter()
                        .map(|s| this.session_row(s, mutating, cx))
                        .collect();
                    let section = div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(group(
                            Some(tr(keys::Q_PREFS_INCOMPLETE_LOGIN_ATTEMPTS)),
                            rows,
                        ))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(tr(keys::Q_PREFS_THESE_DEVICES_HAVE_NO_ACCESS)),
                        );
                    list =
                        list.child(this.setting_spot(&["devices.incomplete_logins"], section, cx));
                }
                let has_others = !others.is_empty();
                if has_others {
                    let rows: Vec<AnyElement> = others
                        .into_iter()
                        .map(|s| this.session_row(s, mutating, cx))
                        .collect();
                    list = list.child(this.setting_spot(
                        &["devices.terminate_session"],
                        group(Some(tr(keys::Q_PREFS_OTHER_SESSIONS)), rows),
                        cx,
                    ));
                }
                let list = this.setting_spot(&["devices.sessions"], list, cx);
                let list = this.setting_stand_in(
                    !has_current,
                    &[
                        "devices.current_session",
                        "devices.accept_secret_chats",
                        "devices.accept_calls",
                        "devices.rename_device",
                    ],
                    list,
                    cx,
                );
                let list = this.setting_stand_in(
                    !has_incomplete,
                    &["devices.incomplete_logins"],
                    list,
                    cx,
                );
                let list =
                    this.setting_stand_in(!has_others, &["devices.terminate_session"], list, cx);
                let list = this.setting_stand_in(
                    !(has_current && any_other),
                    &["devices.terminate_others"],
                    list,
                    cx,
                );
                body = body.child(list);
            }
            let ttl = this.sessions_ttl_section(cx);
            body.child(this.setting_spot(&["devices.terminate_old"], ttl, cx))
                .child(
                    div().flex().child(
                        Button::new("sessions-refresh")
                            .label(tr(keys::Q_PREFS_REFRESH))
                            .outline()
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| this.refresh_sessions(cx))),
                    ),
                )
                .into_any_element()
        }
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
                    .child(tr(keys::Q_PREFS_ENCRYPTION_KEY)),
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
                        let name = Self::secret_peer_name(
                            self.session(),
                            record.user_id,
                            tr(keys::Q_PREFS_YOUR_CONTACT),
                        );
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
                        .child(tr(keys::Q_PREFS_ENCRYPTION_KEY_STILL_LOADING)),
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
        if quill::agent::ui_driver::guard::refuse_hard_if_injecting(
            quill::agent::ui_driver::guard::HardSurface::Security,
        ) {
            return;
        }
        self.twofa.open = true;
        self.open_box(DialogKind::TwoFa);
        self.goto_twofa_view(TwofaView::Status);
        // The last change's error belongs to the box it was shown in; a
        // cached state skips the fetch that would clear it. One still in
        // flight keeps its answer's line.
        if let Some(auth) = self.session_mut().map(|session| &mut session.auth_state)
            && !auth.password_state_loading()
        {
            auth.password_op_error = None;
        }
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.fetch_password_state();
        }
        cx.notify();
    }

    /// Slice A3: open the Active Sessions overlay. Live: guarded fetch of
    /// the authoritative `getActiveSessions` answer (cached state reused,
    /// in-flight fetch deduped). Demo: the fixture is already injected.
    pub(super) fn open_sessions(&mut self, cx: &mut Context<Self>) {
        if quill::agent::ui_driver::guard::refuse_hard_if_injecting(
            quill::agent::ui_driver::guard::HardSurface::Settings,
        ) {
            return;
        }
        self.open_settings(Some(quill::settings_catalog::Page::Devices), cx);
        self.fetch_sessions();
        cx.notify();
    }

    /// Entering the Devices page: a clean list and a guarded fetch.
    pub(crate) fn fetch_sessions(&mut self) {
        self.privacy.sessions_confirm = None;
        self.privacy.extra.session_details = None;
        self.privacy.extra.renaming_device = false;
        // An earlier visit's failure line goes: with a list held, the
        // guarded fetch below sends nothing that would replace it.
        if let Some(session) = self.session_mut() {
            session.settings.sessions.clear_error();
        }
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.maybe_fetch_active_sessions();
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.settings
                .privacy_data
                .inactive_session_ttl_days
                .get_or_insert(180);
        }
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
            .status_check_password
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

    /// Leaving the Devices page stops a camera scan and drops a pending
    /// terminate confirmation.
    pub(crate) fn leave_devices(&mut self) {
        self.clear_device_qr();
        self.privacy.sessions_confirm = None;
        self.privacy.extra.session_details = None;
        self.privacy.extra.renaming_device = false;
    }

    /// Something over the Devices list that Esc peels first.
    /// The details box counts only while its session is in the list: one
    /// terminated from another device closes the box, and Esc must not
    /// spend itself on the invisible layer.
    pub(crate) fn sessions_layer_open(&self) -> bool {
        self.privacy.sessions_confirm.is_some() || self.details_session().is_some()
    }

    /// Esc on the Devices page: the confirmation first, then the details.
    pub(crate) fn close_sessions_top(&mut self, cx: &mut Context<Self>) {
        if self.privacy.sessions_confirm.take().is_none() {
            self.privacy.extra.session_details = None;
            self.privacy.extra.renaming_device = false;
        }
        cx.notify();
    }

    /// Slice A3: refresh the list — live drops the cache so the guarded
    /// fetch refires; demo re-injects the fixture.
    pub(super) fn refresh_sessions(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.settings.sessions = Fetch::Idle;
            live.driver.session.settings.sessions_stale = false;
            let _ = live.driver.maybe_fetch_active_sessions();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.settings.sessions = Fetch::Ready(demo_sessions());
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
    /// after the driver took the request; the status view shows TDLib's
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
            Some(tr(keys::Q_PREFS_ENTER_A_NEW_PASSWORD))
        } else if !include_email && current.is_empty() {
            Some(tr(keys::Q_PREFS_ENTER_YOUR_CURRENT_PASSWORD))
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
        // The form stays until the driver took the request: one it refused
        // (not connected, shutting down) keeps what was typed and says so.
        let sent = match self.live.as_mut() {
            Some(live) => {
                let email_opt = if include_email && !email.is_empty() {
                    Some(email.as_str())
                } else {
                    None
                };
                live.driver
                    .set_two_step_password(&current, &new, &hint, email_opt)
                    .is_ok()
            }
            None => true,
        };
        current.zeroize();
        new.zeroize();
        if !sent {
            self.twofa.notice = Some(tr(keys::Q_OPS_REQUEST_NOT_SENT).to_string());
            cx.notify();
            return;
        }
        self.twofa.notice = None;
        self.clear_twofa_inputs(window, cx);
        self.goto_twofa_view(TwofaView::Status);
        cx.notify();
    }

    /// Empty every field of the two-step forms.
    fn clear_twofa_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
            Some(tr(keys::Q_PREFS_ENTER_YOUR_CURRENT_PASSWORD))
        } else if email.is_empty() {
            Some(tr(keys::Q_PREFS_ENTER_THE_NEW_RECOVERY_EMAIL))
        } else {
            None
        };
        if let Some(note) = missing {
            self.twofa.notice = Some(note.to_string());
            current.zeroize();
            cx.notify();
            return;
        }
        let sent = match self.live.as_mut() {
            Some(live) => live.driver.set_recovery_email(&current, &email).is_ok(),
            None => true,
        };
        current.zeroize();
        if !sent {
            self.twofa.notice = Some(tr(keys::Q_OPS_REQUEST_NOT_SENT).to_string());
            cx.notify();
            return;
        }
        self.twofa.notice = None;
        self.clear_twofa_inputs(window, cx);
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
        self.open_box(DialogKind::SessionsConfirm);
        cx.notify();
    }

    /// Slice A3: arm the "terminate all other sessions" confirmation
    /// (TGX `AreYouSureSessions`).
    pub(super) fn begin_terminate_all_sessions(&mut self, cx: &mut Context<Self>) {
        self.privacy.sessions_confirm = Some(SessionsConfirm::TerminateAll);
        self.open_box(DialogKind::SessionsConfirm);
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
            self.set_note(match result {
                Ok(()) => StatusNote::silent(tr(keys::Q_PREFS_TERMINATING_SESSION)),
                Err(_) => StatusNote::error(tr(keys::Q_PREFS_COULD_NOT_TERMINATE_THE_SESSION)),
            });
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
    )
    .close(|app, _, cx| app.close_websites(cx)),

    TwoFa => DialogSpec::new(
        800,
        |app| app.twofa.open,
        QuillApp::build_twofa_dialog,
    )

    .close(|app, window, cx| app.close_twofa(window, cx)),
}

mod passkey_ceremony;
mod passkey_login;
pub(in crate::ui) mod passkeys;
mod session_list;
mod twofa_status_body;
mod websites_dialog;
