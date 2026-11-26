//! Floating menus pop in from their corner through a snapshot that lives
//! only for the run; Classic menus never take one.

use super::{POP_FROM, POP_MS, pop_draw, pop_origin};
use crate::ui::app::QuillApp;
use crate::ui::appearance::own_appearance_prefs;
use crate::ui::keybindings::bind_keys;
use crate::ui::menu_states::ChatMenuState;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext, Bounds, Entity, Focusable, TestAppContext, VisualTestContext, point, px, size,
};
use quill::ids::ChatId;
use quill::layout_style::LayoutStyle;
use std::time::Duration;

#[test]
fn a_menu_grows_from_the_corner_it_opened_at() {
    let menu = Bounds::new(point(px(100.), px(100.)), size(px(200.), px(300.)));
    let at = |x: f32, y: f32| pop_origin(menu, point(px(x), px(y)));
    // Opened at the pointer: the menu's top left, the usual case.
    assert_eq!(at(100., 100.), point(px(100.), px(100.)));
    // Moved left by the window's right edge: grows from its top right.
    assert_eq!(at(310., 100.), point(px(300.), px(100.)));
    // Moved up by the bottom edge: bottom left and bottom right.
    assert_eq!(at(100., 420.), point(px(100.), px(400.)));
    assert_eq!(at(310., 420.), point(px(300.), px(400.)));
}

#[test]
fn a_pop_runs_from_point_eight_and_clear_to_the_menu_itself() {
    let menu = Bounds::new(point(px(0.), px(0.)), size(px(100.), px(100.)));
    let start = pop_draw(0., menu, menu.origin);
    assert!((start.scale - POP_FROM).abs() < 1e-4);
    assert!(start.opacity.abs() < 1e-4);
    let end = pop_draw(1., menu, menu.origin);
    assert!((end.scale - 1.).abs() < 1e-4);
    assert!((end.opacity - 1.).abs() < 1e-4);
}

fn new_app(cx: &mut TestAppContext, layout: LayoutStyle) -> (Entity<QuillApp>, VisualTestContext) {
    quill::settings::use_isolated_app_root(quill::scratch::process_dir("quill-menu-pop"));
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1200.), px(800.)), move |window, cx| {
        window.set_test_snapshots_supported(true);
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    app.update(&mut vcx, |app, cx| {
        app.set_appearance(cx, |appearance| appearance.layout = layout);
    });
    frames(&mut vcx);
    vcx.run_until_parked();
    (app, vcx)
}

fn frames(vcx: &mut VisualTestContext) {
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open_menu(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    let chat = app.read_with(vcx, |app, _| {
        let session = app.session().expect("demo session");
        session
            .chats
            .keys()
            .next()
            .map(|id| ChatId(*id))
            .expect("a chat")
    });
    app.update(vcx, |app, cx| {
        app.message_ui
            .menu
            .open_row(ChatMenuState::at(chat, point(px(100.), px(200.))));
        cx.notify();
    });
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.message_ui.menu.row().is_some()));
}

fn snapshots(vcx: &mut VisualTestContext) -> usize {
    vcx.update(|window, _| window.live_snapshot_count())
}

#[gpui_kit::test]
fn a_floating_menu_holds_a_snapshot_only_while_it_pops(cx: &mut TestAppContext) {
    let _prefs = own_appearance_prefs("quill-menu-pop-floating");
    let (app, mut vcx) = new_app(cx, LayoutStyle::Floating);
    let vcx = &mut vcx;
    let before = snapshots(vcx);
    open_menu(&app, vcx);
    assert_eq!(snapshots(vcx), before + 1, "popping");
    vcx.executor()
        .advance_clock(Duration::from_millis(POP_MS + 50));
    frames(vcx);
    frames(vcx);
    assert_eq!(snapshots(vcx), before, "settled: drawn live");
    assert!(app.read_with(vcx, |app, _| app.message_ui.menu.row().is_some()));

    // Closed mid-run: the snapshot goes with the menu.
    app.update(vcx, |app, cx| {
        app.message_ui.menu.close_menu();
        cx.notify();
    });
    frames(vcx);
    open_menu(&app, vcx);
    assert_eq!(snapshots(vcx), before + 1, "a new run");
    app.update(vcx, |app, cx| {
        app.message_ui.menu.close_menu();
        cx.notify();
    });
    frames(vcx);
    frames(vcx);
    assert_eq!(snapshots(vcx), before);
}

#[gpui_kit::test]
fn a_classic_menu_takes_no_snapshot(cx: &mut TestAppContext) {
    let _prefs = own_appearance_prefs("quill-menu-pop-classic");
    let (app, mut vcx) = new_app(cx, LayoutStyle::Classic);
    let vcx = &mut vcx;
    let before = snapshots(vcx);
    open_menu(&app, vcx);
    assert_eq!(snapshots(vcx), before);
}
