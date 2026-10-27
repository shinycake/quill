//! Settings > Agent access in a real window: the summary card scrolls to its
//! sections, the tool rows' buttons change what they say, Reset token asks
//! first, Revoke and Allow again, and the chat menus' entry. The app data
//! root is a temp directory.
use super::{Spot, section_for_entry};
use crate::ui::agent_state::{TEST_LOCK, reset_for_test, visible_summary};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::agent::grants::{self, Scope};
use quill::agent::summary::{Claim, Section};
use quill::agent::ui_driver::guard;
use quill::agent::{access, activity};
use quill::ids::AccountKey;
use quill::settings_catalog::Page;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    let root = quill::scratch::process_dir("quill-agent-page-ui");
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
        vcx.update(|window, cx| window.simulate_next_frame(cx));
    }
}

/// The page, open with access on, default grants and an empty log.
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

fn scroll_to(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, section: Section) {
    app.update(vcx, |app, cx| {
        app.agent_ui.page.scroll_to = Some(Spot::Section(section));
        cx.notify();
    });
    settle(vcx);
}

/// Whether the last frame drew element `id`.
fn shown(vcx: &mut VisualTestContext, id: &str) -> bool {
    let id = gpui_kit::SharedString::from(id.to_string());
    vcx.update(|window, _| window.try_find(id).is_some())
}

/// Scrolls the page so element `id` sits near the top of the window.
fn bring_into_view(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, id: &str) {
    let id = gpui_kit::SharedString::from(id.to_string());
    let top = vcx
        .update(|window, _| window.try_find(id.clone()).map(|it| it.bounds().origin.y))
        .unwrap_or_else(|| panic!("{id} is on the page"));
    app.update(vcx, |app, cx| {
        let scroll = &app.agent_ui.page.scroll;
        let offset = scroll.offset();
        scroll.set_offset(gpui_kit::point(offset.x, offset.y - top + px(120.)));
        cx.notify();
    });
    settle(vcx);
}

fn page_offset(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> f32 {
    app.read_with(vcx, |app, _| f32::from(app.agent_ui.page.scroll.offset().y))
}

#[gpui_kit::test]
fn a_summary_line_scrolls_to_its_section(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, _shell, mut vcx) = open_page(cx);
    let vcx = &mut vcx;
    let summary = visible_summary(&AccountKey::primary());
    let index = summary
        .claims
        .iter()
        .position(|claim| matches!(claim, Claim::AlwaysAsks))
        .unwrap_or_else(|| panic!("the card says what always asks"));
    assert_eq!(page_offset(&app, vcx), 0.0, "the page opens at the top");

    vcx.update(|window, cx| window.click(format!("agent-claim-{index}"), cx));
    settle(vcx);
    assert!(
        page_offset(&app, vcx) < 0.0,
        "the page scrolled down to Always asks you"
    );
    assert!(
        vcx.debug_bounds("agent-section-always-asks").is_some(),
        "the section is on screen"
    );
}

#[gpui_kit::test]
fn tool_rows_follow_the_raw_switch_and_a_row_button_shares_contacts(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, _shell, mut vcx) = open_page(cx);
    let vcx = &mut vcx;
    scroll_to(&app, vcx, Section::Tools);
    assert!(access::raw_calls());
    assert!(
        !shown(vcx, "agent-tool-td_call-fix-raw"),
        "td_call is available while raw calls are on"
    );

    bring_into_view(&app, vcx, "agent-raw-calls-switch");
    vcx.update(|window, cx| window.click("agent-raw-calls-switch", cx));
    settle(vcx);
    assert!(!access::raw_calls(), "the group switch turns raw calls off");
    assert!(
        !shown(vcx, "agent-tool-td_call-fix-raw"),
        "the switch right above is the way back, not a button on each row"
    );
    bring_into_view(&app, vcx, "agent-raw-calls-switch");
    vcx.update(|window, cx| window.click("agent-raw-calls-switch", cx));
    settle(vcx);
    assert!(access::raw_calls());

    // Contacts start off: the row says so and offers to share them.
    let account = AccountKey::primary();
    assert!(!grants::for_account(&account).contacts);
    bring_into_view(&app, vcx, "agent-tool-list_contacts-fix-contacts");
    vcx.update(|window, cx| window.click("agent-tool-list_contacts-fix-contacts", cx));
    settle(vcx);
    assert!(grants::for_account(&account).contacts);
}

#[gpui_kit::test]
fn reset_token_asks_first_and_cancel_keeps_the_token(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, shell, mut vcx) = open_page(cx);
    let vcx = &mut vcx;
    scroll_to(&app, vcx, Section::Connection);
    let before = access::generation();

    vcx.update(|window, cx| window.click("agent-reset-token", cx));
    settle(vcx);
    assert_eq!(
        shell.read_with(vcx, |shell, _| shell.open_dialog),
        Some(DialogKind::AgentResetToken)
    );
    vcx.update(|window, cx| window.click("agent-reset-cancel", cx));
    settle(vcx);
    assert_eq!(shell.read_with(vcx, |shell, _| shell.open_dialog), None);
    assert_eq!(access::generation(), before, "cancel changes nothing");

    vcx.update(|window, cx| window.click("agent-reset-token", cx));
    settle(vcx);
    vcx.update(|window, cx| window.click("agent-reset-confirm", cx));
    settle(vcx);
    assert_eq!(shell.read_with(vcx, |shell, _| shell.open_dialog), None);
    assert!(access::generation() > before, "connections end");
    assert!(access::enabled(), "access stays on");
}

#[gpui_kit::test]
fn revoke_disconnects_an_agent_until_allow_again(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, _shell, mut vcx) = open_page(cx);
    let vcx = &mut vcx;
    let presence = activity::connect("claude-code");
    activity::note_tool(presence.id(), "list_chats");
    scroll_to(&app, vcx, Section::Connection);
    assert!(vcx.debug_bounds("agent-connected-row-0").is_some());

    vcx.update(|window, cx| window.click("agent-revoke-0", cx));
    settle(vcx);
    assert!(activity::is_revoked(presence.id()));
    assert!(activity::refused("claude-code"));
    assert!(
        vcx.debug_bounds("agent-connected-row-0").is_none(),
        "a revoked connection leaves the list"
    );

    vcx.update(|window, cx| window.click("agent-allow-again-0", cx));
    settle(vcx);
    assert!(!activity::refused("claude-code"));
    drop(presence);
}

#[gpui_kit::test]
fn the_chat_menu_entry_opens_the_page_at_that_chat(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, _shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let account = AccountKey::primary();
    reset_for_test(&account);
    settle(vcx);
    let chat_id = app
        .read_with(vcx, |app, _| app.demo_agent_chat())
        .unwrap_or_else(|| panic!("the fixture has a private chat"));

    // An agent driving the window never gets here.
    guard::set_injecting(true);
    app.update(vcx, |app, cx| app.open_agent_access_for_chat(chat_id, cx));
    guard::set_injecting(false);
    let _ = guard::take_refusals();
    assert!(
        !app.read_with(vcx, |app, _| app.settings.open),
        "Settings stays shut to the driver"
    );

    app.update(vcx, |app, cx| app.open_agent_access_for_chat(chat_id, cx));
    settle(vcx);
    assert!(app.read_with(vcx, |app, _| app.settings.open
        && app.settings.page == Some(Page::AgentAccess)));
    assert!(
        vcx.debug_bounds("agent-focus-chat-card").is_some(),
        "the page scrolled to the chat's card"
    );
    assert_eq!(
        grants::for_account(&account).scope_for(chat_id, grants::ChatClass::Other),
        None
    );

    vcx.update(|window, cx| window.click("agent-focus-chat-read", cx));
    settle(vcx);
    assert_eq!(
        grants::for_account(&account).scope_for(chat_id, grants::ChatClass::Other),
        Some(Scope::Read),
        "Read adds the chat"
    );
    assert!(shown(vcx, &format!("agent-chat-{chat_id}-remove")));
}

#[test]
fn every_search_entry_opens_a_section() {
    let entries: Vec<_> = quill::settings_catalog::all()
        .filter(|entry| entry.page == Page::AgentAccess)
        .collect();
    assert!(!entries.is_empty());
    for entry in entries {
        assert!(
            section_for_entry(entry.id).is_some(),
            "{} has no section",
            entry.id
        );
    }
    assert_eq!(section_for_entry("privacy.last_seen"), None);
}

#[test]
fn every_tool_has_a_plain_name() {
    for tool in quill::agent::tools::registry() {
        assert!(
            !super::super::agent_tools::tool_name(tool.name).is_empty(),
            "{} has no name on the page",
            tool.name
        );
    }
}

#[test]
fn an_ask_that_cant_come_up_says_why_in_its_own_words() {
    use super::super::agent_tools::ask_unreachable;
    use quill::agent::summary::OffReason;
    use quill::lang::{keys, tr};
    // "Each send" with nothing writable must not blame raw requests.
    assert_eq!(
        ask_unreachable(OffReason::NothingWritable),
        tr(keys::Q_AGENT_PAGE_ASK_UNREACHABLE_WRITABLE)
    );
    assert_eq!(
        ask_unreachable(OffReason::RawCallsOff),
        tr(keys::Q_AGENT_PAGE_ASK_UNREACHABLE_RAW)
    );
    assert_ne!(
        ask_unreachable(OffReason::NothingWritable),
        ask_unreachable(OffReason::RawCallsOff)
    );
}
