//! The Agent access page's confirmation switch, connected list and
//! activity log in a real window.
use crate::ui::agent_state::{TEST_LOCK, reset_for_test};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::agent::access;
use quill::agent::activity::{self, Action};
use quill::ids::AccountKey;
use quill::settings_catalog::Page;

fn open_page(cx: &mut TestAppContext) -> VisualTestContext {
    open_page_app(cx).1
}

fn open_page_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    let root = quill::scratch::process_dir("quill-agent-activity-ui");
    let _ = std::fs::create_dir_all(&root);
    quill::settings::use_isolated_app_root(root);
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(12000.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        Root::new(shell, window, cx)
    });
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    reset_for_test(&AccountKey::primary());
    let app = slot
        .borrow()
        .clone()
        .unwrap_or_else(|| panic!("window opened"));
    app.update(&mut vcx, |app, cx| {
        app.open_settings(Some(Page::AgentAccess));
        cx.notify();
    });
    settle(&mut vcx);
    (app, vcx)
}

fn settle(vcx: &mut VisualTestContext) {
    for _ in 0..3 {
        vcx.run_until_parked();
        vcx.executor()
            .advance_clock(std::time::Duration::from_millis(300));
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn record(summary: &str, ok: bool) {
    activity::record(Action {
        time: 0,
        account: AccountKey::primary(),
        agent: "Claude Code".into(),
        tool: "send_message".into(),
        chat_id: 7,
        chat_title: "Ann Lee".into(),
        summary: summary.into(),
        ok,
        declined: false,
        id: 0,
        message_ids: Vec::new(),
        undone: false,
    });
}

#[gpui_kit::test]
fn the_confirm_switch_persists(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let mut vcx = open_page(cx);
    let vcx = &mut vcx;
    assert!(!access::confirm_sends(), "off by default");
    vcx.update(|window, cx| window.click("agent-confirm-switch", cx));
    settle(vcx);
    assert!(access::confirm_sends());
    vcx.update(|window, cx| window.click("agent-confirm-switch", cx));
    settle(vcx);
    assert!(!access::confirm_sends());
}

#[gpui_kit::test]
fn activity_rows_render_and_clear_empties_them(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let mut vcx = open_page(cx);
    let vcx = &mut vcx;
    assert!(vcx.debug_bounds("agent-activity-empty").is_some());
    assert!(vcx.debug_bounds("agent-activity-clear").is_none());

    record("Sent \"hi\"", true);
    record("Telegram refused it", false);
    settle(vcx);
    assert!(vcx.debug_bounds("agent-activity-empty").is_none());
    assert!(vcx.debug_bounds("agent-activity-0").is_some());
    assert!(vcx.debug_bounds("agent-activity-1").is_some());
    assert!(vcx.debug_bounds("agent-activity-2").is_none());
    assert!(
        vcx.debug_bounds("agent-activity-0-failed").is_some(),
        "the newest one failed"
    );
    assert!(vcx.debug_bounds("agent-activity-1-failed").is_none());

    vcx.update(|window, cx| window.click("agent-activity-clear", cx));
    settle(vcx);
    assert!(activity::recent(&AccountKey::primary()).is_empty());
    assert!(vcx.debug_bounds("agent-activity-empty").is_some());
}

#[gpui_kit::test]
fn the_list_shows_fifty_and_more_on_request(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let mut vcx = open_page(cx);
    let vcx = &mut vcx;
    for n in 0..55 {
        record(&format!("Sent {n}"), true);
    }
    settle(vcx);
    assert!(vcx.debug_bounds("agent-activity-49").is_some());
    assert!(vcx.debug_bounds("agent-activity-50").is_none());
    vcx.update(|window, cx| window.click("agent-activity-more", cx));
    settle(vcx);
    assert!(vcx.debug_bounds("agent-activity-54").is_some());
    assert!(vcx.debug_bounds("agent-activity-more").is_none());
}

#[gpui_kit::test]
fn connected_agents_are_listed_while_they_are_connected(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let mut vcx = open_page(cx);
    let vcx = &mut vcx;
    assert!(vcx.debug_bounds("agent-connected-row-0").is_none());
    let agent = activity::connect("Claude Code");
    settle(vcx);
    assert!(vcx.debug_bounds("agent-connected-row-0").is_some());
    drop(agent);
    settle(vcx);
    assert!(vcx.debug_bounds("agent-connected-row-0").is_none());
}

/// Whether the last frame drew element `id`.
fn shown(vcx: &mut VisualTestContext, id: &str) -> bool {
    let id = gpui_kit::SharedString::from(id.to_string());
    vcx.update(|window, _| window.try_find(id).is_some())
}

/// A send logged for `account`, with a message that still exists.
fn record_send(account: &AccountKey) {
    activity::record(Action {
        time: 0,
        account: account.clone(),
        agent: "Claude Code".into(),
        tool: "send_message".into(),
        chat_id: 7,
        chat_title: "Ann Lee".into(),
        summary: "hi".into(),
        ok: true,
        declined: false,
        id: 0,
        message_ids: vec![42],
        undone: false,
    });
}

#[gpui_kit::test]
fn another_accounts_log_offers_no_undo_open_or_add_a_chat(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx) = open_page_app(cx);
    let vcx = &mut vcx;
    let Some(root) = quill::settings::safe_app_root() else {
        panic!("an app root");
    };
    let other = quill::settings::add_account(&root, "Other")
        .unwrap_or_else(|err| panic!("a second account: {err}"));
    record_send(&AccountKey::primary());
    record_send(&other);
    settle(vcx);
    // The open account: its send can be undone and opened, and chats can
    // be added.
    assert!(shown(vcx, "agent-activity-0-undo"));
    assert!(shown(vcx, "agent-activity-0-open"));
    assert!(vcx.debug_bounds("agent-add-chat").is_some());

    // Another account: Undo and Open would act on the open account's
    // chats, and the picker lists them, so none of them shows.
    app.update(vcx, |app, cx| {
        app.agent_ui.picked = Some(other.clone());
        cx.notify();
    });
    settle(vcx);
    assert!(vcx.debug_bounds("agent-activity-0").is_some(), "its log");
    assert!(!shown(vcx, "agent-activity-0-undo"));
    assert!(!shown(vcx, "agent-activity-0-open"));
    assert!(vcx.debug_bounds("agent-add-chat").is_none());
    // Adding a chat by hand goes nowhere either.
    app.update(vcx, |app, cx| app.add_agent_chat(7, cx));
    assert!(
        !quill::agent::grants::for_account(&other)
            .chats
            .contains_key(&7)
    );
    assert!(
        !quill::agent::grants::for_account(&AccountKey::primary())
            .chats
            .contains_key(&7)
    );

    app.update(vcx, |app, cx| {
        app.agent_ui.picked = None;
        cx.notify();
    });
    let _ = quill::settings::remove_account(&root, &other);
    activity::clear();
}
