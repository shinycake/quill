#![cfg(all(test, feature = "demo-capture"))]
//! Hand-drawn layers give focus back to the composer when they close,
//! whether by Escape, by their close button, or by a direct close call.

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::media_panel::PanelTab;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open_demo(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats-composer");
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    (app, vcx)
}

fn focus_composer(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update_in(vcx, |app, window, cx| {
        app.composer.update(cx, |input, cx| input.focus(window, cx));
    });
    frames(vcx);
}

/// Something inside the open layer takes focus, as its search field does.
fn focus_search(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update_in(vcx, |app, window, cx| {
        app.pickers
            .reaction_search_input
            .update(cx, |input, cx| input.focus(window, cx));
    });
    frames(vcx);
}

fn composer_focused(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.update_in(vcx, |app, window, cx| {
        app.composer.read(cx).focus_handle(cx).is_focused(window)
    })
}

#[gpui_kit::test]
fn closing_the_emoji_panel_returns_focus_to_the_composer(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    focus_composer(&app, vcx);
    assert!(composer_focused(&app, vcx));

    app.update(vcx, |app, cx| app.open_media_panel(PanelTab::Emoji, cx));
    frames(vcx);
    focus_search(&app, vcx);
    assert!(!composer_focused(&app, vcx));

    app.update(vcx, |app, cx| {
        assert!(app.pickers.media_panel.open);
        app.close_media_panel(cx);
    });
    frames(vcx);
    assert!(composer_focused(&app, vcx));
}

#[gpui_kit::test]
fn the_close_button_returns_focus_to_the_composer(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    focus_composer(&app, vcx);
    app.update(vcx, |app, cx| app.open_media_panel(PanelTab::Emoji, cx));
    frames(vcx);
    focus_search(&app, vcx);
    vcx.update(|window, cx| window.click("media-panel-close", cx));
    frames(vcx);
    app.update(vcx, |app, _| assert!(!app.pickers.media_panel.open));
    assert!(composer_focused(&app, vcx));
}

#[gpui_kit::test]
fn escape_on_the_sticker_panel_returns_focus_to_the_composer(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    focus_composer(&app, vcx);
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.stickers.stickers.open = true;
        }
        cx.notify();
    });
    frames(vcx);
    focus_search(&app, vcx);
    assert!(!composer_focused(&app, vcx));
    app.update_in(vcx, |app, window, cx| {
        assert!(app.dismiss_topmost_layer(window, cx));
    });
    frames(vcx);
    app.update(vcx, |app, _| assert!(!app.sticker_panel_open()));
    assert!(composer_focused(&app, vcx));
}
