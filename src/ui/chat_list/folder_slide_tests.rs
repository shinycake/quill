//! Floating slides folder pages by tab order and settles; a second tab
//! mid-run replaces the slide; Classic cuts.

use super::{FOLDER_SLIDE, arriving, leaving};
use crate::ui::app::ChatListFilter;
use crate::ui::app::QuillApp;
use crate::ui::appearance::own_appearance_prefs;
use crate::ui::folder_tabs::FolderSlotKind;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use quill::layout_style::LayoutStyle;
use std::time::Duration;

#[test]
fn pages_move_a_width_by_tab_order() {
    let width = 300.;
    // Forward: the old page leaves left, the new one comes from the right.
    assert_eq!(leaving(1., width, true).translation.x, px(-300.));
    assert_eq!(arriving(0., width, true).translation.x, px(300.));
    // Back: the other way.
    assert_eq!(leaving(1., width, false).translation.x, px(300.));
    assert_eq!(arriving(0., width, false).translation.x, px(-300.));
    // Both land in place.
    assert_eq!(leaving(0., width, true).translation.x, px(0.));
    assert_eq!(arriving(1., width, false).translation.x, px(0.));
}

fn new_app(cx: &mut TestAppContext, layout: LayoutStyle) -> (Entity<QuillApp>, VisualTestContext) {
    quill::settings::use_isolated_app_root(quill::scratch::process_dir("quill-folder-slide"));
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

fn show(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, filter: ChatListFilter) {
    app.update(vcx, |app, cx| {
        app.set_chat_list_view(filter, None);
        cx.notify();
    });
    frames(vcx);
}

fn sliding(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<(FolderSlotKind, bool)> {
    app.read_with(vcx, |app, _| app.chat_list.folder_slide.crossing())
}

fn held(vcx: &mut VisualTestContext) -> bool {
    vcx.update(|window, _| window.try_find(FOLDER_SLIDE.hold).is_some())
}

#[gpui_kit::test]
fn tabs_slide_by_their_order_and_settle(cx: &mut TestAppContext) {
    let _prefs = own_appearance_prefs("quill-folder-slide-order");
    let (app, mut vcx) = new_app(cx, LayoutStyle::Floating);
    let vcx = &mut vcx;
    show(&app, vcx, ChatListFilter::Unread);
    assert_eq!(sliding(&app, vcx), Some((FolderSlotKind::Unread, true)));
    assert!(held(vcx), "the pointer waits while the pages slide");
    // A second tab mid-run replaces the slide: back to All.
    show(&app, vcx, ChatListFilter::All);
    assert_eq!(sliding(&app, vcx), Some((FolderSlotKind::All, false)));
    vcx.executor()
        .advance_clock(FOLDER_SLIDE.duration + Duration::from_millis(20));
    frames(vcx);
    assert_eq!(sliding(&app, vcx), None, "the run ended");
    frames(vcx);
    assert!(!held(vcx));
}

#[gpui_kit::test]
fn classic_folder_pages_cut(cx: &mut TestAppContext) {
    let _prefs = own_appearance_prefs("quill-folder-slide-classic");
    let (app, mut vcx) = new_app(cx, LayoutStyle::Classic);
    let vcx = &mut vcx;
    show(&app, vcx, ChatListFilter::Unread);
    assert_eq!(sliding(&app, vcx), None);
}

#[gpui_kit::test]
fn with_reduced_motion_the_page_cuts(cx: &mut TestAppContext) {
    let _prefs = own_appearance_prefs("quill-folder-slide-reduced");
    let (app, mut vcx) = new_app(cx, LayoutStyle::Floating);
    let vcx = &mut vcx;
    vcx.update(|_, cx| cx.set_reduce_motion(true));
    show(&app, vcx, ChatListFilter::Unread);
    assert_eq!(sliding(&app, vcx), None);
}

#[gpui_kit::test]
fn with_animations_off_the_page_cuts(cx: &mut TestAppContext) {
    let _prefs = own_appearance_prefs("quill-folder-slide-off");
    quill::power_saving::isolate_for_tests();
    let (app, mut vcx) = new_app(cx, LayoutStyle::Floating);
    let vcx = &mut vcx;
    quill::power_saving::set_force_all(true);
    show(&app, vcx, ChatListFilter::Unread);
    assert_eq!(sliding(&app, vcx), None);
}
