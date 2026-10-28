//! Settings > Agent access in a real window: the page opens, the switch turns access on and off, and Copy puts the setup
//! command on the clipboard. The app data root is a temp directory.
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::agent::access;
use quill::settings_catalog::Page;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    let root = quill::scratch::process_dir("quill-agent-access-ui");
    let _ = std::fs::create_dir_all(&root);
    quill::settings::use_isolated_app_root(root);
    quill::fast_buttons::set_persistence(false);
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(1500.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-settings-advanced")),
            )
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
        // Tests have no frame loop: deliver the next frame, which runs a
        // scroll the page asked for.
        vcx.update(|window, cx| window.simulate_next_frame(cx));
    }
}

#[gpui_kit::test]
fn the_agent_access_page_turns_access_on_and_offers_the_setup_command(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&crate::ui::agent_state::TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    crate::ui::agent_state::reset_for_test(&quill::ids::AccountKey::primary());
    access::set_enabled(false);
    let vcx = &mut vcx;
    app.update(vcx, |_, cx| cx.notify());
    settle(vcx);
    app.update(vcx, |app, cx| {
        app.open_settings(Some(Page::Advanced));
        cx.notify();
    });
    settle(vcx);
    assert!(!access::enabled(), "off by default");

    // Off, the page's only way in is Advanced's switch (`agent_switch.rs`).
    vcx.update(|window, cx| window.click("advanced-agent-access-switch", cx));
    settle(vcx);
    assert!(access::enabled());
    assert!(
        vcx.debug_bounds("agent-summary").is_some(),
        "the summary card shows"
    );
    assert!(vcx.debug_bounds("agent-section-chats").is_some());

    // The setup lines sit at the bottom of a long page: scroll there the
    // way a search result does.
    app.update(vcx, |app, cx| {
        app.agent_ui.page.scroll_to = Some(crate::ui::settings_window::agent_page::Spot::Section(
            quill::agent::summary::Section::Connect,
        ));
        cx.notify();
    });
    settle(vcx);
    vcx.update(|window, cx| window.click("agent-copy-claude-code", cx));
    settle(vcx);
    let copied = vcx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    let copied = copied.unwrap_or_default();
    assert!(copied.starts_with("claude mcp add quill -- "), "{copied}");
    assert!(copied.ends_with(" mcp"), "{copied}");

    // Back up to the switch.
    app.update(vcx, |app, cx| {
        app.agent_ui.page.scroll_to = Some(crate::ui::settings_window::agent_page::Spot::Section(
            quill::agent::summary::Section::Connection,
        ));
        cx.notify();
    });
    settle(vcx);
    vcx.update(|window, cx| window.click("agent-access-switch", cx));
    settle(vcx);
    assert!(!access::enabled());
    assert!(
        vcx.debug_bounds("advanced-agent-access").is_some(),
        "off, Settings goes to the switch on Advanced"
    );
}

#[test]
fn setup_lines_name_this_quill() {
    let lines = quill::agent::cli::setup_lines();
    let exe = quill::agent::link::executable()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = std::path::Path::new(&exe)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    for (_, line) in lines.claude_code.iter().chain(&lines.mcp) {
        assert!(line.contains(&stem), "{line}");
        assert!(line.ends_with(" mcp"), "{line}");
    }
    for (_, line) in lines.claude_desktop.iter().chain(&lines.cursor) {
        assert!(line.contains(" agent connect "), "{line}");
        assert!(line.ends_with(" --yes"), "{line}");
    }
}
