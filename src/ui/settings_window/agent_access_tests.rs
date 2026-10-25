//! Settings > Advanced > Agent access in a real window: the row opens the
//! page, the switch turns access on and off, and Copy puts the setup
//! command on the clipboard. The app data root is a temp directory.
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::settings_window::SubPage;
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
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
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
    }
}

#[gpui_kit::test]
fn the_agent_access_page_turns_access_on_and_offers_the_setup_command(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update(vcx, |_, cx| cx.notify());
    settle(vcx);
    vcx.update(|window, cx| window.click("settings-agent-access", cx));
    settle(vcx);
    app.read_with(vcx, |app, _| {
        assert_eq!(app.settings.page, Some(Page::Advanced));
        assert_eq!(app.settings.sub, Some(SubPage::AgentAccess));
    });
    assert!(!access::enabled(), "off by default");

    vcx.update(|window, cx| window.click("agent-access-switch", cx));
    settle(vcx);
    assert!(access::enabled());

    vcx.update(|window, cx| window.click("agent-copy-claude-code", cx));
    settle(vcx);
    let copied = vcx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    let copied = copied.unwrap_or_default();
    assert!(copied.starts_with("claude mcp add quill -- "), "{copied}");
    assert!(copied.ends_with(" mcp"), "{copied}");

    vcx.update(|window, cx| window.click("agent-access-switch", cx));
    settle(vcx);
    assert!(!access::enabled());
}

#[test]
fn setup_commands_quote_paths_for_the_shell() {
    use super::shell_quote;
    assert_eq!(shell_quote("/usr/bin/quill"), "/usr/bin/quill");
    if cfg!(windows) {
        assert_eq!(
            shell_quote(r"C:\Program Files\Quill\quill.exe"),
            r#""C:\Program Files\Quill\quill.exe""#
        );
    } else {
        assert_eq!(
            shell_quote("/Applications/Quill.app/Contents/MacOS/quill"),
            "/Applications/Quill.app/Contents/MacOS/quill"
        );
        assert_eq!(
            shell_quote("/home/a b/it's $HOME"),
            r"'/home/a b/it'\''s $HOME'"
        );
    }
}
