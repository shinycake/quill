//! Proxy settings UI (`parity:proxy-settings`): the proxy list box, the
//! add / edit box, the `tg://proxy` link confirmation, and the shield
//! in the sidebar header / connection strip. Modeled on tdesktop's
//! `ProxiesBox`, `ProxyBox` and `ShowApplyConfirmation`
//! (`boxes/connection_box.cpp`); the data lives in TDLib, so everything
//! here is a view over `Session::proxy` plus driver calls.
#![deny(clippy::indexing_slicing)]
use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Input, InputContentType, InputState};
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::calls::proxy::ProxyKind;
use quill::lang::{keys, tr, tr_with};
use quill::proxy::{
    AUTO_SWITCH_TIMEOUTS, PingStatus, ProxyDraft, ProxyEntry, ProxyIssue, ProxyMode, kind_label,
    parse_proxy_link, proxy_link_from_text,
};
use quill::state::LINK_PING_ID;
use quill::telegram::envelope::ConnectionState;
use std::cell::RefCell;
use std::rc::Rc;

/// All proxy-related UI state, in one field of [`QuillApp`].
#[derive(Default)]
pub(crate) struct ProxyUi {
    pub(crate) editor: Option<ProxyEditor>,
    pub(crate) link: Option<ProxyLinkBox>,
    /// One-line result of the last list action ("Proxy List copied…").
    pub(crate) notice: Option<String>,
    /// The "this exposes your IP" warning was confirmed for this run.
    pub(crate) ip_warning_acked: bool,
    /// The "Share proxy with QR code" box.
    pub(crate) qr: Option<super::proxy_qr::ProxyQr>,
}

/// The add / edit box.
pub(crate) struct ProxyEditor {
    id: Option<i32>,
    kind: ProxyKind,
    /// Adding from the "Use custom proxy" radio enables the new proxy.
    enable_on_save: bool,
    http_only: bool,
    comment: String,
    server: Entity<InputState>,
    /// The web proxy's address (host and optional base path).
    web_host: Entity<InputState>,
    port: Entity<InputState>,
    username: Entity<InputState>,
    password: Entity<InputState>,
    secret: Entity<InputState>,
    error: Option<String>,
}

/// The "Proxy Server — Connect Proxy" confirmation for a proxy link.
pub(crate) struct ProxyLinkBox {
    draft: ProxyDraft,
    /// Showing the one-time "this exposes your IP" warning.
    warn: bool,
}

/// Editor validation message (tdesktop highlights the bad field; Quill
/// says what is wrong in one line).
pub(crate) fn editor_issue_message(kind: ProxyKind, issue: ProxyIssue) -> String {
    match (issue, kind) {
        (ProxyIssue::Invalid, ProxyKind::Web) => tr(keys::Q_PREFS_ENTER_A_WEB_PROXY_ADDRESS).into(),
        (ProxyIssue::Invalid, ProxyKind::Mtproto) => tr(keys::Q_PREFS_ENTER_A_SERVER_A_PORT).into(),
        (ProxyIssue::Invalid, _) => tr(keys::Q_PREFS_ENTER_A_SERVER_AND_A).into(),
        (other, _) => other.message().into(),
    }
}

fn input(
    window: &mut Window,
    cx: &mut Context<QuillApp>,
    placeholder: &'static str,
    value: &str,
    masked: bool,
) -> Entity<InputState> {
    let value = value.to_string();
    cx.new(|cx| {
        InputState::new(window, cx)
            .masked(masked)
            .placeholder(placeholder)
            .default_value(value)
            .submit_on_enter(false)
    })
}

impl QuillApp {
    /// Settings → Connection, with the proxy list freshly fetched.
    pub(crate) fn open_proxy_list(&mut self, cx: &mut Context<Self>) {
        self.open_settings(Some(quill::settings_catalog::Page::Connection));
        self.fetch_proxy_list();
        cx.notify();
    }

    pub(crate) fn fetch_proxy_list(&mut self) {
        self.settings.proxy.notice = None;
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.fetch_proxies();
        }
    }

    /// Run a driver call; demo sessions have no driver, so say so.
    fn proxy_driver<R>(
        &mut self,
        f: impl FnOnce(&mut quill::connect::ConnectDriver<quill::connect::LiveSender>) -> R,
    ) -> Option<R> {
        match self.live.as_mut() {
            Some(live) => Some(f(&mut live.driver)),
            None => {
                self.connection.set_note(StatusNote::silent(tr(
                    keys::Q_PREFS_DEMO_PROXY_SETTINGS_ARE_NOT,
                )));
                None
            }
        }
    }

    pub(super) fn open_proxy_editor(
        &mut self,
        id: Option<i32>,
        enable_on_save: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let existing = id.and_then(|id| {
            self.session()
                .and_then(|s| s.settings.proxy.find(id).cloned())
        });
        let (kind, draft, comment) = match &existing {
            Some(entry) => (entry.proxy.kind, entry.proxy.clone(), entry.comment.clone()),
            None => (
                ProxyKind::Socks5,
                ProxyDraft::empty(ProxyKind::Socks5),
                String::new(),
            ),
        };
        let port = if draft.port == 0 {
            String::new()
        } else {
            draft.port.to_string()
        };
        self.settings.proxy.editor = Some(ProxyEditor {
            id: existing.as_ref().map(|e| e.id),
            kind,
            enable_on_save,
            http_only: draft.http_only,
            comment,
            server: input(window, cx, tr(keys::Q_PREFS_HOSTNAME), &draft.server, false),
            web_host: input(
                window,
                cx,
                tr(keys::PROXY_WEB_HOST_PH),
                &if kind == ProxyKind::Web {
                    draft.display_address()
                } else {
                    String::new()
                },
                false,
            ),
            port: input(window, cx, tr(keys::PROXY_BOX_PORT), &port, false),
            username: input(
                window,
                cx,
                tr(keys::PROXY_BOX_USERNAME),
                &draft.username,
                false,
            ),
            password: input(
                window,
                cx,
                tr(keys::PROXY_BOX_PASSWORD),
                &draft.password,
                true,
            ),
            secret: input(window, cx, tr(keys::PROXY_BOX_SECRET), &draft.secret, false),
            error: None,
        });
        if let Some(editor) = &self.settings.proxy.editor {
            let first = if kind == ProxyKind::Web {
                &editor.web_host
            } else {
                &editor.server
            };
            first.update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    fn close_proxy_editor(&mut self, cx: &mut Context<Self>) {
        self.settings.proxy.editor = None;
        cx.notify();
    }

    /// Validate the editor and send `addProxy` / `editProxy`.
    fn save_proxy_editor(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.settings.proxy.editor.as_ref() else {
            return;
        };
        let mut draft = if editor.kind == ProxyKind::Web {
            ProxyDraft::web(
                editor.web_host.read(cx).value().as_ref(),
                editor.secret.read(cx).value().as_ref(),
            )
        } else {
            ProxyDraft::empty(editor.kind)
        };
        if editor.kind != ProxyKind::Web {
            draft.server = editor.server.read(cx).value().to_string();
            draft.port = editor.port.read(cx).value().trim().parse().unwrap_or(0);
        }
        draft.http_only = editor.http_only;
        if editor.kind == ProxyKind::Mtproto {
            draft.secret = editor.secret.read(cx).value().to_string();
        } else if editor.kind != ProxyKind::Web {
            draft.username = editor.username.read(cx).value().to_string();
            draft.password = editor.password.read(cx).value().to_string();
        }
        let (id, enable_on_save, kind, comment) = (
            editor.id,
            editor.enable_on_save,
            editor.kind,
            editor.comment.clone(),
        );
        let fail = |this: &mut Self, message: String| {
            if let Some(editor) = this.settings.proxy.editor.as_mut() {
                editor.error = Some(message);
            }
        };
        let draft = match draft.validated() {
            Ok(draft) => draft,
            Err(issue) => {
                fail(self, editor_issue_message(kind, issue));
                cx.notify();
                return;
            }
        };
        let duplicate = self.session().is_some_and(|s| {
            s.settings
                .proxy
                .entries()
                .iter()
                .any(|p| Some(p.id) != id && p.proxy.same_endpoint(&draft))
        });
        if duplicate {
            fail(
                self,
                tr(keys::PROXY_ADD_FROM_CLIPBOARD_EXISTING_TOAST).into(),
            );
            cx.notify();
            return;
        }
        let was_enabled = id.is_some_and(|id| {
            self.session()
                .and_then(|s| s.settings.proxy.find(id))
                .is_some_and(|p| p.is_enabled)
        });
        if was_enabled && kind == ProxyKind::Web {
            fail(
                self,
                tr(keys::Q_PREFS_SWITCH_THE_PROXY_OFF_BEFORE_WEB).into(),
            );
            cx.notify();
            return;
        }
        let sent = self.proxy_driver(|driver| match id {
            Some(id) => driver.edit_proxy(id, &draft, was_enabled, &comment),
            None => driver.add_proxy(&draft, enable_on_save, &comment),
        });
        match sent {
            Some(Ok(_)) | None => self.settings.proxy.editor = None,
            Some(Err(_)) => fail(self, tr(keys::Q_PREFS_COULDNT_SAVE_THE_PROXY_TRY).into()),
        }
        cx.notify();
    }

    fn enable_proxy_entry(&mut self, id: i32, cx: &mut Context<Self>) {
        let web = self
            .session()
            .and_then(|s| s.settings.proxy.find(id))
            .is_some_and(|p| !p.proxy.kind.can_be_enabled());
        if web {
            self.settings.proxy.notice =
                Some(tr(keys::Q_PREFS_WEB_PROXIES_ARENT_SUPPORTED_YET).into());
            cx.notify();
            return;
        }
        let _ = self.proxy_driver(|driver| driver.enable_proxy(id));
        cx.notify();
    }

    /// Radios "Disable proxy" (`use_system` false) and "Use system proxy
    /// settings" (true).
    fn set_proxy_mode(&mut self, use_system: bool, cx: &mut Context<Self>) {
        let _ = self.proxy_driver(|driver| driver.set_proxy_mode(use_system));
        cx.notify();
    }

    /// Deleting the proxy in use falls back to the system proxy settings,
    /// as tdesktop's `ProxiesBoxController::setDeleted` does.
    fn delete_proxy_entry(&mut self, id: i32, cx: &mut Context<Self>) {
        let _ = self.proxy_driver(|driver| {
            if driver.session.settings.proxy.enabled().map(|p| p.id) == Some(id) {
                driver.session.settings.proxy.prefs.use_system = true;
                let _ = driver.save_proxy_prefs();
            }
            driver.remove_proxy(id)
        });
        cx.notify();
    }

    fn ping_proxy_entry(&mut self, id: i32, cx: &mut Context<Self>) {
        let _ = self.proxy_driver(|driver| driver.ping_listed_proxy(id));
        cx.notify();
    }

    fn copy_proxy_link(&mut self, id: i32, cx: &mut Context<Self>) {
        let link = self
            .session()
            .and_then(|s| s.settings.proxy.find(id))
            .and_then(|p| p.proxy.share_link(false));
        match link {
            Some(link) => {
                cx.write_to_clipboard(ClipboardItem::new_string(link));
                self.settings.proxy.notice = Some(tr(keys::USERNAME_COPIED).into());
            }
            None => {
                self.settings.proxy.notice =
                    Some(tr(keys::Q_PREFS_HTTP_PROXIES_CANT_BE_SHARED).into());
            }
        }
        cx.notify();
    }

    /// tdesktop "Share Proxy List": every shareable link, one per line.
    fn share_proxy_list(&mut self, cx: &mut Context<Self>) {
        let links: Vec<String> = self
            .session()
            .map(|s| {
                s.settings
                    .proxy
                    .entries()
                    .iter()
                    .filter_map(|p| p.proxy.share_link(false))
                    .collect()
            })
            .unwrap_or_default();
        if links.is_empty() {
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(links.join("\n")));
        self.settings.proxy.notice = Some(tr(keys::PROXY_EDIT_SHARE_LIST_TOAST).into());
        cx.notify();
    }

    /// tdesktop "Add proxy from clipboard".
    fn add_proxy_from_clipboard(&mut self, cx: &mut Context<Self>) {
        let text = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default();
        let notice = match proxy_link_from_text(&text) {
            None => tr(keys::PROXY_ADD_FROM_CLIPBOARD_FAILED_TOAST).to_string(),
            Some(Err(issue)) => issue.message().to_string(),
            Some(Ok(draft)) => {
                let exists = self.session().is_some_and(|s| {
                    s.settings
                        .proxy
                        .entries()
                        .iter()
                        .any(|p| p.proxy.same_endpoint(&draft))
                });
                if exists {
                    tr(keys::PROXY_ADD_FROM_CLIPBOARD_EXISTING_TOAST).to_string()
                } else {
                    match self.proxy_driver(|driver| driver.add_proxy(&draft, false, "")) {
                        Some(Ok(_)) => tr(keys::PROXY_ADD_FROM_CLIPBOARD_GOOD_TOAST).to_string(),
                        _ => tr(keys::Q_PREFS_COULDNT_ADD_THE_PROXY_TRY).to_string(),
                    }
                }
            }
        };
        self.settings.proxy.notice = Some(notice);
        cx.notify();
    }

    /// Radio "Use custom proxy": enable the most recently used proxy, or
    /// open the add box when the list is empty.
    fn use_custom_proxy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let preferred = self.session().and_then(|s| {
            s.settings
                .proxy
                .entries()
                .iter()
                .filter(|p| p.proxy.kind.can_be_enabled())
                .max_by_key(|p| p.last_used_date)
                .map(|p| p.id)
        });
        match preferred {
            Some(id) => self.enable_proxy_entry(id, cx),
            None => self.open_proxy_editor(None, true, window, cx),
        }
    }

    fn set_proxy_auto_switch(
        &mut self,
        on: Option<bool>,
        secs: Option<u32>,
        cx: &mut Context<Self>,
    ) {
        let _ = self.proxy_driver(|driver| {
            let prefs = &mut driver.session.settings.proxy.prefs;
            if let Some(on) = on {
                prefs.auto_switch = on;
            }
            if let Some(secs) = secs {
                prefs.auto_switch_secs = secs;
            }
            // The clock restarts with the new settings.
            driver.session.settings.proxy.disconnected_since_ms = None;
            driver.save_proxy_prefs()
        });
        cx.notify();
    }

    /// Entry point for a launch / OS link: show the confirmation box (or
    /// the reason the link is unusable). `false` when it is not a proxy
    /// link.
    pub(super) fn handle_proxy_link(&mut self, link: &str, cx: &mut Context<Self>) -> bool {
        let Some(parsed) = parse_proxy_link(link) else {
            return false;
        };
        match parsed {
            Ok(draft) => {
                self.settings.proxy.link = Some(ProxyLinkBox { draft, warn: false });
                if let Some(live) = self.live.as_mut() {
                    live.driver
                        .session
                        .settings
                        .proxy
                        .pings
                        .remove(&LINK_PING_ID);
                    let _ = live.driver.maybe_fetch_proxies();
                }
            }
            Err(issue) => self.links.deep_link_dialog = Some(issue.message().into()),
        }
        cx.notify();
        true
    }

    fn close_proxy_link(&mut self, cx: &mut Context<Self>) {
        self.settings.proxy.link = None;
        cx.notify();
    }

    /// "Connect Proxy": enable the listed proxy, or add and enable it.
    fn connect_link_proxy(&mut self, cx: &mut Context<Self>) {
        let Some(link) = self.settings.proxy.link.take() else {
            return;
        };
        let existing = self.session().and_then(|s| {
            s.settings
                .proxy
                .entries()
                .iter()
                .find(|p| p.proxy.same_endpoint(&link.draft))
                .map(|p| p.id)
        });
        if existing.is_some() && !link.draft.kind.can_be_enabled() {
            // Nothing to add and nothing to switch to: say so.
            self.settings.proxy.notice =
                Some(tr(keys::PROXY_ADD_FROM_CLIPBOARD_EXISTING_TOAST).into());
            cx.notify();
            return;
        }
        let _ = self.proxy_driver(|driver| match existing {
            Some(id) => driver.enable_proxy(id),
            None => driver.add_proxy(&link.draft, true, ""),
        });
        cx.notify();
    }

    fn check_link_proxy(&mut self, cx: &mut Context<Self>) {
        let acked = self.settings.proxy.ip_warning_acked;
        let Some(link) = self.settings.proxy.link.as_mut() else {
            return;
        };
        link.warn = !acked;
        let draft = link.draft.clone();
        if acked {
            let _ = self.proxy_driver(|driver| driver.ping_link_proxy(&draft));
        }
        cx.notify();
    }

    fn confirm_link_ip_warning(&mut self, cx: &mut Context<Self>) {
        self.settings.proxy.ip_warning_acked = true;
        self.check_link_proxy(cx);
    }

    /// The shield button for the sidebar header: shown while a proxy is
    /// enabled (tdesktop's `proxyButton`); a checked shield once
    /// connected, a plain one while connecting.
    pub(super) fn proxy_shield_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        if !session.settings.proxy.shield() {
            return None;
        }
        let connected = matches!(
            session.connection,
            ConnectionState::Ready | ConnectionState::Updating
        );
        Some(
            Button::new("proxy-shield")
                .icon(if connected {
                    gpui_kit::assets::IconName::ShieldCheck
                } else {
                    gpui_kit::assets::IconName::Shield
                })
                .ghost()
                .tooltip(if connected {
                    tr(keys::Q_PREFS_PROXY_ENABLED)
                } else {
                    tr(keys::CONNECTION_PROXY_CONNECTING)
                })
                .accessibility_label(tr(keys::PROXY_SETTINGS))
                .on_click(cx.listener(|this, _, _, cx| this.open_proxy_list(cx)))
                .into_any_element(),
        )
    }

    /// "Proxy settings" link for the connection strip while connecting,
    /// with tdesktop's proxy shield (`ProxyIcon` in
    /// `window_connecting_widget.cpp`) in front while a proxy is enabled.
    pub(super) fn proxy_strip_link(&self, cx: &mut Context<Self>) -> AnyElement {
        let shield = self
            .session()
            .is_some_and(|s| s.settings.proxy.shield())
            .then(|| {
                Icon::new(gpui_kit::assets::IconName::Shield)
                    .small()
                    .text_color(cx.theme().muted_foreground)
            });
        div()
            .flex()
            .items_center()
            .gap_1()
            .children(shield)
            .child(
                Button::new("proxy-strip-settings")
                    .label(tr(keys::PROXY_SETTINGS))
                    .ghost()
                    .xsmall()
                    .on_click(cx.listener(|this, _, _, cx| this.open_proxy_list(cx))),
            )
            .into_any_element()
    }

    fn switch_row(
        id: &'static str,
        on: bool,
        title: &'static str,
        caption: Option<&'static str>,
        cx: &mut Context<Self>,
        set: impl Fn(&mut Self, bool, &mut Context<Self>) + 'static,
    ) -> impl IntoElement {
        div()
            .id(id)
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .child(
                Switch::new(format!("{id}-switch"))
                    .checked(on)
                    .accessibility_label(title)
                    .on_click(cx.listener(move |this, &on, _, cx| set(this, on, cx))),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(div().text_sm().child(title))
                    .when_some(caption, |this, caption| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(caption),
                        )
                    }),
            )
    }

    /// Settings → Connection: the proxy mode, IPv6, calls through the
    /// proxy, auto-switch, and the proxy list with add and share.
    pub(crate) fn proxy_settings_body(&mut self, cx: &mut Context<Self>) -> Div {
        let this = self;
        let state = this
            .session()
            .map(|s| s.settings.proxy.clone())
            .unwrap_or_default();
        let use_for_calls = this
            .session()
            .is_some_and(|s| s.calls.prefs.use_proxy_for_calls);
        let enabled = state.enabled().is_some();
        let mode = match state.mode() {
            ProxyMode::Disabled => 0,
            ProxyMode::System => 1,
            ProxyMode::Custom => 2,
        };
        let mut body = div().flex().flex_col().gap_2();
        if let Some(error) = state.error.clone() {
            body = body.child(div().text_xs().text_color(danger()).child(tr_with(
                keys::Q_PREFS_ERROR_2,
                &[("error", &error.to_string())],
            )));
        }
        body = body
            .child(
                RadioGroup::vertical("proxy-mode")
                    .selected_index(Some(mode))
                    .children([
                        Radio::new("proxy-mode-off").label(tr(keys::PROXY_DISABLE)),
                        Radio::new("proxy-mode-system").label(tr(keys::PROXY_USE_SYSTEM_SETTINGS)),
                        Radio::new("proxy-mode-custom").label(tr(keys::PROXY_USE_CUSTOM)),
                    ])
                    .on_click(cx.listener(|this, &ix, window, cx| match ix {
                        0 => this.set_proxy_mode(false, cx),
                        1 => this.set_proxy_mode(true, cx),
                        _ => this.use_custom_proxy(window, cx),
                    })),
            )
            .child(Self::switch_row(
                "proxy-ipv6",
                state.prefer_ipv6,
                tr(keys::CONNECTION_TRY_IPV6),
                None,
                cx,
                |this, on, cx| {
                    let _ = this.proxy_driver(|driver| driver.set_prefer_ipv6(on));
                    cx.notify();
                },
            ));
        if state.enabled_supports_calls() {
            body = body.child(Self::switch_row(
                "proxy-calls",
                use_for_calls,
                tr(keys::PROXY_USE_FOR_CALLS),
                Some(tr(keys::Q_PREFS_ONLY_SOCKS5_PROXIES_CAN_CARRY)),
                cx,
                |this, on, cx| this.set_call_pref(|prefs| prefs.use_proxy_for_calls = on, cx),
            ));
        }
        if enabled && state.entries().len() > 1 {
            body = body.child(Self::switch_row(
                "proxy-auto-switch",
                state.prefs.auto_switch,
                tr(keys::PROXY_AUTO_SWITCH),
                None,
                cx,
                |this, on, cx| this.set_proxy_auto_switch(Some(on), None, cx),
            ));
            if state.prefs.auto_switch {
                let current = state.prefs.timeout_secs();
                body = body
                    .child(
                        RadioGroup::horizontal("proxy-auto-switch-timeout")
                            .selected_index(AUTO_SWITCH_TIMEOUTS.iter().position(|t| *t == current))
                            .children(AUTO_SWITCH_TIMEOUTS.iter().map(|secs| {
                                Radio::new(format!("proxy-auto-switch-{secs}"))
                                    .label(format!("{secs} s"))
                            }))
                            .on_click(cx.listener(|this, &ix, _, cx| {
                                if let Some(&timeout) = AUTO_SWITCH_TIMEOUTS.get(ix) {
                                    this.set_proxy_auto_switch(None, Some(timeout), cx);
                                }
                            })),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(text_muted())
                            .child(tr(keys::PROXY_AUTO_SWITCH_ABOUT)),
                    );
            }
        }
        body = body.child(
            div()
                .text_xs()
                .text_color(text_muted())
                .child(tr(keys::PROXY_ABOUT)),
        );
        if state.entries().is_empty() {
            body = body.child(div().py_3().text_sm().text_color(text_muted()).child(
                if state.loading {
                    tr(keys::Q_PREFS_LOADING)
                } else {
                    tr(keys::PROXY_DESCRIPTION)
                },
            ));
        } else {
            let mut rows = div().flex().flex_col().gap_1();
            let connected = this.session().is_some_and(|s| {
                matches!(
                    s.connection,
                    ConnectionState::Ready | ConnectionState::Updating
                )
            });
            for entry in state.entries() {
                let ping = state.pings.get(&entry.id).copied();
                let web = (entry.proxy.kind == ProxyKind::Web).then(|| {
                    quill::proxy_web::WebRowState::for_entry(
                        entry.is_enabled,
                        connected,
                        state.carrier_failed == Some(entry.id),
                    )
                });
                rows = rows.child(this.proxy_row(entry, ping, web, cx));
            }
            body = body.child(rows);
        }
        body = body.child(
            div()
                .flex()
                .flex_wrap()
                .gap_2()
                .child(
                    Button::new("proxy-add")
                        .icon(gpui_kit::assets::IconName::Plus)
                        .label(tr(keys::PROXY_ADD))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_proxy_editor(None, false, window, cx);
                        })),
                )
                .child(
                    Button::new("proxy-add-clipboard")
                        .label(tr(keys::PROXY_ADD_FROM_CLIPBOARD))
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.add_proxy_from_clipboard(cx);
                        })),
                )
                .when(
                    state
                        .entries()
                        .iter()
                        .any(|p| p.proxy.share_link(false).is_some()),
                    |this| {
                        this.child(
                            Button::new("proxy-share-list")
                                .icon(gpui_kit::assets::IconName::Copy)
                                .label(tr(keys::PROXY_EDIT_SHARE_LIST_BUTTON))
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.share_proxy_list(cx);
                                })),
                        )
                    },
                ),
        );
        if let Some(notice) = this.settings.proxy.notice.clone() {
            body = body.child(div().text_xs().text_color(text_muted()).child(notice));
        }
        body
    }

    fn editor_field(cx: &mut Context<Self>, label: &'static str, field: impl IntoElement) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(label),
            )
            .child(field)
    }
}

crate::ui::shell::register_dialogs! {
    ProxyEdit => DialogSpec::new(
        // The edit / link boxes open over the list, so they rank first.
        3000,
        |app| app.settings.proxy.editor.is_some(),
        QuillApp::build_proxy_edit_dialog,
    ),

    ProxyLink => DialogSpec::new(
        3100,
        |app| app.settings.proxy.link.is_some(),
        QuillApp::build_proxy_link_dialog,
    ),
}

mod build_proxy_edit_dialog;
mod proxy_row;
mod web_demo;
#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod web_ui_tests;

#[cfg(test)]
mod tests {
    use super::editor_issue_message;
    use super::proxy_row::{row_subtitle, row_title};
    use quill::calls::proxy::ProxyKind;
    use quill::proxy::{PingStatus, ProxyDraft, ProxyEntry, ProxyIssue};

    fn entry() -> ProxyEntry {
        let mut proxy = ProxyDraft::empty(ProxyKind::Mtproto);
        proxy.server = "mt.example".into();
        proxy.port = 443;
        ProxyEntry {
            id: 4,
            last_used_date: 0,
            is_enabled: false,
            comment: String::new(),
            proxy,
        }
    }

    #[test]
    fn rows_show_endpoint_type_and_status() {
        let e = entry();
        assert_eq!(row_title(&e), "mt.example:443");
        assert_eq!(row_subtitle(&e, None, None), "MTPROTO · not tested");
        assert_eq!(
            row_subtitle(&e, Some(PingStatus::Available(42)), None),
            "MTPROTO · available (ping: 42 ms)"
        );
        assert_eq!(
            row_subtitle(&e, Some(PingStatus::Checking), None),
            "MTPROTO · checking…"
        );
        assert_eq!(
            row_subtitle(&e, Some(PingStatus::Unavailable), None),
            "MTPROTO · not available"
        );
    }

    #[test]
    fn editor_messages_name_the_missing_field() {
        assert!(editor_issue_message(ProxyKind::Mtproto, ProxyIssue::Invalid).contains("secret"));
        assert!(!editor_issue_message(ProxyKind::Socks5, ProxyIssue::Invalid).contains("secret"));
        assert_eq!(
            editor_issue_message(ProxyKind::Mtproto, ProxyIssue::Unsupported),
            ProxyIssue::Unsupported.message()
        );
    }
}
