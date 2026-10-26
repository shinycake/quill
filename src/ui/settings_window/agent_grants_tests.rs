//! The grants section of Settings > Advanced > Agent access: segmented
//! controls change the grant, "All other chats" asks first, contacts and
//! the chats the user added.
use crate::ui::agent_state::{TEST_LOCK, reset_for_test};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::agent::grants::{self, ChatGrant, Scope};
use quill::ids::AccountKey;
use quill::settings_catalog::Page;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    let root = quill::scratch::process_dir("quill-agent-grants-ui");
    let _ = std::fs::create_dir_all(&root);
    quill::settings::use_isolated_app_root(root);
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(1500.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let (app, shell) = slot
        .borrow()
        .clone()
        .unwrap_or_else(|| panic!("window opened"));
    (
        app,
        shell,
        VisualTestContext::from_window(handle.into(), cx),
    )
}

fn settle(vcx: &mut VisualTestContext) {
    for _ in 0..3 {
        vcx.run_until_parked();
        vcx.executor()
            .advance_clock(std::time::Duration::from_millis(300));
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

/// The page, open with access on and default grants.
fn open_page(cx: &mut TestAppContext) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    let (app, shell, mut vcx) = new_app(cx);
    reset_for_test(&AccountKey::primary());
    app.update(&mut vcx, |app, cx| {
        app.open_settings(Some(Page::AgentAccess));
        cx.notify();
    });
    settle(&mut vcx);
    (app, shell, vcx)
}

fn now() -> grants::AccountGrants {
    grants::for_account(&AccountKey::primary())
}

#[gpui_kit::test]
fn a_segmented_control_changes_the_grant(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (_app, _shell, mut vcx) = open_page(cx);
    let vcx = &mut vcx;
    assert_eq!(now().saved, Some(Scope::ReadWrite), "the default");

    vcx.update(|window, cx| window.click("agent-grant-saved-off", cx));
    settle(vcx);
    assert_eq!(now().saved, None);
    vcx.update(|window, cx| window.click("agent-grant-bots-read", cx));
    settle(vcx);
    assert_eq!(now().bots, Some(Scope::Read));
    vcx.update(|window, cx| window.click("agent-grant-own-channels-read-write", cx));
    settle(vcx);
    assert_eq!(now().own_channels, Some(Scope::ReadWrite));
}

#[gpui_kit::test]
fn all_other_chats_ask_first(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, shell, mut vcx) = open_page(cx);
    let vcx = &mut vcx;

    vcx.update(|window, cx| window.click("agent-grant-all-chats-read-write", cx));
    settle(vcx);
    assert_eq!(now().all_chats, None, "not before the user agrees");
    assert_eq!(
        shell.read_with(vcx, |shell, _| shell.open_dialog),
        Some(DialogKind::AgentConsent)
    );

    vcx.update(|window, cx| window.click("agent-consent-cancel", cx));
    settle(vcx);
    assert_eq!(now().all_chats, None);
    assert!(app.read_with(vcx, |app, _| app.agent_ui.consent.is_none()));
    assert_eq!(shell.read_with(vcx, |shell, _| shell.open_dialog), None);

    vcx.update(|window, cx| window.click("agent-grant-all-chats-read", cx));
    settle(vcx);
    vcx.update(|window, cx| window.click("agent-consent-allow", cx));
    settle(vcx);
    assert_eq!(now().all_chats, Some(Scope::Read));
    assert_eq!(shell.read_with(vcx, |shell, _| shell.open_dialog), None);

    // Widening asks again; narrowing and switching off do not.
    vcx.update(|window, cx| window.click("agent-grant-all-chats-read-write", cx));
    settle(vcx);
    assert_eq!(now().all_chats, Some(Scope::Read));
    vcx.update(|window, cx| window.click("agent-consent-allow", cx));
    settle(vcx);
    assert_eq!(now().all_chats, Some(Scope::ReadWrite));
    vcx.update(|window, cx| window.click("agent-grant-all-chats-read", cx));
    settle(vcx);
    assert_eq!(now().all_chats, Some(Scope::Read));
    vcx.update(|window, cx| window.click("agent-grant-all-chats-off", cx));
    settle(vcx);
    assert_eq!(now().all_chats, None);
    assert_eq!(shell.read_with(vcx, |shell, _| shell.open_dialog), None);
}

#[gpui_kit::test]
fn contacts_and_added_chats_are_editable(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (_app, _shell, mut vcx) = open_page(cx);
    let vcx = &mut vcx;

    vcx.update(|window, cx| window.click("agent-contacts-switch", cx));
    settle(vcx);
    assert!(now().contacts);

    assert!(vcx.debug_bounds("agent-chats-empty").is_some());
    grants::update(&AccountKey::primary(), |g| {
        g.chats.insert(
            7,
            ChatGrant {
                scope: Some(Scope::Read),
                title: "Ann Lee".into(),
            },
        );
    });
    settle(vcx);
    assert!(vcx.debug_bounds("agent-chats-empty").is_none());
    vcx.update(|window, cx| window.click("agent-chat-7-off", cx));
    settle(vcx);
    assert_eq!(
        now().chats.get(&7).map(|g| g.scope),
        Some(None),
        "Off keeps the entry, so the chat stays blocked"
    );
    vcx.update(|window, cx| window.click("agent-chat-7-remove", cx));
    settle(vcx);
    assert!(now().chats.is_empty());
}
