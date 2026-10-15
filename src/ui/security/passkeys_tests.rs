//! Passkeys in the UI over the demo account: the list, deleting one through
//! the confirmation, and the "Protect your account" box
//! (needs the `demo-capture` feature for gpui-kit's test support). Nothing
//! reaches Telegram: the app has no connection.
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};

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
                Some(ScreenshotDemo::named("ready-passkeys")),
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

/// The kit dialog that shows, once its open animation (wall-clock time)
/// has finished so clicks land where the controls are drawn.
fn shown_dialog(shell: &Entity<QuillShell>, vcx: &mut VisualTestContext) -> Option<DialogKind> {
    vcx.run_until_parked();
    std::thread::sleep(std::time::Duration::from_millis(400));
    vcx.update(|window, cx| window.render_frame(cx));
    shell.read_with(vcx, |shell, _| shell.open_dialog)
}

fn passkey_ids(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Vec<String> {
    app.read_with(vcx, |app, _| {
        app.session()
            .map(|s| {
                s.settings
                    .passkeys
                    .list
                    .iter()
                    .map(|p| p.id.clone())
                    .collect()
            })
            .unwrap_or_default()
    })
}

#[gpui_kit::test]
fn deleting_a_passkey_asks_then_returns_to_the_shorter_list(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    // The demo opens the list with two passkeys, each with its menu.
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::Passkeys));
    assert_eq!(passkey_ids(&app, vcx).len(), 2);
    vcx.update(|window, _| {
        for id in ["demo-passkey-icloud", "demo-passkey-yubikey"] {
            assert!(window.find(format!("passkey-menu-{id}")).visible());
        }
    });
    // The menu's Delete asks first.
    app.update_in(vcx, |app, _, cx| {
        app.ask_delete_passkey("demo-passkey-yubikey".into(), cx)
    });
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::PasskeyDelete));
    vcx.update(|window, cx| {
        let question = window.find("passkey-delete-question");
        assert!(question.visible());
        assert_eq!(
            question.label(),
            Some("Once deleted, YubiKey 5C can't be used to log in.")
        );
        window.click("passkey-delete-confirm", cx);
    });
    // Confirmed: the passkey is gone and the list shows again.
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::Passkeys));
    assert_eq!(
        passkey_ids(&app, vcx),
        vec!["demo-passkey-icloud".to_string()]
    );
    app.read_with(vcx, |app, _| {
        assert_eq!(app.privacy.passkeys.confirm, None);
        let label = app
            .session()
            .map(|s| s.settings.passkeys.row_label().text());
        assert_eq!(label.as_deref(), Some("iCloud Keychain"));
    });
    vcx.update(|window, _| {
        assert!(
            window
                .try_find("passkey-menu-demo-passkey-yubikey")
                .is_none()
        );
    });
}

#[gpui_kit::test]
fn cancel_keeps_the_passkey(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::Passkeys));
    app.update_in(vcx, |app, _, cx| {
        app.ask_delete_passkey("demo-passkey-icloud".into(), cx)
    });
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::PasskeyDelete));
    vcx.update(|window, cx| window.click("passkey-delete-cancel", cx));
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::Passkeys));
    assert_eq!(passkey_ids(&app, vcx).len(), 2);
    app.read_with(vcx, |app, _| assert_eq!(app.privacy.passkeys.confirm, None));
}

#[gpui_kit::test]
fn learn_more_shows_the_box(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::Passkeys));
    vcx.update(|window, cx| window.click("passkeys-learn-more", cx));
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::PasskeysAbout));
    vcx.update(|window, cx| {
        let create = window.find("passkeys-create");
        let label = if quill::passkeys::webauthn_supported() {
            "Create Passkey"
        } else {
            "Unsupported"
        };
        assert_eq!(create.label(), Some(label));
        // Without a connection (or without WebAuthn) it starts nothing.
        window.click("passkeys-create", cx);
    });
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::PasskeysAbout));
    // Closing the box shows the list again.
    app.update_in(vcx, |app, window, cx| {
        app.privacy.passkeys.about_open = false;
        cx.notify();
        app.close_kit_dialog_if_done(DialogKind::PasskeysAbout, window, cx);
    });
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::Passkeys));
}

#[gpui_kit::test]
fn the_privacy_row_shows_the_count_and_opens_the_list(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::Passkeys));
    app.update_in(vcx, |app, window, cx| {
        app.close_passkeys(cx);
        app.close_kit_dialog_if_done(DialogKind::Passkeys, window, cx);
        app.settings.open = true;
        app.settings.page = Some("Privacy and security");
        cx.notify();
    });
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::Settings));
    vcx.update(|window, cx| {
        assert!(window.find("setting-Passkeys").visible());
        window.click("setting-Passkeys", cx);
    });
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::Passkeys));
    app.read_with(vcx, |app, _| assert!(!app.settings.open));
}

#[gpui_kit::test]
fn the_privacy_row_without_passkeys_follows_webauthn(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::Passkeys));
    app.update_in(vcx, |app, window, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.settings.passkeys.accept_list(Vec::new());
        }
        app.close_passkeys(cx);
        app.close_kit_dialog_if_done(DialogKind::Passkeys, window, cx);
        app.settings.open = true;
        app.settings.page = Some("Privacy and security");
        cx.notify();
    });
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::Settings));
    // Shown while a passkey can be created (tdesktop `canRegister`).
    let shown = quill::passkeys::webauthn_supported();
    vcx.update(|window, _| assert_eq!(window.try_find("setting-Passkeys").is_some(), shown));
}
