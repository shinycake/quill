//! The passkey ceremony dialog over the demo account: its three sheets and
//! how they close (needs the `demo-capture` feature for gpui-kit's test
//! support). No security key is touched: the sheets are shown without a
//! worker, and worker events are fed by hand.
use super::CeremonySheet;
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::lang::keys;
use quill::webauthn::{CeremonyError, CeremonyEvent};

pub(super) fn new_app(
    cx: &mut TestAppContext,
) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
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
                Some(ScreenshotDemo::named("ready-passkey-ceremony")),
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

pub(super) fn shown_dialog(
    shell: &Entity<QuillShell>,
    vcx: &mut VisualTestContext,
) -> Option<DialogKind> {
    vcx.run_until_parked();
    std::thread::sleep(std::time::Duration::from_millis(400));
    vcx.update(|window, cx| window.render_frame(cx));
    shell.read_with(vcx, |shell, _| shell.open_dialog)
}

pub(super) fn sheet(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<CeremonySheet> {
    app.read_with(vcx, |app, _| {
        app.privacy.passkeys.ceremony.as_ref().map(|ui| ui.sheet)
    })
}

#[gpui_kit::test]
fn cancel_on_the_key_sheet_returns_to_the_box(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::PasskeyCeremony));
    vcx.update(|window, cx| {
        let about = window.find("passkey-ceremony-about");
        assert_eq!(
            about.label(),
            Some("Insert your security key, then touch it to continue.")
        );
        assert!(window.try_find("passkey-pin").is_none());
        window.click("passkey-ceremony-cancel", cx);
    });
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::PasskeysAbout));
    assert_eq!(sheet(&app, vcx), None);
}

#[gpui_kit::test]
fn the_pin_sheet_wants_a_pin_and_says_when_it_was_wrong(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::PasskeyCeremony));
    app.update_in(vcx, |app, _, cx| {
        app.feed_ceremony_event(CeremonyEvent::NeedPin { invalid: false }, cx);
    });
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::PasskeyCeremony));
    vcx.update(|window, cx| {
        assert!(window.find("passkey-pin").visible());
        // An empty PIN is not sent: the sheet stays.
        window.click("passkey-pin-confirm", cx);
    });
    assert_eq!(
        sheet(&app, vcx),
        Some(CeremonySheet::Pin { invalid: false })
    );
    app.update_in(vcx, |app, _, cx| {
        app.feed_ceremony_event(CeremonyEvent::NeedPin { invalid: true }, cx);
    });
    shown_dialog(&shell, vcx);
    vcx.update(|window, _| {
        assert_eq!(
            window.find("passkey-ceremony-about").label(),
            Some("Incorrect PIN. Please try again.")
        );
    });
    // A typed PIN goes on and the sheet waits for the key again; the
    // field is emptied.
    app.update_in(vcx, |app, window, cx| {
        if let Some(input) = app.privacy.passkeys.pin_input.clone() {
            input.update(cx, |input, cx| input.set_value("1234", window, cx));
        }
        app.submit_passkey_pin(window, cx);
    });
    assert_eq!(sheet(&app, vcx), Some(CeremonySheet::Key));
    app.read_with(vcx, |app, cx| {
        let value = app
            .privacy
            .passkeys
            .pin_input
            .as_ref()
            .map(|input| input.read(cx).value().to_string());
        assert_eq!(value.as_deref(), Some(""));
    });
}

#[gpui_kit::test]
fn a_failure_shows_the_error_sheet_and_close_ends_it(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::PasskeyCeremony));
    app.update_in(vcx, |app, _, cx| {
        app.feed_ceremony_event(CeremonyEvent::Failed(CeremonyError::PinBlocked), cx);
    });
    assert_eq!(
        sheet(&app, vcx),
        Some(CeremonySheet::Error(keys::PASSKEY_PIN_BLOCKED))
    );
    shown_dialog(&shell, vcx);
    vcx.update(|window, cx| {
        assert!(window.try_find("passkey-ceremony-cancel").is_none());
        window.click("passkey-ceremony-close", cx);
    });
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::PasskeysAbout));
    // A cancelled ceremony closes without an error sheet.
    app.update_in(vcx, |app, _, cx| {
        app.show_passkey_sheet(CeremonySheet::Key, cx);
        app.feed_ceremony_event(CeremonyEvent::Failed(CeremonyError::Cancelled), cx);
    });
    assert_eq!(sheet(&app, vcx), None);
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::PasskeysAbout));
}

/// Windows: while Windows Security runs the ceremony Quill shows one
/// waiting row and Cancel, nothing to type and no Confirm.
#[gpui_kit::test]
fn the_windows_waiting_row_has_only_cancel(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.show_passkey_sheet(CeremonySheet::System, cx)
    });
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::PasskeyCeremony));
    vcx.update(|window, cx| {
        assert_eq!(
            window.find("passkey-ceremony-about").label(),
            Some("Waiting for Windows Security…")
        );
        assert!(window.try_find("passkey-pin").is_none());
        assert!(window.try_find("passkey-pin-confirm").is_none());
        assert!(window.try_find("passkey-ceremony-close").is_none());
        window.click("passkey-ceremony-cancel", cx);
    });
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::PasskeysAbout));
    assert_eq!(sheet(&app, vcx), None);
}

/// Windows: a failure closes the dialog and toasts the generic reason
/// (tdesktop `ShowNativeError`) instead of an error sheet; a cancel in the
/// system dialog just closes it.
#[gpui_kit::test]
fn a_windows_failure_is_a_toast(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.show_passkey_sheet(CeremonySheet::System, cx);
        app.feed_ceremony_event(
            CeremonyEvent::Failed(CeremonyError::Other("hr 0x80090011".into())),
            cx,
        );
    });
    assert_eq!(sheet(&app, vcx), None);
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::PasskeysAbout));
    app.read_with(vcx, |app, _| {
        assert_eq!(
            app.connection.status_note,
            "Couldn't add a passkey with the security key."
        );
    });
    app.update_in(vcx, |app, _, cx| {
        app.connection.status_note.clear();
        app.show_passkey_sheet(CeremonySheet::System, cx);
        app.feed_ceremony_event(CeremonyEvent::Failed(CeremonyError::Cancelled), cx);
    });
    assert_eq!(sheet(&app, vcx), None);
    app.read_with(vcx, |app, _| assert!(app.connection.status_note.is_empty()));
}
