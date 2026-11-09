//! Landing on a setting from search, in the real settings window.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::agent::access;
use quill::ids::AccountKey;
use quill::settings_catalog::{self, Page, Platforms, Target};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    let root = quill::scratch::process_dir("quill-settings-arrival");
    let _ = std::fs::create_dir_all(&root);
    quill::settings::use_isolated_app_root(root);
    cx.update(gpui_kit::init);
    cx.update(crate::ui::bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("app view");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

/// Opens the result for `id` and renders until the page has settled.
fn land(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, id: &str) {
    let target = settings_catalog::target(id).expect("catalog id");
    app.update(vcx, |app, cx| {
        app.open_settings(None, cx);
        app.open_search_target(target, cx);
    });
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

#[gpui_kit::test]
fn a_result_centers_its_row_on_the_page(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    land(&app, vcx, "notifications.reset_all");
    app.read_with(vcx, |app, _| {
        let arrival = app.settings.arrival.as_ref().expect("arrival");
        let row = arrival.landed_row().expect("the row was found");
        let viewport = app.settings.page_scroll.bounds();
        assert!(app.settings.page_scroll.offset().y < px(0.));
        assert!(row.top() >= viewport.top() && row.bottom() <= viewport.bottom());
    });
}

#[gpui_kit::test]
fn a_page_result_opens_at_the_top_without_a_highlight(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    land(&app, vcx, "notifications.reset_all");
    land(&app, vcx, "privacy");
    app.read_with(vcx, |app, _| {
        assert!(app.settings.arrival.is_none());
        assert_eq!(app.settings.page_scroll.offset().y, px(0.));
    });
}

#[gpui_kit::test]
fn reopening_settings_starts_at_the_top(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    land(&app, vcx, "notifications.reset_all");
    app.update(vcx, |app, _| app.close_settings());
    app.read_with(vcx, |app, _| {
        assert_eq!(app.settings.page_scroll.offset().y, px(0.));
    });
}

#[gpui_kit::test]
fn every_setting_has_a_place_to_land(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let mut lost = Vec::new();
    for entry in settings_catalog::all() {
        if !entry.platforms.contains(Platforms::current()) || !Target::Entry(entry).shown(false) {
            continue;
        }
        land(&app, vcx, entry.id);
        let found = app.read_with(vcx, |app, _| {
            app.settings
                .arrival
                .as_ref()
                .is_some_and(|arrival| arrival.landed_row().is_some())
        });
        if !found {
            lost.push(entry.id);
        }
    }
    assert!(lost.is_empty(), "no spot for {lost:?}");
}

#[gpui_kit::test]
fn every_agent_access_setting_has_a_place_to_land(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&crate::ui::agent_state::TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    crate::ui::agent_state::reset_for_test(&AccountKey::primary());
    access::set_enabled(true);
    let mut lost = Vec::new();
    let mut tried = 0;
    for entry in settings_catalog::all() {
        if entry.page != Page::AgentAccess || !Target::Entry(entry).shown(true) {
            continue;
        }
        tried += 1;
        land(&app, vcx, entry.id);
        let found = app.read_with(vcx, |app, _| {
            app.settings
                .arrival
                .as_ref()
                .is_some_and(|arrival| arrival.landed_row().is_some())
        });
        if !found {
            lost.push(entry.id);
        }
    }
    access::set_enabled(false);
    assert!(tried > 0);
    assert!(lost.is_empty(), "no spot for {lost:?}");
}
