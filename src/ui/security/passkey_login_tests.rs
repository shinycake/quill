//! "Log in using passkey" on the QR sign-in screen and the sign-in dialog,
//! over the signed-out `wait-qr-passkey` demo (needs the `demo-capture`
//! feature for gpui-kit's test support). No security key or account is
//! touched: the sheets are shown without a worker and its events are fed
//! by hand.
use super::super::passkey_ceremony::CeremonySheet;
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::lang::keys;
use quill::webauthn::{Assertion, CeremonyError, CeremonyEvent};

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
                Some(ScreenshotDemo::named("wait-qr-passkey")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let (app, shell) = slot.borrow().clone().expect("the app view");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, shell, vcx)
}

fn shown_dialog(shell: &Entity<QuillShell>, vcx: &mut VisualTestContext) -> Option<DialogKind> {
    vcx.run_until_parked();
    std::thread::sleep(std::time::Duration::from_millis(400));
    vcx.update(|window, cx| window.render_frame(cx));
    shell.read_with(vcx, |shell, _| shell.open_dialog)
}

fn sheet(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<CeremonySheet> {
    app.read_with(vcx, |app, _| {
        app.privacy.passkeys.ceremony.as_ref().map(|ui| ui.sheet)
    })
}

#[gpui_kit::test]
fn the_link_follows_the_app_config_switch(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    assert_eq!(shown_dialog(&shell, vcx), None);
    vcx.update(|window, _| {
        let link = window.find("passkey-login");
        assert!(link.visible());
        assert_eq!(link.label(), Some("Log in using passkey"));
        assert!(window.try_find("passkey-login-error").is_none());
    });
    app.update_in(vcx, |app, _, cx| {
        app.privacy.passkeys.demo_login_link = false;
        cx.notify();
    });
    shown_dialog(&shell, vcx);
    vcx.update(|window, _| assert!(window.try_find("passkey-login").is_none()));
}

#[gpui_kit::test]
fn a_server_error_shows_under_the_link(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.privacy.passkeys.demo_login_error = Some("PASSKEY_CREDENTIAL_NOT_FOUND".into());
        cx.notify();
    });
    shown_dialog(&shell, vcx);
    vcx.update(|window, _| {
        assert_eq!(
            window.find("passkey-login-error").label(),
            Some("PASSKEY_CREDENTIAL_NOT_FOUND")
        );
    });
}

#[gpui_kit::test]
fn the_sign_in_dialog_waits_for_the_key_and_maps_failures(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.show_login_passkey_sheet(CeremonySheet::Key, cx);
    });
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::PasskeyCeremony));
    vcx.update(|window, _| {
        assert_eq!(
            window.find("passkey-ceremony-about").label(),
            Some("Insert your security key, then touch it to continue.")
        );
    });
    // The link is busy while the dialog is open.
    app.read_with(vcx, |app, _| assert!(app.passkey_login_busy()));
    // A key without a passkey for the account: sign-in's own text.
    app.update_in(vcx, |app, _, cx| {
        app.feed_ceremony_event(CeremonyEvent::Failed(CeremonyError::NoCredentials), cx);
    });
    assert_eq!(
        sheet(&app, vcx),
        Some(CeremonySheet::Error(keys::PASSKEY_ERROR_NO_CREDENTIALS))
    );
    shown_dialog(&shell, vcx);
    vcx.update(|window, _| {
        assert_eq!(
            window.find("passkey-ceremony-about").label(),
            Some("This security key has no passkey for your account.")
        );
    });
    // Anything else reads as tdesktop's login failure.
    app.update_in(vcx, |app, _, cx| {
        app.show_login_passkey_sheet(CeremonySheet::Key, cx);
        app.feed_ceremony_event(CeremonyEvent::Failed(CeremonyError::Other("x".into())), cx);
    });
    assert_eq!(
        sheet(&app, vcx),
        Some(CeremonySheet::Error(keys::PASSKEY_ERROR_LOGIN))
    );
    shown_dialog(&shell, vcx);
    vcx.update(|window, cx| window.click("passkey-ceremony-close", cx));
    assert_eq!(shown_dialog(&shell, vcx), None);
    assert_eq!(sheet(&app, vcx), None);
}

#[gpui_kit::test]
fn signing_in_or_cancelling_closes_the_dialog(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.show_login_passkey_sheet(CeremonySheet::Key, cx);
    });
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::PasskeyCeremony));
    vcx.update(|window, cx| window.click("passkey-ceremony-cancel", cx));
    assert_eq!(shown_dialog(&shell, vcx), None);
    assert_eq!(sheet(&app, vcx), None);
    app.update_in(vcx, |app, _, cx| {
        app.show_login_passkey_sheet(CeremonySheet::Pin { invalid: false }, cx);
    });
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::PasskeyCeremony));
    app.update_in(vcx, |app, _, cx| {
        app.feed_ceremony_event(
            CeremonyEvent::SignedIn(Assertion {
                credential_id: vec![1],
                authenticator_data: vec![2],
                signature: vec![3],
                user_handle: b"2:4".to_vec(),
                client_data_json: String::new(),
            }),
            cx,
        );
    });
    assert_eq!(sheet(&app, vcx), None);
    assert_eq!(shown_dialog(&shell, vcx), None);
}
