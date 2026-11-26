//! Floating crosses from one chat to the next and settles; every way out
//! (the run ending, another chat, no chat, Classic) leaves the stage idle.

use super::{CHAT_SWITCH, arriving, leaving};
use crate::ui::app::QuillApp;
use crate::ui::appearance::own_appearance_prefs;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;
use quill::layout_style::LayoutStyle;
use std::time::Duration;

#[test]
fn the_new_chat_travels_two_hundred_points_over_the_old() {
    let start = (leaving(0., 800., true), arriving(0., 800., true));
    assert_eq!((start.0.translation.x, start.0.opacity), (px(0.), 1.));
    assert_eq!((start.1.translation.x, start.1.opacity), (px(200.), 0.));
    let end = (leaving(1., 800., true), arriving(1., 800., true));
    assert_eq!((end.0.translation.x, end.0.opacity), (px(0.), 0.));
    assert_eq!((end.1.translation.x, end.1.opacity), (px(0.), 1.));
}

fn new_app(cx: &mut TestAppContext, layout: LayoutStyle) -> (Entity<QuillApp>, VisualTestContext) {
    quill::settings::use_isolated_app_root(quill::scratch::process_dir("quill-chat-switch"));
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

fn chats(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Vec<ChatId> {
    app.read_with(vcx, |app, _| {
        let session = app.session().expect("demo session");
        session.chats.keys().map(|id| ChatId(*id)).collect()
    })
}

fn open(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, chat: Option<ChatId>) {
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            match chat {
                Some(chat) => session.open_chat(chat),
                None => session.open = quill::state::OpenView::of(None),
            }
        }
        cx.notify();
    });
    frames(vcx);
}

/// The chat the window is crossing into, if it is.
fn crossing(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<ChatId> {
    app.read_with(vcx, |app, _| {
        app.slices.window.chat_switch.crossing().map(|(to, _)| to)
    })
}

#[gpui_kit::test]
fn floating_crosses_into_the_next_chat_and_settles(cx: &mut TestAppContext) {
    let _prefs = own_appearance_prefs("quill-chat-switch-settles");
    let (app, mut vcx) = new_app(cx, LayoutStyle::Floating);
    let vcx = &mut vcx;
    let ids = chats(&app, vcx);
    let (Some(a), Some(b)) = (ids.first().copied(), ids.get(1).copied()) else {
        panic!("the demo has two chats");
    };
    open(&app, vcx, Some(a));
    vcx.executor().advance_clock(CHAT_SWITCH.duration * 2);
    frames(vcx);
    let before = snapshots(vcx);
    open(&app, vcx, Some(b));
    assert_eq!(crossing(&app, vcx), Some(b));
    assert!(held(vcx), "the pointer waits while the chats cross");
    assert!(snapshots(vcx) > before, "the new chat is drawn through one");
    vcx.executor()
        .advance_clock(CHAT_SWITCH.duration + Duration::from_millis(20));
    frames(vcx);
    assert_eq!(crossing(&app, vcx), None, "the run ended");
    frames(vcx);
    assert!(!held(vcx), "and the pointer is back");
    assert_eq!(snapshots(vcx), before, "every picture dropped");
}

fn snapshots(vcx: &mut VisualTestContext) -> usize {
    vcx.update(|window, _| window.live_snapshot_count())
}

fn held(vcx: &mut VisualTestContext) -> bool {
    vcx.update(|window, _| window.try_find(CHAT_SWITCH.hold).is_some())
}

#[gpui_kit::test]
fn another_chat_or_none_ends_the_crossing(cx: &mut TestAppContext) {
    let _prefs = own_appearance_prefs("quill-chat-switch-exits");
    let (app, mut vcx) = new_app(cx, LayoutStyle::Floating);
    let vcx = &mut vcx;
    let ids = chats(&app, vcx);
    let (Some(a), Some(b), Some(c)) = (
        ids.first().copied(),
        ids.get(1).copied(),
        ids.get(2).copied(),
    ) else {
        panic!("the demo has three chats");
    };
    open(&app, vcx, Some(a));
    open(&app, vcx, Some(b));
    assert_eq!(crossing(&app, vcx), Some(b));
    open(&app, vcx, Some(c));
    assert_eq!(crossing(&app, vcx), Some(c), "a new run replaces the old");
    open(&app, vcx, None);
    assert_eq!(crossing(&app, vcx), None, "no chat: nothing to cross into");
    open(&app, vcx, Some(a));
    assert_eq!(crossing(&app, vcx), None, "opened from no chat: it appears");
}

#[gpui_kit::test]
fn with_animations_off_the_chat_appears(cx: &mut TestAppContext) {
    let _prefs = own_appearance_prefs("quill-chat-switch-off");
    quill::power_saving::isolate_for_tests();
    let (app, mut vcx) = new_app(cx, LayoutStyle::Floating);
    let vcx = &mut vcx;
    quill::power_saving::set_force_all(true);
    let ids = chats(&app, vcx);
    let (Some(a), Some(b)) = (ids.first().copied(), ids.get(1).copied()) else {
        panic!("the demo has two chats");
    };
    open(&app, vcx, Some(a));
    open(&app, vcx, Some(b));
    assert_eq!(crossing(&app, vcx), None);
}

#[gpui_kit::test]
fn with_reduced_motion_the_chat_appears(cx: &mut TestAppContext) {
    let _prefs = own_appearance_prefs("quill-chat-switch-reduced");
    let (app, mut vcx) = new_app(cx, LayoutStyle::Floating);
    let vcx = &mut vcx;
    vcx.update(|_, cx| cx.set_reduce_motion(true));
    let ids = chats(&app, vcx);
    let (Some(a), Some(b)) = (ids.first().copied(), ids.get(1).copied()) else {
        panic!("the demo has two chats");
    };
    open(&app, vcx, Some(a));
    open(&app, vcx, Some(b));
    assert_eq!(crossing(&app, vcx), None);
}

#[gpui_kit::test]
fn classic_cuts(cx: &mut TestAppContext) {
    let _prefs = own_appearance_prefs("quill-chat-switch-classic");
    let (app, mut vcx) = new_app(cx, LayoutStyle::Classic);
    let vcx = &mut vcx;
    let ids = chats(&app, vcx);
    let (Some(a), Some(b)) = (ids.first().copied(), ids.get(1).copied()) else {
        panic!("the demo has two chats");
    };
    open(&app, vcx, Some(a));
    open(&app, vcx, Some(b));
    assert_eq!(crossing(&app, vcx), None);
}
