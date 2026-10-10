//! Proxy settings UI (`parity:proxy-settings`): the proxy list box, the
//! add / edit box, the `tg://proxy` link confirmation, and the shield
//! in the sidebar header / connection strip. Modeled on tdesktop's
//! `ProxiesBox`, `ProxyBox` and `ShowApplyConfirmation`
//! (`boxes/connection_box.cpp`); the data lives in TDLib, so everything
//! here is a view over `Session::proxy` plus driver calls.
use super::app::QuillApp;
use super::screenshot_demo::{DemoSpec, register_demos};
use super::shell::{DialogKind, QuillShell};
use super::*;
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
use quill::proxy::{
    AUTO_SWITCH_TIMEOUTS, PingStatus, ProxyDraft, ProxyEntry, ProxyIssue, kind_label,
    parse_proxy_link, proxy_link_from_text,
};
use quill::state::LINK_PING_ID;
use quill::telegram::envelope::ConnectionState;
use std::cell::RefCell;
use std::rc::Rc;

/// All proxy-related UI state, in one field of [`QuillApp`].
#[derive(Default)]
pub(crate) struct ProxyUi {
    pub(crate) list_open: bool,
    pub(crate) editor: Option<ProxyEditor>,
    pub(crate) link: Option<ProxyLinkBox>,
    /// One-line result of the last list action ("Proxy List copied…").
    pub(crate) notice: Option<String>,
    /// The "this exposes your IP" warning was confirmed for this run.
    pub(crate) ip_warning_acked: bool,
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

/// What the list row shows under the title: `SOCKS5 · available (ping:
/// 42 ms)`; untested proxies show only their type.
pub(crate) fn row_subtitle(entry: &ProxyEntry, ping: Option<PingStatus>) -> String {
    let kind = kind_label(entry.proxy.kind);
    match ping {
        Some(status) => format!("{kind} · {}", status.label()),
        None => format!("{kind} · not tested"),
    }
}

pub(crate) fn row_title(entry: &ProxyEntry) -> String {
    format!("{}:{}", entry.proxy.server, entry.proxy.port)
}

/// Editor validation message (tdesktop highlights the bad field; Quill
/// says what is wrong in one line).
pub(crate) fn editor_issue_message(kind: ProxyKind, issue: ProxyIssue) -> String {
    match (issue, kind) {
        (ProxyIssue::Invalid, ProxyKind::Mtproto) => {
            "Enter a server, a port (1–65535) and a valid secret.".into()
        }
        (ProxyIssue::Invalid, _) => "Enter a server and a port (1–65535).".into(),
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
    pub(super) fn open_proxy_list(&mut self, cx: &mut Context<Self>) {
        self.proxy_ui.list_open = true;
        self.proxy_ui.notice = None;
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.fetch_proxies();
        }
        cx.notify();
    }

    fn close_proxy_list(&mut self, cx: &mut Context<Self>) {
        self.proxy_ui.list_open = false;
        self.proxy_ui.notice = None;
        cx.notify();
    }

    /// Run a driver call; demo sessions have no driver, so say so.
    fn proxy_driver<R>(
        &mut self,
        f: impl FnOnce(&mut quill::connect::ConnectDriver<quill::connect::LiveSender>) -> R,
    ) -> Option<R> {
        match self.live.as_mut() {
            Some(live) => Some(f(&mut live.driver)),
            None => {
                self.status_note = "demo: proxy settings are not applied".into();
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
        let existing = id.and_then(|id| self.session().and_then(|s| s.proxy.find(id).cloned()));
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
        self.proxy_ui.editor = Some(ProxyEditor {
            id: existing.as_ref().map(|e| e.id),
            kind,
            enable_on_save,
            http_only: draft.http_only,
            comment,
            server: input(window, cx, "Hostname", &draft.server, false),
            port: input(window, cx, "Port", &port, false),
            username: input(window, cx, "Username", &draft.username, false),
            password: input(window, cx, "Password", &draft.password, true),
            secret: input(window, cx, "Secret", &draft.secret, false),
            error: None,
        });
        if let Some(editor) = &self.proxy_ui.editor {
            editor
                .server
                .update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    fn close_proxy_editor(&mut self, cx: &mut Context<Self>) {
        self.proxy_ui.editor = None;
        cx.notify();
    }

    /// Validate the editor and send `addProxy` / `editProxy`.
    fn save_proxy_editor(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.proxy_ui.editor.as_ref() else {
            return;
        };
        let mut draft = ProxyDraft::empty(editor.kind);
        draft.server = editor.server.read(cx).value().to_string();
        draft.port = editor.port.read(cx).value().trim().parse().unwrap_or(0);
        draft.http_only = editor.http_only;
        if editor.kind == ProxyKind::Mtproto {
            draft.secret = editor.secret.read(cx).value().to_string();
        } else {
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
            if let Some(editor) = this.proxy_ui.editor.as_mut() {
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
            s.proxy
                .entries()
                .iter()
                .any(|p| Some(p.id) != id && p.proxy.same_endpoint(&draft))
        });
        if duplicate {
            fail(self, "This proxy is already in the list.".into());
            cx.notify();
            return;
        }
        let was_enabled = id.is_some_and(|id| {
            self.session()
                .and_then(|s| s.proxy.find(id))
                .is_some_and(|p| p.is_enabled)
        });
        let sent = self.proxy_driver(|driver| match id {
            Some(id) => driver.edit_proxy(id, &draft, was_enabled, &comment),
            None => driver.add_proxy(&draft, enable_on_save, &comment),
        });
        match sent {
            Some(Ok(_)) | None => self.proxy_ui.editor = None,
            Some(Err(_)) => fail(self, "Couldn't save the proxy. Try again.".into()),
        }
        cx.notify();
    }

    fn enable_proxy_entry(&mut self, id: i32, cx: &mut Context<Self>) {
        let _ = self.proxy_driver(|driver| driver.enable_proxy(id));
        cx.notify();
    }

    fn disable_proxy_entry(&mut self, cx: &mut Context<Self>) {
        let _ = self.proxy_driver(|driver| driver.disable_proxy());
        cx.notify();
    }

    fn delete_proxy_entry(&mut self, id: i32, cx: &mut Context<Self>) {
        let _ = self.proxy_driver(|driver| driver.remove_proxy(id));
        cx.notify();
    }

    fn ping_proxy_entry(&mut self, id: i32, cx: &mut Context<Self>) {
        let _ = self.proxy_driver(|driver| driver.ping_listed_proxy(id));
        cx.notify();
    }

    fn copy_proxy_link(&mut self, id: i32, cx: &mut Context<Self>) {
        let link = self
            .session()
            .and_then(|s| s.proxy.find(id))
            .and_then(|p| p.proxy.share_link(false));
        match link {
            Some(link) => {
                cx.write_to_clipboard(ClipboardItem::new_string(link));
                self.proxy_ui.notice = Some("Link copied to clipboard.".into());
            }
            None => {
                self.proxy_ui.notice = Some("HTTP proxies can't be shared as a link.".into());
            }
        }
        cx.notify();
    }

    /// tdesktop "Share Proxy List": every shareable link, one per line.
    fn share_proxy_list(&mut self, cx: &mut Context<Self>) {
        let links: Vec<String> = self
            .session()
            .map(|s| {
                s.proxy
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
        self.proxy_ui.notice = Some("Proxy List copied to clipboard.".into());
        cx.notify();
    }

    /// tdesktop "Add proxy from clipboard".
    fn add_proxy_from_clipboard(&mut self, cx: &mut Context<Self>) {
        let text = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default();
        let notice = match proxy_link_from_text(&text) {
            None => "This is not a proxy link.".to_string(),
            Some(Err(issue)) => issue.message().to_string(),
            Some(Ok(draft)) => {
                let exists = self.session().is_some_and(|s| {
                    s.proxy
                        .entries()
                        .iter()
                        .any(|p| p.proxy.same_endpoint(&draft))
                });
                if exists {
                    "This proxy is already in the list.".to_string()
                } else {
                    match self.proxy_driver(|driver| driver.add_proxy(&draft, false, "")) {
                        Some(Ok(_)) => "Proxy was added from clipboard.".to_string(),
                        _ => "Couldn't add the proxy. Try again.".to_string(),
                    }
                }
            }
        };
        self.proxy_ui.notice = Some(notice);
        cx.notify();
    }

    /// Radio "Use custom proxy": enable the most recently used proxy, or
    /// open the add box when the list is empty.
    fn use_custom_proxy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let preferred = self.session().and_then(|s| {
            s.proxy
                .entries()
                .iter()
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
            let prefs = &mut driver.session.proxy.prefs;
            if let Some(on) = on {
                prefs.auto_switch = on;
            }
            if let Some(secs) = secs {
                prefs.auto_switch_secs = secs;
            }
            // The clock restarts with the new settings.
            driver.session.proxy.disconnected_since_ms = None;
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
                self.proxy_ui.link = Some(ProxyLinkBox { draft, warn: false });
                if let Some(live) = self.live.as_mut() {
                    live.driver.session.proxy.pings.remove(&LINK_PING_ID);
                    let _ = live.driver.maybe_fetch_proxies();
                }
            }
            Err(issue) => self.deep_link_dialog = Some(issue.message().into()),
        }
        cx.notify();
        true
    }

    fn close_proxy_link(&mut self, cx: &mut Context<Self>) {
        self.proxy_ui.link = None;
        cx.notify();
    }

    /// "Connect Proxy": enable the listed proxy, or add and enable it.
    fn connect_link_proxy(&mut self, cx: &mut Context<Self>) {
        let Some(link) = self.proxy_ui.link.take() else {
            return;
        };
        let existing = self.session().and_then(|s| {
            s.proxy
                .entries()
                .iter()
                .find(|p| p.proxy.same_endpoint(&link.draft))
                .map(|p| p.id)
        });
        let _ = self.proxy_driver(|driver| match existing {
            Some(id) => driver.enable_proxy(id),
            None => driver.add_proxy(&link.draft, true, ""),
        });
        cx.notify();
    }

    fn check_link_proxy(&mut self, cx: &mut Context<Self>) {
        let acked = self.proxy_ui.ip_warning_acked;
        let Some(link) = self.proxy_ui.link.as_mut() else {
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
        self.proxy_ui.ip_warning_acked = true;
        self.check_link_proxy(cx);
    }

    /// The shield button for the sidebar header: shown while a proxy is
    /// enabled (tdesktop's `proxyButton`); a checked shield once
    /// connected, a plain one while connecting.
    pub(super) fn proxy_shield_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        if !session.proxy.shield() {
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
                    "Proxy enabled"
                } else {
                    "Connecting through proxy…"
                })
                .accessibility_label("Proxy settings")
                .on_click(cx.listener(|this, _, _, cx| this.open_proxy_list(cx)))
                .into_any_element(),
        )
    }

    /// "Proxy settings" link for the connection strip while connecting.
    pub(super) fn proxy_strip_link(&self, cx: &mut Context<Self>) -> AnyElement {
        Button::new("proxy-strip-settings")
            .label("Proxy settings")
            .ghost()
            .xsmall()
            .on_click(cx.listener(|this, _, _, cx| this.open_proxy_list(cx)))
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

    fn proxy_row(
        &self,
        entry: &ProxyEntry,
        ping: Option<PingStatus>,
        cx: &mut Context<Self>,
    ) -> Div {
        let id = entry.id;
        let status_color = match ping {
            Some(PingStatus::Available(_)) => success(),
            Some(PingStatus::Unavailable) => danger(),
            _ => text_muted(),
        };
        let shareable = entry.proxy.share_link(false).is_some();
        let icon_button = |name: &'static str, icon, tip: &'static str| {
            Button::new(SharedString::from(format!("proxy-{name}-{id}")))
                .icon(icon)
                .ghost()
                .xsmall()
                .tooltip(tip)
                .accessibility_label(tip)
        };
        div()
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .rounded_md()
            .when(entry.is_enabled, |this| this.bg(cx.theme().accent))
            .child(
                div()
                    .id(SharedString::from(format!("proxy-use-{id}")))
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap_2()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if this
                            .session()
                            .and_then(|s| s.proxy.find(id))
                            .is_some_and(|p| p.is_enabled)
                        {
                            this.disable_proxy_entry(cx);
                        } else {
                            this.enable_proxy_entry(id, cx);
                        }
                    }))
                    .child(div().w(px(16.)).flex_none().when(entry.is_enabled, |this| {
                        this.child(Icon::new(gpui_kit::assets::IconName::Check).small())
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(
                                div()
                                    .text_sm()
                                    .font_semibold()
                                    .truncate()
                                    .child(row_title(entry)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(status_color)
                                    .child(row_subtitle(entry, ping)),
                            ),
                    ),
            )
            .child(
                icon_button("ping", gpui_kit::assets::IconName::RotateCw, "Check status")
                    .on_click(cx.listener(move |this, _, _, cx| this.ping_proxy_entry(id, cx))),
            )
            // Unshareable (HTTP) rows keep an invisible slot so the icons align.
            .child(
                div().when(!shareable, |this| this.invisible()).child(
                    icon_button("copy", gpui_kit::assets::IconName::Copy, "Copy link")
                        .disabled(!shareable)
                        .on_click(cx.listener(move |this, _, _, cx| this.copy_proxy_link(id, cx))),
                ),
            )
            .child(
                icon_button("edit", gpui_kit::assets::IconName::Pencil, "Edit").on_click(
                    cx.listener(move |this, _, window, cx| {
                        this.open_proxy_editor(Some(id), false, window, cx)
                    }),
                ),
            )
            .child(
                icon_button("delete", gpui_kit::assets::IconName::Trash, "Delete")
                    .on_click(cx.listener(move |this, _, _, cx| this.delete_proxy_entry(id, cx))),
            )
    }

    pub(super) fn build_proxy_list_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::ProxyList, |this, _, cx| {
                this.close_proxy_list(cx);
            });
        app.update(cx, |this, cx| {
            let state = this
                .session()
                .map(|s| s.proxy.clone())
                .unwrap_or_default();
            let use_for_calls = this
                .session()
                .is_some_and(|s| s.call_prefs.use_proxy_for_calls);
            let enabled = state.enabled().is_some();
            let mut body = div().flex().flex_col().gap_2();
            if let Some(error) = state.error.clone() {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(danger())
                        .child(format!("Error: {error}")),
                );
            }
            body = body
                .child(
                    RadioGroup::vertical("proxy-mode")
                        .selected_index(Some(usize::from(enabled)))
                        .children([
                            Radio::new("proxy-mode-off").label("Disable proxy"),
                            Radio::new("proxy-mode-custom").label("Use custom proxy"),
                        ])
                        .on_click(cx.listener(|this, &ix, window, cx| {
                            if ix == 0 {
                                this.disable_proxy_entry(cx);
                            } else {
                                this.use_custom_proxy(window, cx);
                            }
                        })),
                )
                .child(Self::switch_row(
                    "proxy-ipv6",
                    state.prefer_ipv6,
                    "Try connecting through IPv6",
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
                    "Use proxy for calls",
                    Some("Only SOCKS5 proxies can carry call media"),
                    cx,
                    |this, on, cx| this.set_call_pref(|prefs| prefs.use_proxy_for_calls = on, cx),
                ));
            }
            if enabled && state.entries().len() > 1 {
                body = body.child(Self::switch_row(
                    "proxy-auto-switch",
                    state.prefs.auto_switch,
                    "Auto-switch proxies",
                    None,
                    cx,
                    |this, on, cx| this.set_proxy_auto_switch(Some(on), None, cx),
                ));
                if state.prefs.auto_switch {
                    let current = state.prefs.timeout_secs();
                    body = body
                        .child(
                            RadioGroup::horizontal("proxy-auto-switch-timeout")
                                .selected_index(
                                    AUTO_SWITCH_TIMEOUTS.iter().position(|t| *t == current),
                                )
                                .children(AUTO_SWITCH_TIMEOUTS.iter().map(|secs| {
                                    Radio::new(format!("proxy-auto-switch-{secs}"))
                                        .label(format!("{secs} s"))
                                }))
                                .on_click(cx.listener(|this, &ix, _, cx| {
                                    this.set_proxy_auto_switch(
                                        None,
                                        Some(AUTO_SWITCH_TIMEOUTS[ix]),
                                        cx,
                                    );
                                })),
                        )
                        .child(div().text_xs().text_color(text_muted()).child(
                            "You can choose how quickly the app should auto-connect to the nearest active proxy if the current one stops working.",
                        ));
                }
            }
            body = body.child(div().text_xs().text_color(text_muted()).child(
                "Proxy servers may be helpful in accessing Telegram if there is no connection in a specific region.",
            ));
            if state.entries().is_empty() {
                body = body.child(
                    div()
                        .py_3()
                        .text_sm()
                        .text_color(text_muted())
                        .child(if state.loading {
                            "Loading…"
                        } else {
                            "Your saved proxy list will be here."
                        }),
                );
            } else {
                let mut rows = div().flex().flex_col().gap_1();
                for entry in state.entries() {
                    let ping = state.pings.get(&entry.id).copied();
                    rows = rows.child(this.proxy_row(entry, ping, cx));
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
                            .label("Add proxy")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_proxy_editor(None, false, window, cx);
                            })),
                    )
                    .child(
                        Button::new("proxy-add-clipboard")
                            .label("Add proxy from clipboard")
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
                                    .label("Share Proxy List")
                                    .ghost()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.share_proxy_list(cx);
                                    })),
                            )
                        },
                    ),
            );
            if let Some(notice) = this.proxy_ui.notice.clone() {
                body = body.child(div().text_xs().text_color(text_muted()).child(notice));
            }
            let footer = div().flex().justify_end().child(
                Button::new("proxy-close")
                    .label("Close")
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_proxy_list(cx);
                        this.close_kit_dialog_if_done(DialogKind::ProxyList, window, cx);
                    })),
            );
            dialog
                .overlay(true)
                .width(px(520.))
                .title(crate::ui::shell::dialog_title("Proxy settings"))
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        content.child(
                            body.borrow_mut()
                                .take()
                                .unwrap_or_else(|| div().into_any_element()),
                        )
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
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

    pub(super) fn build_proxy_edit_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::ProxyEdit, |this, _, cx| {
                this.close_proxy_editor(cx);
            });
        app.update(cx, |this, cx| {
            let Some(editor) = this.proxy_ui.editor.as_ref() else {
                return dialog
                    .overlay(true)
                    .title(crate::ui::shell::dialog_title("Edit proxy"))
                    .on_close(on_close);
            };
            let editing = editor.id;
            let kind = editor.kind;
            let error = editor.error.clone();
            let (server, port, username, password, secret) = (
                editor.server.clone(),
                editor.port.clone(),
                editor.username.clone(),
                editor.password.clone(),
                editor.secret.clone(),
            );
            let kinds = [ProxyKind::Socks5, ProxyKind::Http, ProxyKind::Mtproto];
            let mut body = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    RadioGroup::horizontal("proxy-kind")
                        .selected_index(kinds.iter().position(|k| *k == kind))
                        .children(kinds.iter().map(|k| {
                            Radio::new(format!("proxy-kind-{}", kind_label(*k)))
                                .label(kind_label(*k))
                        }))
                        .on_click(cx.listener(move |this, &ix, _, cx| {
                            if let Some(editor) = this.proxy_ui.editor.as_mut() {
                                editor.kind = kinds[ix];
                                editor.error = None;
                            }
                            cx.notify();
                        })),
                )
                .child(Self::editor_field(
                    cx,
                    "Hostname",
                    Input::new(&server).aria_label("Hostname").h(px(40.)),
                ))
                .child(Self::editor_field(
                    cx,
                    "Port",
                    Input::new(&port).aria_label("Port").h(px(40.)),
                ));
            if kind == ProxyKind::Mtproto {
                body = body.child(Self::editor_field(
                    cx,
                    "Secret",
                    Input::new(&secret).aria_label("Secret").h(px(40.)),
                ));
            } else {
                body = body
                    .child(Self::editor_field(
                        cx,
                        "Username (optional)",
                        Input::new(&username).aria_label("Username").h(px(40.)),
                    ))
                    .child(Self::editor_field(
                        cx,
                        "Password (optional)",
                        Input::new(&password)
                            .aria_label("Password")
                            .content_type(InputContentType::Password)
                            .h(px(40.)),
                    ));
            }
            if let Some(error) = error {
                body = body.child(div().text_xs().text_color(danger()).child(error));
            }
            let mut footer = div().flex().items_center().gap_2();
            if let Some(id) = editing {
                footer = footer
                    .child(
                        Button::new("proxy-edit-delete")
                            .label("Delete")
                            .danger()
                            .ghost()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.delete_proxy_entry(id, cx);
                                this.close_proxy_editor(cx);
                                this.close_kit_dialog_if_done(DialogKind::ProxyEdit, window, cx);
                            })),
                    )
                    .child(
                        Button::new("proxy-edit-copy")
                            .label("Copy link")
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.copy_proxy_link(id, cx);
                            })),
                    );
            }
            footer = footer
                .child(div().flex_1())
                .child(
                    Button::new("proxy-edit-cancel")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_proxy_editor(cx);
                            this.close_kit_dialog_if_done(DialogKind::ProxyEdit, window, cx);
                        })),
                )
                .child(
                    Button::new("proxy-edit-save")
                        .label("Save")
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.save_proxy_editor(cx);
                            this.close_kit_dialog_if_done(DialogKind::ProxyEdit, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(if editing.is_some() {
                    "Edit proxy"
                } else {
                    "Add proxy"
                }))
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        content.child(
                            body.borrow_mut()
                                .take()
                                .unwrap_or_else(|| div().into_any_element()),
                        )
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }

    fn link_row(label: &'static str, value: String, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .items_start()
            .justify_between()
            .gap_4()
            .child(
                div()
                    .flex_none()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(label),
            )
            .child(
                div()
                    .min_w_0()
                    .text_sm()
                    .font_semibold()
                    .text_right()
                    .child(value),
            )
    }

    pub(super) fn build_proxy_link_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::ProxyLink, |this, _, cx| {
                this.close_proxy_link(cx);
            });
        app.update(cx, |this, cx| {
            let Some(link) = this.proxy_ui.link.as_ref() else {
                return dialog
                    .overlay(true)
                    .title(crate::ui::shell::dialog_title("Proxy Server"))
                    .on_close(on_close);
            };
            let draft = link.draft.clone();
            let warn = link.warn;
            let ping = this
                .session()
                .and_then(|s| s.proxy.pings.get(&LINK_PING_ID).copied());
            let mut body = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(Self::link_row("Server", draft.server.clone(), cx))
                .child(Self::link_row("Port", draft.port.to_string(), cx));
            if draft.kind == ProxyKind::Mtproto {
                body = body.child(Self::link_row("Secret", draft.secret.clone(), cx));
            } else {
                if !draft.username.is_empty() {
                    body = body.child(Self::link_row("Username", draft.username.clone(), cx));
                }
                if !draft.password.is_empty() {
                    body = body.child(Self::link_row("Password", draft.password.clone(), cx));
                }
            }
            let status = match ping {
                Some(status) => {
                    let (text, color) = match status {
                        PingStatus::Checking => ("Checking…".to_string(), text_muted()),
                        PingStatus::Available(ms) => {
                            (format!("Available (ping: {ms} ms)"), success())
                        }
                        PingStatus::Unavailable => ("Not Available".to_string(), danger()),
                    };
                    div().text_sm().font_semibold().text_color(color).child(text)
                }
                None => div().child(
                    Button::new("proxy-link-check")
                        .label("Check Status")
                        .ghost()
                        .xsmall()
                        .on_click(cx.listener(|this, _, _, cx| this.check_link_proxy(cx))),
                ),
            };
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_4()
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Status"),
                    )
                    .child(status),
            );
            if warn {
                body = body.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(div().text_xs().text_color(text_muted()).child(
                            "This will expose your IP address to the admin of the proxy server.",
                        ))
                        .child(
                            Button::new("proxy-link-proceed")
                                .label("Proceed")
                                .xsmall()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.confirm_link_ip_warning(cx);
                                })),
                        ),
                );
            }
            if draft.kind == ProxyKind::Mtproto {
                body = body.child(div().text_xs().text_color(text_muted()).child(
                    "This proxy may display a sponsored channel in your chat list. This doesn't reveal any of your Telegram traffic.",
                ));
            }
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("proxy-link-cancel")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_proxy_link(cx);
                            this.close_kit_dialog_if_done(DialogKind::ProxyLink, window, cx);
                        })),
                )
                .child(
                    Button::new("proxy-link-connect")
                        .label("Connect Proxy")
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.connect_link_proxy(cx);
                            this.close_kit_dialog_if_done(DialogKind::ProxyLink, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title("Proxy Server"))
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        content.child(
                            body.borrow_mut()
                                .take()
                                .unwrap_or_else(|| div().into_any_element()),
                        )
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

/// Injected proxies for the demo capture: one enabled SOCKS5 proxy, an
/// MTProto and an HTTP proxy, with one ping result each. No real proxy
/// is ever contacted.
fn demo_proxy_entries() -> Vec<ProxyEntry> {
    let mk = |id, kind, server: &str, port, enabled, last_used| {
        let mut proxy = ProxyDraft::empty(kind);
        proxy.server = server.into();
        proxy.port = port;
        if kind == ProxyKind::Mtproto {
            proxy.secret = quill::proxy::synthetic_fake_tls_secret();
        }
        ProxyEntry {
            id,
            last_used_date: last_used,
            is_enabled: enabled,
            comment: String::new(),
            proxy,
        }
    };
    vec![
        mk(
            1,
            ProxyKind::Socks5,
            "socks.example.net",
            1080,
            true,
            1_700_000_300,
        ),
        mk(
            2,
            ProxyKind::Mtproto,
            "mt.example.org",
            443,
            false,
            1_700_000_200,
        ),
        mk(3, ProxyKind::Http, "10.0.0.5", 8080, false, 0),
    ]
}

register_demos![
    // `parity:proxy-settings`: proxy list / editor / link confirmation
    // (`QUILL_DEMO_PROXY=list|edit|link|link-bad`; injected data, no live
    // Telegram, no real proxy).
    DemoSpec::chats(
        "ready-proxy",
        "screenshot demo — proxy settings (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_setup_proxy),
];

impl QuillApp {
    /// `QUILL_DEMO_PROXY=list|edit|link|link-bad` over the seeded chat
    /// list (`ready-proxy`).
    fn demo_setup_proxy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mode = std::env::var("QUILL_DEMO_PROXY").unwrap_or_else(|_| "list".into());
        if let Some(session) = self.demo_session.as_mut() {
            session.connection = ConnectionState::Ready;
            let proxy = &mut session.proxy;
            proxy.list = Some(demo_proxy_entries());
            proxy.pings.insert(1, PingStatus::Available(42));
            proxy.pings.insert(2, PingStatus::Checking);
            proxy.pings.insert(3, PingStatus::Unavailable);
            proxy.prefs.auto_switch = true;
            if mode == "link" {
                proxy.pings.insert(LINK_PING_ID, PingStatus::Available(87));
            }
        }
        match mode.as_str() {
            "edit" => self.open_proxy_editor(Some(2), false, window, cx),
            "link" => {
                let link = format!(
                    "tg://proxy?server=proxy.example.com&port=443&secret={}",
                    quill::proxy::synthetic_fake_tls_secret()
                );
                self.handle_proxy_link(&link, cx);
            }
            "link-bad" => {
                self.handle_proxy_link(
                    "tg://proxy?server=proxy.example.com&port=443&secret=zz",
                    cx,
                );
            }
            _ => self.open_proxy_list(cx),
        }
        self.status_note = "screenshot demo — proxy settings".into();
    }
}

#[cfg(test)]
mod tests {
    use super::{editor_issue_message, row_subtitle, row_title};
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
        assert_eq!(row_subtitle(&e, None), "MTPROTO · not tested");
        assert_eq!(
            row_subtitle(&e, Some(PingStatus::Available(42))),
            "MTPROTO · available (ping: 42 ms)"
        );
        assert_eq!(
            row_subtitle(&e, Some(PingStatus::Checking)),
            "MTPROTO · checking…"
        );
        assert_eq!(
            row_subtitle(&e, Some(PingStatus::Unavailable)),
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
