//! The proxy form with the WEB type: the type radios offer it, the address
//! field replaces the host and port, and saving validates the address and
//! the secret (needs the `demo-capture` feature for gpui-kit's test support).
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::calls::proxy::ProxyKind;

const SECRET: &str = "8561944064fc730cbfa4473562d8ec59";

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-privacy")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let (app, shell) = slot.borrow().clone().expect("window built");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, shell, vcx)
}

fn fill(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, address: &str, secret: &str) {
    app.update_in(vcx, |app, window, cx| {
        let Some(editor) = app.settings.proxy.editor.as_mut() else {
            return;
        };
        editor.kind = ProxyKind::Web;
        editor
            .web_host
            .update(cx, |input, cx| input.set_value(address, window, cx));
        editor
            .secret
            .update(cx, |input, cx| input.set_value(secret, window, cx));
        cx.notify();
    });
    vcx.run_until_parked();
}

#[gpui_kit::test]
fn the_form_offers_web_and_rejects_a_bad_address_or_secret(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        app.open_proxy_editor(None, false, window, cx);
    });
    vcx.run_until_parked();
    shell.read_with(vcx, |shell, _| {
        assert_eq!(shell.open_dialog, Some(DialogKind::ProxyEdit));
    });

    // An IP address is not a web proxy address.
    fill(&app, vcx, "127.0.0.1", SECRET);
    app.update_in(vcx, |app, _, cx| app.save_proxy_editor(cx));
    app.read_with(vcx, |app, _| {
        let editor = app.settings.proxy.editor.as_ref().expect("still open");
        assert!(editor.error.is_some());
    });

    // A fake-TLS secret is not supported by web proxies.
    let fake_tls = quill::proxy::synthetic_fake_tls_secret();
    fill(&app, vcx, "proxy.example.com", &fake_tls);
    app.update_in(vcx, |app, _, cx| app.save_proxy_editor(cx));
    app.read_with(vcx, |app, _| {
        let editor = app.settings.proxy.editor.as_ref().expect("still open");
        assert!(editor.error.is_some());
    });

    // A valid address (a pasted URL with a base path) and secret save; the
    // demo has no connection, so the form just closes.
    fill(&app, vcx, "https://Proxy.Example.com/app/", SECRET);
    app.update_in(vcx, |app, _, cx| app.save_proxy_editor(cx));
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| {
        assert!(app.settings.proxy.editor.is_none());
    });
}
