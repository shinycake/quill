//! The proxy list's rows (`parity:proxy-settings`, `parity:data-proxy-web`).

use super::*;
use quill::proxy_web::WebRowState;

/// What the list row shows under the title: `SOCKS5 · available (ping:
/// 42 ms)`; untested proxies show only their type. A web proxy is never
/// pinged: its status is `web`, derived from the connection (tdesktop
/// `ProxiesBoxController::updateView`).
pub(crate) fn row_subtitle(
    entry: &ProxyEntry,
    ping: Option<PingStatus>,
    web: Option<WebRowState>,
) -> String {
    let kind = kind_label(entry.proxy.kind);
    let status = match web {
        Some(WebRowState::NotTested) | None if entry.proxy.kind == ProxyKind::Web => {
            return tr_with(keys::Q_PREFS_NOT_TESTED, &[("kind", kind)]);
        }
        Some(WebRowState::Connecting) => tr(keys::PROXY_CONNECTING).to_string(),
        Some(WebRowState::Online) => tr(keys::PROXY_ONLINE).to_string(),
        Some(WebRowState::Unavailable) => tr(keys::PROXY_UNAVAILABLE).to_string(),
        _ => match ping {
            Some(status) => status.label(),
            None => return tr_with(keys::Q_PREFS_NOT_TESTED, &[("kind", kind)]),
        },
    };
    format!("{kind} · {status}")
}

/// A web proxy shows its address without a port (tdesktop `ProxyRow`).
pub(crate) fn row_title(entry: &ProxyEntry) -> String {
    match entry.proxy.kind {
        ProxyKind::Web => entry.proxy.display_address(),
        _ => format!("{}:{}", entry.proxy.server, entry.proxy.port),
    }
}

impl QuillApp {
    pub(super) fn proxy_row(
        &self,
        entry: &ProxyEntry,
        ping: Option<PingStatus>,
        web: Option<WebRowState>,
        cx: &mut Context<Self>,
    ) -> Div {
        let id = entry.id;
        let status_color = match (web, ping) {
            (Some(WebRowState::Online), _) | (None, Some(PingStatus::Available(_))) => success(),
            (Some(WebRowState::Unavailable), _) | (None, Some(PingStatus::Unavailable)) => danger(),
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
                    // The proxy in use stays on (tdesktop `applyItem`); the
                    // radios above switch it off.
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let enabled = this
                            .session()
                            .and_then(|s| s.settings.proxy.find(id))
                            .is_some_and(|p| p.is_enabled);
                        if !enabled {
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
                                    .child(row_subtitle(entry, ping, web)),
                            ),
                    ),
            )
            // Web proxies are never pinged (tdesktop shows "not tested").
            .child(
                div().when(web.is_some(), |this| this.invisible()).child(
                    icon_button(
                        "ping",
                        gpui_kit::assets::IconName::RotateCw,
                        tr(keys::PROXY_BOX_CHECK_STATUS),
                    )
                    .disabled(web.is_some())
                    .on_click(cx.listener(move |this, _, _, cx| this.ping_proxy_entry(id, cx))),
                ),
            )
            // Unshareable (HTTP) rows keep an invisible slot so the icons align.
            .child(
                div().when(!shareable, |this| this.invisible()).child(
                    icon_button(
                        "copy",
                        gpui_kit::assets::IconName::Copy,
                        tr(keys::CONTEXT_COPY_LINK),
                    )
                    .disabled(!shareable)
                    .on_click(cx.listener(move |this, _, _, cx| this.copy_proxy_link(id, cx))),
                ),
            )
            .child(
                div().when(!shareable, |this| this.invisible()).child(
                    icon_button(
                        "qr",
                        gpui_kit::assets::IconName::QrCode,
                        tr(keys::Q_PREFS_SHOW_QR_CODE),
                    )
                    .disabled(!shareable)
                    .on_click(cx.listener(move |this, _, _, cx| this.open_proxy_qr(id, cx))),
                ),
            )
            .child(
                icon_button(
                    "edit",
                    gpui_kit::assets::IconName::Pencil,
                    tr(keys::PROXY_MENU_EDIT),
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_proxy_editor(Some(id), false, window, cx)
                })),
            )
            .child(
                icon_button(
                    "delete",
                    gpui_kit::assets::IconName::Trash,
                    tr(keys::PROXY_MENU_DELETE),
                )
                .on_click(cx.listener(move |this, _, _, cx| this.delete_proxy_entry(id, cx))),
            )
    }
}
