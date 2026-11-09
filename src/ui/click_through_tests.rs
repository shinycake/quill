//! Clicks on a layer stay on it (`dismiss_layer.rs`): a click on a panel
//! keeps it open, a click outside closes it and reaches nothing behind, and
//! a toast takes its own click. Over the screenshot demos; nothing reaches
//! Telegram.
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Modifiers, TestAppContext, VisualTestContext, point, px, size};
use quill::ids::ChatId;
use quill::peer_qr::state::QrPanel;

fn open_app(cx: &mut TestAppContext, demo: &'static str) -> (Entity<QuillApp>, VisualTestContext) {
    open_sized(cx, demo, 800.)
}

fn open_sized(
    cx: &mut TestAppContext,
    demo: &'static str,
    height: f32,
) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(height)), move |window, cx| {
        let view = cx
            .new(|cx| QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named(demo))));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("the app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    (app, vcx)
}

fn frames(vcx: &mut VisualTestContext) {
    vcx.update(|_, cx| cx.set_reduce_motion(true));
    vcx.run_until_parked();
    vcx.executor()
        .advance_clock(std::time::Duration::from_millis(250));
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

/// A click just inside the top-left corner of `id` (its padding).
fn click_inside(vcx: &mut VisualTestContext, id: &'static str) {
    vcx.update(|window, cx| window.click_at(id, point(px(6.), px(6.)), cx));
    frames(vcx);
}

/// A click near the window's left edge, outside every centered panel.
fn click_outside(vcx: &mut VisualTestContext) {
    vcx.simulate_click(point(px(6.), px(400.)), Modifiers::default());
    frames(vcx);
}

type Opener = fn(&mut QuillApp, &mut gpui_kit::Window, &mut gpui_kit::Context<QuillApp>);
type IsOpen = fn(&QuillApp) -> bool;

/// The centered panels that used to close on a click inside them.
fn panels() -> Vec<(&'static str, &'static str, Opener, IsOpen)> {
    vec![
        (
            "story composer",
            "story-composer-panel",
            |app, window, cx| app.open_story_composer(window, cx),
            |app| app.stories.composer.open,
        ),
        (
            "story page",
            "story-page-panel",
            |app, window, cx| {
                app.stories.page =
                    Some(crate::ui::story_page::StoryPage::new(ChatId(1), window, cx));
            },
            |app| app.stories.page.is_some(),
        ),
        (
            "privacy editor",
            "privacy-dialog-editor",
            |app, _, _| {
                app.privacy.editor = Some(crate::ui::privacy::PrivacyEditorTarget::Rule(
                    quill::telegram::requests_privacy::PrivacySettingKey::ShowStatus,
                ));
            },
            |app| app.privacy.editor.is_some(),
        ),
    ]
}

#[gpui_kit::test]
fn a_click_inside_a_panel_keeps_it_and_outside_closes_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-chats");
    let vcx = &mut vcx;
    for (name, panel, open, is_open) in panels() {
        app.update_in(vcx, open);
        frames(vcx);
        assert!(app.read_with(vcx, |app, _| is_open(app)), "{name} opens");
        click_inside(vcx, panel);
        assert!(
            app.read_with(vcx, |app, _| is_open(app)),
            "a click inside the {name} keeps it open"
        );
        click_outside(vcx);
        assert!(
            !app.read_with(vcx, |app, _| is_open(app)),
            "a click outside closes the {name}"
        );
    }
}

#[gpui_kit::test]
fn a_click_on_a_toast_over_the_viewer_leaves_the_viewer_open(cx: &mut TestAppContext) {
    crate::ui::app_render::pin_toasts_for_tests();
    // Tall, so the toast sits over the black below the picture, which
    // closes the viewer when clicked.
    let (app, mut vcx) = open_sized(cx, "ready-status-toast-viewer", 1400.);
    let vcx = &mut vcx;
    assert!(app.read_with(vcx, |app, _| app.viewer.state.is_open()));
    app.update(vcx, |app, cx| {
        app.set_note(crate::ui::status_note::StatusNote::info(
            "Saved to Downloads",
        ));
        cx.notify();
    });
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.update(|window, cx| window.click("status-toast", cx));
    frames(vcx);
    assert!(
        app.read_with(vcx, |app, _| app.viewer.state.is_open()),
        "the click dismissed the toast only, not the viewer behind it"
    );
}

fn qr_panel(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<QrPanel> {
    app.read_with(vcx, |app, _| {
        app.peer_qr.open.as_ref().and_then(|open| open.state.panel)
    })
}

#[gpui_kit::test]
fn a_click_off_the_qr_panel_closes_only_the_panel(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-peer-qr");
    let vcx = &mut vcx;
    let colors = vcx
        .debug_bounds("peer-qr-colors")
        .expect("the colors button");
    let close = vcx.debug_bounds("peer-qr-close").expect("the close button");
    vcx.simulate_click(colors.center(), Modifiers::default());
    frames(vcx);
    assert_eq!(qr_panel(&app, vcx), Some(QrPanel::Colors));
    let panel = vcx.debug_bounds("peer-qr-panel").expect("the panel");
    assert!(
        !panel.contains(&close.center()),
        "the close button shows beside the panel"
    );
    vcx.simulate_click(close.center(), Modifiers::default());
    frames(vcx);
    assert_eq!(qr_panel(&app, vcx), None, "the click closed the colors");
    assert!(
        app.read_with(vcx, |app, _| app.peer_qr.open.is_some()),
        "and did not press Close under the layer"
    );
}

/// The demo's main window and its call window.
fn open_call(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext, VisualTestContext) {
    let (app, mut main) = open_app(cx, "ready-video-call");
    app.update(&mut main, |app, cx| app.sync_call_window(cx));
    frames(&mut main);
    let window = app
        .read_with(&main, |app, _| app.calls.window)
        .expect("the call window opens");
    let mut call = VisualTestContext::from_window(window, cx);
    frames(&mut call);
    (app, main, call)
}

fn device_menu_shown(call: &mut VisualTestContext) -> bool {
    call.update(|window, _| window.try_find("call-device-menu").is_some())
}

#[gpui_kit::test]
fn the_device_menu_toggle_closes_the_menu_it_opened(cx: &mut TestAppContext) {
    let (_app, _main, mut call) = open_call(cx);
    let call = &mut call;
    call.update(|window, cx| window.click("call-mute-devices", cx));
    frames(call);
    assert!(device_menu_shown(call), "the toggle opens the menu");
    // The menu's layer takes this click: it closes the menu, and the toggle
    // under it does not open it again.
    call.update(|window, cx| window.click("call-mute-devices", cx));
    frames(call);
    assert!(!device_menu_shown(call), "the second click closes it");
}
