//! The Dock and Window menus against a demo window: what the Dock shows for
//! the signed-in accounts and the player, and what the Window menu's
//! actions open (needs the `demo-capture` feature for gpui-kit's test
//! support).
use super::{
    DockEntry, DockOpenAccount, DockToggleNotifications, Registry, WindowMenuAddContact,
    WindowMenuNewGroup, dock_entries, register_actions,
};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::AccountKey;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    let weak = app.downgrade();
    vcx.update(|window, cx| register_actions(window.window_handle(), weak, cx));
    (app, vcx)
}

#[gpui_kit::test]
fn the_dock_lists_signed_in_accounts_and_the_notification_toggle(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, _, _| {
        let on_screen = app.session().unwrap().account.clone();
        let registry = Registry {
            accounts: vec![
                (on_screen.clone(), "Stored name".into()),
                // Registered but not signed in here: no row.
                (AccountKey("other".into()), "Work".into()),
            ],
            active: Some(on_screen.clone()),
        };
        let model = app.dock_model(&registry);
        assert_eq!(model.registered, 2);
        assert_eq!(model.accounts.len(), 1);
        assert_eq!(model.accounts[0].key, on_screen);
        // The on-screen account shows its own profile name when the
        // session knows it, the stored one otherwise.
        let own = app
            .session()
            .and_then(|session| session.my_user_id.and_then(|id| session.user(id)))
            .map(|user| user.display_name())
            .unwrap_or_else(|| "Stored name".into());
        assert_eq!(model.accounts[0].name, own);
        let entries = dock_entries(&model);
        assert_eq!(entries[1], DockEntry::ProfilesHeader);
        assert!(
            entries
                .iter()
                .any(|entry| entry.label() == "Disable notifications")
        );
        // No track loaded: no player rows.
        assert!(!entries.iter().any(|entry| entry.label() == "Pause"));
    });
    // The toggle flips the desktop notification setting.
    vcx.dispatch_action(DockToggleNotifications);
    vcx.run_until_parked();
    app.update_in(&mut vcx, |app, _, _| {
        assert!(!app.desktop_notifications_enabled());
    });
    // Picking the account on screen keeps it.
    let on_screen = app.update_in(&mut vcx, |app, _, _| {
        app.session().unwrap().account.0.clone()
    });
    vcx.dispatch_action(DockOpenAccount {
        key: on_screen.clone(),
    });
    vcx.run_until_parked();
    app.update_in(&mut vcx, |app, _, _| {
        assert_eq!(app.session().unwrap().account.0, on_screen);
    });
}

#[gpui_kit::test]
fn window_menu_add_contact_opens_the_new_contact_box(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    vcx.dispatch_action(WindowMenuAddContact);
    vcx.run_until_parked();
    app.update_in(&mut vcx, |app, window, cx| {
        let dialog = app.dialogs.add_contact_dialog.as_ref().unwrap();
        assert_eq!(dialog.user_id, 0);
        assert!(!dialog.not_joined);
        // Create with no name keeps the box open and sends nothing.
        app.submit_new_contact(window, cx);
        assert!(
            app.dialogs
                .add_contact_dialog
                .as_ref()
                .unwrap()
                .sent_name
                .is_none()
        );
        let dialog = app.dialogs.add_contact_dialog.as_ref().unwrap();
        let first = dialog.first_name_input.clone();
        let phone = dialog.phone_input.clone();
        first.update(cx, |input, cx| input.set_value("Ada", window, cx));
        // Too short for tdesktop's phone check.
        phone.update(cx, |input, cx| input.set_value("12345", window, cx));
        app.submit_new_contact(window, cx);
        assert!(
            app.dialogs
                .add_contact_dialog
                .as_ref()
                .unwrap()
                .sent_name
                .is_none()
        );
        phone.update(cx, |input, cx| {
            input.set_value("+1 555 010 0199", window, cx)
        });
        // The demo has no Telegram connection, so nobody has joined.
        app.submit_new_contact(window, cx);
        let dialog = app.dialogs.add_contact_dialog.as_ref().unwrap();
        assert_eq!(dialog.sent_name.as_deref(), Some("Ada"));
        assert!(dialog.not_joined);
    });
    vcx.run_until_parked();
}

#[gpui_kit::test]
fn window_menu_new_group_opens_the_create_group_box(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    vcx.dispatch_action(WindowMenuNewGroup);
    vcx.run_until_parked();
    app.update_in(&mut vcx, |app, _, _| {
        assert!(app.admin.create_chat_dialog.is_some());
    });
}

#[gpui_kit::test]
fn window_menu_items_do_nothing_while_locked(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, _, _| {
        app.account.passcode.locked = true;
        assert!(!app.os_menu_session_usable());
    });
    vcx.dispatch_action(WindowMenuAddContact);
    vcx.run_until_parked();
    app.update_in(&mut vcx, |app, _, _| {
        assert!(app.dialogs.add_contact_dialog.is_none());
    });
}
