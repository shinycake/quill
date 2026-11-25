//! The Floating cards and the drawer through GPUI: where the list card
//! sits, and every way the drawer opens and closes in an 800 px window.

use crate::ui::app::QuillApp;
use crate::ui::appearance::own_appearance_prefs;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use crate::ui::sidebar_reveal::{Peek, SLIDE_DURATION};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Modifiers, TestAppContext, VisualTestContext, point, px, size};
use quill::ids::ChatId;
use quill::layout_style::LayoutStyle;
use std::time::Duration;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn slide(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update(vcx, |app, _| {
        app.frame
            .sidebar
            .advance(SLIDE_DURATION + Duration::from_millis(60));
    });
    frames(vcx);
}

fn open(
    cx: &mut TestAppContext,
    width: f32,
    layout: LayoutStyle,
) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    crate::ui::interface_zoom::set_initial_zoom(1.);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    let handle = cx.open_window(size(px(width), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    app.update(&mut vcx, |app, cx| {
        app.frame.sidebar.pin();
        app.set_appearance(cx, |a| a.layout = layout);
    });
    frames(&mut vcx);
    (app, vcx)
}

fn held(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| app.frame.sidebar.pointer() == Peek::Held)
}

fn click_at(vcx: &mut VisualTestContext, x: f32, y: f32) {
    vcx.simulate_click(point(px(x), px(y)), Modifiers::default());
    frames(vcx);
}

#[gpui_kit::test]
fn the_list_card_sits_inside_its_gutter(cx: &mut TestAppContext) {
    let _prefs = own_appearance_prefs("quill-cards-gutter");
    let (_app, mut vcx) = open(cx, 1440., LayoutStyle::Floating);
    let title = vcx.debug_bounds("conversation-slot").expect("chat").top();
    let card = vcx.debug_bounds("sidebar-card").expect("card");
    assert_eq!(card.left(), px(16.));
    assert_eq!(card.top(), title + px(16.));
}

#[gpui_kit::test]
fn the_classic_list_fills_its_column(cx: &mut TestAppContext) {
    let _prefs = own_appearance_prefs("quill-cards-classic");
    let (_app, mut vcx) = open(cx, 1440., LayoutStyle::Classic);
    let title = vcx.debug_bounds("conversation-slot").expect("chat").top();
    let card = vcx.debug_bounds("sidebar-card").expect("card");
    assert_eq!(card.left(), px(0.));
    assert_eq!(card.top(), title);
}

#[gpui_kit::test]
fn the_drawer_opens_from_the_button_and_closes_every_way(cx: &mut TestAppContext) {
    let _prefs = own_appearance_prefs("quill-cards-drawer");
    let (app, mut vcx) = open(cx, 800., LayoutStyle::Floating);
    let vcx = &mut vcx;
    assert!(app.read_with(vcx, |app, _| app.shell_layout().drawer()));
    assert!(!held(&app, vcx));

    // The header's button opens it; again closes it.
    app.update_in(vcx, |app, window, cx| app.toggle_sidebar(window, cx));
    slide(&app, vcx);
    assert!(held(&app, vcx));
    let drawer = vcx.debug_bounds("sidebar-card").expect("drawer drawn");
    assert_eq!(drawer.left(), px(16.), "the drawer lands at its gutter");
    app.update_in(vcx, |app, window, cx| app.toggle_sidebar(window, cx));
    slide(&app, vcx);
    assert!(!held(&app, vcx));

    // A click on the chat beside it closes it.
    app.update_in(vcx, |app, window, cx| app.toggle_sidebar(window, cx));
    slide(&app, vcx);
    click_at(vcx, 780., 400.);
    assert!(!held(&app, vcx));

    // Escape closes it.
    app.update_in(vcx, |app, window, cx| app.toggle_sidebar(window, cx));
    slide(&app, vcx);
    let dismissed = app.update_in(vcx, |app, window, cx| app.dismiss_topmost_layer(window, cx));
    assert!(dismissed);
    assert!(!held(&app, vcx));

    // Opening a chat closes it.
    app.update_in(vcx, |app, window, cx| app.toggle_sidebar(window, cx));
    slide(&app, vcx);
    let open_now = app.read_with(vcx, |app, _| app.open_chat_id());
    let chat = [ChatId(11), ChatId(12)]
        .into_iter()
        .find(|chat| Some(*chat) != open_now)
        .expect("another chat");
    app.update_in(vcx, |app, window, cx| {
        app.select_listed_chat(chat, window, cx)
    });
    frames(vcx);
    assert!(!held(&app, vcx));

    // The preference was never written.
    assert!(app.read_with(vcx, |app, _| !app.frame.sidebar.hidden()));
}

#[gpui_kit::test]
fn leaving_the_band_drops_the_drawer(cx: &mut TestAppContext) {
    let _prefs = own_appearance_prefs("quill-cards-band");
    let (app, mut vcx) = open(cx, 800., LayoutStyle::Floating);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| app.toggle_sidebar(window, cx));
    slide(&app, vcx);
    assert!(held(&app, vcx));
    vcx.simulate_resize(size(px(1200.), px(800.)));
    frames(vcx);
    assert!(!held(&app, vcx));
    assert!(app.read_with(vcx, |app, _| !app.shell_layout().drawer()));
}
