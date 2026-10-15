//! The phone sheets of the passkey ceremony over the demo account: the QR
//! sheet, Bluetooth off, connecting, continue, the phone error, and the
//! "Use a security key" pill. No phone or Bluetooth is involved: the
//! worker's events are fed by hand (the worker itself is covered by
//! `quill::webauthn::hybrid`'s tests over a scripted fake phone).
use super::CeremonySheet;
use super::tests::{new_app, sheet, shown_dialog};
use crate::ui::shell::DialogKind;
use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt;
use quill::lang::keys;
use quill::webauthn::{CeremonyError, CeremonyEvent, HybridQr};

fn demo_qr() -> HybridQr {
    HybridQr(super::hybrid::demo_qr())
}

/// The probe shows nothing; the QR sheet follows with the code, the
/// Bluetooth hint, the pill and Cancel.
#[gpui_kit::test]
fn the_qr_sheet_follows_the_bluetooth_probe(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.show_passkey_sheet(CeremonySheet::Probing, cx)
    });
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::PasskeysAbout));
    app.update_in(vcx, |app, _, cx| {
        app.feed_ceremony_event(
            CeremonyEvent::Hybrid {
                qr: Some(demo_qr()),
            },
            cx,
        );
    });
    assert_eq!(
        sheet(&app, vcx),
        Some(CeremonySheet::Choose { bluetooth: true })
    );
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::PasskeyCeremony));
    vcx.update(|window, _| {
        assert_eq!(
            window.find("passkey-ceremony-about").label(),
            Some(
                "Scan this QR code with the camera on the phone or tablet where you want to save your passkey."
            )
        );
        assert!(window.find("passkey-cable-qr").visible());
        assert!(window.find("passkey-cable-bluetooth").visible());
        assert!(window.find("passkey-cable-use-key").visible());
        assert!(window.find("passkey-cable-key-about").visible());
        assert!(window.try_find("passkey-ceremony-close").is_none());
    });
    // Cancel closes it back to the box.
    vcx.update(|window, cx| window.click("passkey-ceremony-cancel", cx));
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::PasskeysAbout));
    assert_eq!(sheet(&app, vcx), None);
}

/// Without Bluetooth the sheet says so, leaves the code out and keeps the
/// security key way.
#[gpui_kit::test]
fn bluetooth_off_keeps_only_the_security_key(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.show_passkey_sheet(CeremonySheet::Probing, cx);
        app.feed_ceremony_event(CeremonyEvent::Hybrid { qr: None }, cx);
    });
    assert_eq!(
        sheet(&app, vcx),
        Some(CeremonySheet::Choose { bluetooth: false })
    );
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::PasskeyCeremony));
    vcx.update(|window, _| {
        assert_eq!(
            window.find("passkey-ceremony-about").label(),
            Some("Bluetooth is unavailable. Turn on Bluetooth to sign in with a phone.")
        );
        assert!(window.try_find("passkey-cable-qr").is_none());
        assert!(window.try_find("passkey-cable-bluetooth").is_none());
        assert!(window.find("passkey-cable-use-key").visible());
    });
}

/// The pill: with no key the QR stays and a toast says so; with one the
/// key sheet takes over.
#[gpui_kit::test]
fn the_security_key_pill_switches_to_the_key_sheet(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.show_passkey_sheet(CeremonySheet::Probing, cx);
        app.feed_ceremony_event(
            CeremonyEvent::Hybrid {
                qr: Some(demo_qr()),
            },
            cx,
        );
    });
    shown_dialog(&shell, vcx);
    // Without a worker the click asks nothing; the sheet stays.
    vcx.update(|window, cx| window.click("passkey-cable-use-key", cx));
    assert_eq!(
        sheet(&app, vcx),
        Some(CeremonySheet::Choose { bluetooth: true })
    );
    app.update_in(vcx, |app, _, cx| {
        app.feed_ceremony_event(CeremonyEvent::NoKey, cx)
    });
    assert_eq!(
        sheet(&app, vcx),
        Some(CeremonySheet::Choose { bluetooth: true })
    );
    app.read_with(vcx, |app, _| {
        assert_eq!(
            app.connection.status_note,
            "No security key found. Insert one and try again."
        );
    });
    app.update_in(vcx, |app, _, cx| {
        app.feed_ceremony_event(CeremonyEvent::UseKey, cx)
    });
    assert_eq!(sheet(&app, vcx), Some(CeremonySheet::Key));
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::PasskeyCeremony));
    vcx.update(|window, _| {
        assert_eq!(
            window.find("passkey-ceremony-about").label(),
            Some("Insert your security key, then touch it to continue.")
        );
        assert!(window.try_find("passkey-cable-qr").is_none());
    });
}

/// Connecting has no description; Continue tells the user to follow the
/// phone; a phone failure ends on the error sheet with Close.
#[gpui_kit::test]
fn the_phone_sheets_end_on_the_error_sheet(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.show_passkey_sheet(CeremonySheet::Probing, cx);
        app.feed_ceremony_event(
            CeremonyEvent::Hybrid {
                qr: Some(demo_qr()),
            },
            cx,
        );
        app.feed_ceremony_event(CeremonyEvent::PhoneConnecting, cx);
    });
    assert_eq!(sheet(&app, vcx), Some(CeremonySheet::Connecting));
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::PasskeyCeremony));
    vcx.update(|window, _| {
        assert!(window.try_find("passkey-ceremony-about").is_none());
        assert!(window.try_find("passkey-cable-qr").is_none());
        assert!(window.find("passkey-ceremony-cancel").visible());
    });
    app.update_in(vcx, |app, _, cx| {
        app.feed_ceremony_event(CeremonyEvent::PhoneContinue, cx)
    });
    shown_dialog(&shell, vcx);
    vcx.update(|window, _| {
        assert_eq!(
            window.find("passkey-ceremony-about").label(),
            Some("Follow the instructions on your phone.")
        );
    });
    app.update_in(vcx, |app, _, cx| {
        app.feed_ceremony_event(CeremonyEvent::Failed(CeremonyError::Phone), cx)
    });
    assert_eq!(
        sheet(&app, vcx),
        Some(CeremonySheet::Error(keys::PASSKEY_CABLE_ERROR))
    );
    shown_dialog(&shell, vcx);
    vcx.update(|window, cx| {
        assert_eq!(
            window.find("passkey-ceremony-about").label(),
            Some("Couldn't connect to your phone.\nPlease try again.")
        );
        assert!(window.try_find("passkey-ceremony-cancel").is_none());
        window.click("passkey-ceremony-close", cx);
    });
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::PasskeysAbout));
    assert_eq!(sheet(&app, vcx), None);
}

/// A phone ceremony that fails before its box shows toasts the reason
/// (tdesktop `cable_ceremony.cpp` `finish` without a box).
#[gpui_kit::test]
fn a_failure_during_the_probe_is_a_toast(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.show_passkey_sheet(CeremonySheet::Probing, cx);
        app.feed_ceremony_event(CeremonyEvent::Failed(CeremonyError::Phone), cx);
    });
    assert_eq!(sheet(&app, vcx), None);
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::PasskeysAbout));
    app.read_with(vcx, |app, _| {
        assert_eq!(
            app.connection.status_note,
            "Couldn't connect to your phone.\nPlease try again."
        );
    });
}
