//! The hard surfaces stay closed to an agent's input, whatever the route:
//! a chord bound to Settings or the lock, a menu item or a main-menu entry
//! reaches a handler that refuses while the driver injects, so nothing is
//! left open for the user to close, and the step's reply says why.
use super::tests::{click, drive, grant, message_box, new_app, read_tree, settle, turn_on};
use crate::ui::agent_state::TEST_LOCK;
use crate::ui::app::QuillApp;
use crate::ui::keybindings::apply_custom_bindings;
use crate::ui::navigation::NavigationAction;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use quill::agent::activity;
use quill::agent::grants::Scope;
use quill::agent::ui_driver::bridge::{Target, UiCommand, UiReply};
use quill::agent::ui_driver::guard;
use quill::agent::ui_driver::keys::parse_sequence;

/// Press `keys` as an agent and return the step's refusals.
fn press(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, keys: &str) -> Vec<String> {
    let chords = parse_sequence(keys).unwrap_or_default();
    match drive(app, vcx, UiCommand::PressKeys { chords }) {
        Ok(UiReply::Done { refusals, .. }) => refusals,
        other => panic!("expected Done for {keys}, got {other:?}"),
    }
}

/// The open chat's message box has the focus, so chords reach the app's
/// handlers through it as the user's keys do.
fn focus_message_box(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    let tree = read_tree(app, vcx);
    let reply = drive(app, vcx, click(Target::Node(message_box(&tree))));
    assert!(matches!(reply, Ok(UiReply::Done { .. })), "{reply:?}");
}

#[gpui_kit::test]
fn the_settings_and_lock_chords_open_nothing_for_an_agent(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    // The keymap a signed-in Quill runs with: the defaults plus the fixed
    // window and app chords (Cmd/Ctrl+, among them).
    vcx.update(|_, cx| apply_custom_bindings(cx, &[]));
    turn_on();
    grant(11, Scope::ReadWrite);
    let _presence = activity::connect("Test agent");
    settle(vcx);
    focus_message_box(&app, vcx);

    // Cmd+, (and Ctrl+, elsewhere): Settings stays closed and the reply
    // carries the refusal.
    for chord in ["cmd-,", "ctrl-,"] {
        let refusals = press(&app, vcx, chord);
        assert!(
            refusals.iter().any(|r| r.contains("open Settings")),
            "{chord}: {refusals:?}"
        );
        assert!(
            !vcx.update(|_, cx| app.read(cx).settings.open),
            "{chord} left Settings open"
        );
    }

    // Cmd/Ctrl+L with a passcode set: Quill stays unlocked.
    app.update(vcx, |app, _| {
        app.account.passcode.enabled = true;
        app.account.passcode.demo = true;
    });
    for chord in ["cmd-l", "ctrl-l"] {
        let refusals = press(&app, vcx, chord);
        assert!(
            refusals.iter().any(|r| r.contains("lock Quill")),
            "{chord}: {refusals:?}"
        );
        assert!(
            !vcx.update(|_, cx| app.read(cx).account.passcode.locked),
            "{chord} locked Quill"
        );
    }

    // Minimise has a Ctrl chord on Linux and Windows that the chord list
    // does not name; the handler refuses it on every platform.
    let refusals = press(&app, vcx, "ctrl-m");
    assert!(
        refusals.iter().any(|r| r.contains("minimise")),
        "{refusals:?}"
    );

    // The user's own Cmd+, still opens Settings.
    vcx.simulate_keystrokes("cmd-,");
    settle(vcx);
    assert!(vcx.update(|_, cx| app.read(cx).settings.open));
}

#[gpui_kit::test]
fn menu_routes_to_hard_surfaces_refuse_while_injecting(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    settle(vcx);
    let _ = guard::take_refusals();

    // What a click on a main-menu entry or a menu bar item runs, under the
    // flag a step sets.
    guard::set_injecting(true);
    app.update_in(vcx, |app, window, cx| {
        for action in [
            NavigationAction::Settings,
            NavigationAction::Privacy,
            NavigationAction::Sessions,
            NavigationAction::Passcode,
            NavigationAction::TwoFa,
            NavigationAction::Account,
            NavigationAction::Accounts,
            NavigationAction::Stars,
            NavigationAction::Premium,
        ] {
            app.navigate(action, window, cx);
        }
        app.open_settings(Some(quill::settings_catalog::Page::Shortcuts));
        app.open_account_lifecycle(cx);
        app.open_accounts(cx);
        app.open_passcode(cx);
        app.lock_by_passcode(cx);
    });
    guard::set_injecting(false);
    let refusals = guard::take_refusals();
    for part in [
        "open Settings",
        "log out or switch accounts",
        "open the passcode settings",
        "open the security settings",
        "open payments",
        "lock Quill",
    ] {
        assert!(
            refusals.iter().any(|r| r.contains(part)),
            "{part}: {refusals:?}"
        );
    }
    vcx.update(|_, cx| {
        let app = app.read(cx);
        assert!(!app.settings.open, "Settings");
        assert!(!app.account.lifecycle.open, "log out");
        assert!(!app.account.accounts.open, "accounts");
        assert!(!app.account.passcode.open, "passcode");
        assert!(!app.twofa.open, "two-step verification");
        assert!(!app.account.passcode.locked, "lock");
    });

    // Not injecting, the same entry opens.
    app.update_in(vcx, |app, window, cx| {
        app.navigate(NavigationAction::Settings, window, cx);
    });
    assert!(vcx.update(|_, cx| app.read(cx).settings.open));
    assert!(guard::take_refusals().is_empty());
}
