//! The gallery's search field: what the panel shows for a query, and that
//! leaving the tab empties both (needs the `demo-capture` feature for
//! gpui-kit's test support).
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::state::{SharedMediaTab, SharedMediaTabStatus};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
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
                Some(ScreenshotDemo::named("ready-shared-media-search")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let Some(app) = slot.borrow().clone() else {
        panic!("the window opened");
    };
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

fn frame(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    vcx.update(|window, cx| window.render_frame(cx));
}

fn seed(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update_in(vcx, |app, window, cx| {
        app.demo_shared_media_search(window, cx);
    });
    frame(vcx);
}

fn type_into_field(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, text: &'static str) {
    app.update_in(vcx, |app, window, cx| {
        app.media_search
            .input
            .update(cx, |input, cx| input.set_value(text, window, cx));
        // A programmatic value raises no edit event; hand the text to the
        // handler the field's edit event calls.
        app.shared_media_search_typed(text, cx);
    });
    frame(vcx);
}

fn query(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> String {
    app.read_with(vcx, |app, _| {
        app.session()
            .map(|s| s.media.shared_media.active_query().to_string())
            .unwrap_or_default()
    })
}

fn field_text(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> String {
    app.read_with(vcx, |app, cx| {
        app.media_search.input.read(cx).value().to_string()
    })
}

#[gpui_kit::test]
fn the_files_tab_shows_its_search_field_and_the_results(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx);
    assert!(vcx.debug_bounds("shared-media-search").is_some());
    assert!(vcx.debug_bounds("shared-media-search-empty").is_none());
    assert_eq!(query(&app, vcx), "report");
    assert_eq!(field_text(&app, vcx), "report");
}

#[gpui_kit::test]
fn a_query_without_results_shows_the_search_empty_state(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx);
    app.update_in(vcx, |app, _, cx| {
        let tab = SharedMediaTab::Files.index();
        if let Some(session) = app.demo_session.as_mut()
            && let Some(state) = session.media.shared_media.tabs.get_mut(tab)
        {
            state.items.clear();
            state.status = SharedMediaTabStatus::Empty;
            state.query = "zzz".into();
        }
        cx.notify();
    });
    frame(vcx);
    assert!(vcx.debug_bounds("shared-media-search-empty").is_some());
    assert!(vcx.debug_bounds("shared-media-search").is_some());
}

#[gpui_kit::test]
fn typing_sets_the_tab_query(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx);
    type_into_field(&app, vcx, "budget");
    assert_eq!(query(&app, vcx), "budget");
}

/// The info panel's media rows change the tab without the tab strip.
#[gpui_kit::test]
fn a_tab_change_from_anywhere_leaves_the_field_empty_and_searches_nothing(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx);
    type_into_field(&app, vcx, "budget");
    app.update_in(vcx, |app, _, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session
                .media
                .shared_media
                .select_tab_dropping_search(SharedMediaTab::Links);
        }
        // Text typed for Files, arriving after the tab changed.
        app.shared_media_search_typed("budgetx", cx);
        cx.notify();
    });
    frame(vcx);
    assert_eq!(field_text(&app, vcx), "");
    assert_eq!(query(&app, vcx), "");
    let files_query = app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|s| s.media.shared_media.tab_at(SharedMediaTab::Files))
            .map(|state| state.query.clone())
    });
    assert_eq!(files_query.as_deref(), Some(""));
}

#[gpui_kit::test]
fn a_tab_without_search_has_no_field_and_the_text_is_dropped(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx);
    type_into_field(&app, vcx, "budget");
    app.update_in(vcx, |app, _, cx| {
        app.select_shared_media_tab_ui(SharedMediaTab::Voice, cx);
    });
    frame(vcx);
    assert!(vcx.debug_bounds("shared-media-search").is_none());
    assert_eq!(field_text(&app, vcx), "");
}
