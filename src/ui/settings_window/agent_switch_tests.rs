//! Agent access's master switch in a real window: off, the only trace is
//! one switch on Advanced (no sidebar entry, no page, no search hit, no
//! chat-menu entry); on reveals the page and opens it; off on the page
//! comes back to Advanced; an agent driving the window can't flip it.
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::agent::access;
use quill::agent::ui_driver::guard;
use quill::ids::AccountKey;
use quill::settings_catalog::Page;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    let root = quill::scratch::process_dir("quill-agent-switch-ui");
    let _ = std::fs::create_dir_all(&root);
    quill::settings::use_isolated_app_root(root);
    quill::fast_buttons::set_persistence(false);
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot
        .borrow()
        .clone()
        .unwrap_or_else(|| panic!("window opened"));
    (app, VisualTestContext::from_window(handle.into(), cx))
}

fn settle(vcx: &mut VisualTestContext) {
    for _ in 0..3 {
        vcx.run_until_parked();
        vcx.executor()
            .advance_clock(std::time::Duration::from_millis(300));
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

/// Whether the settings sidebar lists the Agent access page (its rows are
/// tree items in the accessibility tree).
fn sidebar_lists_agent_access(vcx: &mut VisualTestContext) -> bool {
    vcx.update(|window, _| window.set_a11y_forced(true));
    settle(vcx);
    let title = quill::lang::tr(quill::lang::keys::Q_AGENT_ACCESS);
    let tree = vcx.update(|window, _| crate::ui::agent_driver_tree::raw_tree(window));
    let listed = tree
        .iter()
        .flat_map(|tree| tree.nodes.iter())
        .any(|node| node.role.contains("TreeItem") && node.name.as_deref() == Some(title));
    vcx.update(|window, _| window.set_a11y_forced(false));
    listed
}

fn shown_page(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Page {
    app.update(vcx, |app, _| app.settings_page_shown())
}

#[gpui_kit::test]
fn off_leaves_one_switch_on_advanced_and_on_reveals_the_page(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&crate::ui::agent_state::TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    crate::ui::agent_state::reset_for_test(&AccountKey::primary());
    access::set_enabled(false);
    app.update(vcx, |app, cx| {
        app.open_settings(Some(Page::Advanced));
        cx.notify();
    });
    settle(vcx);
    assert!(
        vcx.debug_bounds("advanced-agent-access").is_some(),
        "the switch"
    );
    assert!(!sidebar_lists_agent_access(vcx), "no Agent access page");

    // Whatever asked for the page gets Advanced, where the switch is.
    app.update(vcx, |app, cx| app.show_settings_page(Page::AgentAccess, cx));
    settle(vcx);
    assert_eq!(shown_page(&app, vcx), Page::Advanced);
    assert!(vcx.debug_bounds("agent-summary").is_none());

    vcx.update(|window, cx| window.click("advanced-agent-access-switch", cx));
    settle(vcx);
    assert!(access::enabled());
    assert_eq!(
        shown_page(&app, vcx),
        Page::AgentAccess,
        "on opens the page"
    );
    assert!(vcx.debug_bounds("agent-summary").is_some());
    assert!(
        sidebar_lists_agent_access(vcx),
        "the page is in the sidebar"
    );

    app.update(vcx, |app, cx| app.show_settings_page(Page::Advanced, cx));
    settle(vcx);
    assert!(
        vcx.debug_bounds("advanced-agent-access").is_none(),
        "on, the switch lives on the page"
    );

    app.update(vcx, |app, cx| app.show_settings_page(Page::AgentAccess, cx));
    settle(vcx);
    vcx.update(|window, cx| window.click("agent-access-switch", cx));
    settle(vcx);
    assert!(!access::enabled());
    assert_eq!(shown_page(&app, vcx), Page::Advanced, "off comes back here");
    assert!(vcx.debug_bounds("advanced-agent-access").is_some());
    assert!(!sidebar_lists_agent_access(vcx));
}

#[gpui_kit::test]
fn an_agent_driving_the_window_cannot_flip_the_switch(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&crate::ui::agent_state::TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    crate::ui::agent_state::reset_for_test(&AccountKey::primary());
    access::set_enabled(false);
    app.update(vcx, |app, cx| {
        app.open_settings(Some(Page::Advanced));
        cx.notify();
    });
    settle(vcx);
    app.update(vcx, |app, cx| {
        guard::set_injecting(true);
        app.set_agent_access(true, cx);
        guard::set_injecting(false);
    });
    settle(vcx);
    assert!(!access::enabled(), "refused");
    assert_eq!(shown_page(&app, vcx), Page::Advanced, "nothing moved");
    let _ = guard::take_refusals();
}

#[gpui_kit::test]
fn off_hides_the_chat_menu_entry_and_search(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&crate::ui::agent_state::TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    crate::ui::agent_state::reset_for_test(&AccountKey::primary());
    access::set_enabled(false);
    assert!(!crate::ui::agent_state::sees_agents(), "no agent surface");
    access::set_enabled(true);
    assert!(crate::ui::agent_state::sees_agents());
    access::set_enabled(false);

    app.update(vcx, |app, cx| {
        app.open_settings(Some(Page::Advanced));
        cx.notify();
    });
    settle(vcx);
    let hits = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.update(vcx, |app, _| {
            app.settings.search.query = "agent".into();
            app.settings_search_ids()
        })
    };
    let off = hits(&app, vcx);
    assert!(
        off.iter().all(|id| !id.starts_with("agent_access")),
        "{off:?}"
    );
    assert!(
        off.contains(&quill::settings_catalog::AGENT_SWITCH),
        "{off:?}"
    );
    access::set_enabled(true);
    let on = hits(&app, vcx);
    assert!(on.contains(&"agent_access"), "{on:?}");
    assert!(
        !on.contains(&quill::settings_catalog::AGENT_SWITCH),
        "{on:?}"
    );
    access::set_enabled(false);
}
