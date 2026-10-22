//! Screenshot demos of web proxies (injected, no live Telegram): the proxy
//! list with a web proxy in use, the same list on a system that cannot
//! host carriers, and the add box with the WEB type chosen.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use quill::proxy::ProxyState;

register_demos![
    DemoSpec::chats(
        "ready-proxy-web-list",
        "screenshot demo — the proxy list with a web proxy in use"
    )
    .setup(QuillApp::demo_proxy_web_list),
    DemoSpec::chats(
        "ready-proxy-web-unhosted",
        "screenshot demo — a web proxy in use where no web view can run"
    )
    .setup(QuillApp::demo_proxy_web_unhosted),
    DemoSpec::chats(
        "ready-proxy-web-form",
        "screenshot demo — the proxy box with the WEB type"
    )
    .setup(QuillApp::demo_proxy_web_form),
];

/// A made-up secret for the demo (assembled at runtime).
fn demo_secret() -> String {
    "ab".repeat(16)
}

fn entry(id: i32, enabled: bool, draft: ProxyDraft) -> ProxyEntry {
    ProxyEntry {
        id,
        last_used_date: 1_790_000_000 + id,
        is_enabled: enabled,
        comment: String::new(),
        proxy: draft,
    }
}

impl QuillApp {
    fn demo_proxy_web_list(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.demo_proxy_web_rows(true, cx);
    }

    fn demo_proxy_web_unhosted(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.demo_proxy_web_rows(false, cx);
    }

    fn demo_proxy_web_rows(&mut self, hosting: bool, cx: &mut Context<Self>) {
        let mut socks = ProxyDraft::empty(ProxyKind::Socks5);
        socks.server = "socks.example.org".into();
        socks.port = 1080;
        let mut mtproto = ProxyDraft::empty(ProxyKind::Mtproto);
        mtproto.server = "mt.example.net".into();
        mtproto.port = 443;
        mtproto.secret = demo_secret();
        let web = ProxyDraft::web("relay.example.com/quill", &demo_secret())
            .validated()
            .unwrap_or_else(|_| ProxyDraft::empty(ProxyKind::Web));
        if let Some(session) = self.demo_session.as_mut() {
            session.settings.proxy = ProxyState {
                list: Some(vec![
                    entry(1, false, socks),
                    entry(2, false, mtproto),
                    entry(3, true, web),
                ]),
                // The live client gets this from its carrier host.
                carriers: quill::proxy_carrier::Carriers {
                    hosting,
                    ..Default::default()
                },
                ..ProxyState::default()
            };
        }
        self.open_settings(Some(quill::settings_catalog::Page::Connection));
        cx.notify();
    }

    fn demo_proxy_web_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_proxy_editor(None, false, window, cx);
        if let Some(editor) = self.settings.proxy.editor.as_mut() {
            editor.kind = ProxyKind::Web;
            editor.web_host.update(cx, |input, cx| {
                input.set_value("relay.example.com/quill", window, cx)
            });
            editor
                .secret
                .update(cx, |input, cx| input.set_value(demo_secret(), window, cx));
        }
        cx.notify();
    }
}
