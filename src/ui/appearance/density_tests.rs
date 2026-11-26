//! Compact mode in the UI: the Appearance switch saves the density and every
//! surface follows it (the kit title bar, the chat rows at their declared
//! height), the chat list keeps its top row, and the sizes the sidebar header
//! fit reckons with are the ones the kit draws.

use crate::ui::app::QuillApp;
use crate::ui::appearance::own_appearance_prefs;
use crate::ui::chat_list_keys::ListRow;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::{ActiveTheme, Root};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Bounds, Entity, Pixels, TestAppContext, VisualTestContext, px, size};
use quill::density::Density;
use quill::ids::ChatId;
use quill::settings_catalog::Page;

fn new_app(cx: &mut TestAppContext, height: f32) -> (Entity<QuillApp>, VisualTestContext) {
    // Saved to a scratch root, never the installed app's settings.
    quill::settings::use_isolated_app_root(quill::scratch::process_dir("quill-density-ui"));
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    crate::ui::interface_zoom::set_initial_zoom(1.);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    let handle = cx.open_window(size(px(1100.), px(height)), move |window, cx| {
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

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn bounds(vcx: &mut VisualTestContext, id: impl Into<gpui_kit::ElementId>) -> Bounds<Pixels> {
    let id = id.into();
    vcx.update(|window, _| {
        let target = window.find(id.clone());
        assert!(target.visible(), "{id:?} is not visible");
        target.bounds()
    })
}

fn chat_row(chat: i64) -> (&'static str, u64) {
    ("chat-row", chat as u64)
}

/// The height the chat list declared to its `VirtualList` for `chat`'s row.
fn declared(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, chat: i64) -> Pixels {
    app.read_with(vcx, |app, _| {
        app.chat_list
            .items
            .iter()
            .find(|item| item.list_row() == Some(ListRow::Chat(ChatId(chat))))
            .map(|item| item.height())
            .expect("the chat is listed")
    })
}

fn set_density(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, density: Density) {
    app.update(vcx, |app, cx| {
        app.set_appearance(cx, |appearance| appearance.density = density);
    });
    frames(vcx);
}

#[gpui_kit::test]
fn the_compact_switch_saves_the_density_and_the_app_follows_it(cx: &mut TestAppContext) {
    let _prefs = own_appearance_prefs("quill-density-switch");
    // Tall enough for the Appearance page down to the compact switch.
    let (app, mut vcx) = new_app(cx, 1900.);
    let vcx = &mut vcx;
    let standard_row = bounds(vcx, chat_row(11)).size.height;
    assert_eq!(standard_row, declared(&app, vcx, 11));
    assert_eq!(
        bounds(vcx, "title-bar").size.height,
        px(quill::density::STANDARD.title_bar)
    );

    app.update(vcx, |app, cx| app.open_settings(Some(Page::Appearance), cx));
    frames(vcx);
    vcx.update(|window, cx| window.click("appearance-compact-mode", cx));
    frames(vcx);

    // Saved, and the kit draws the title bar (and keeps its overlays clear
    // of it) at the compact height.
    assert_eq!(QuillApp::load_appearance().density, Density::Compact);
    let compact = quill::density::COMPACT;
    assert_eq!(
        vcx.update(|_, cx| cx.theme().title_bar_height),
        px(compact.title_bar)
    );
    assert_eq!(bounds(vcx, "title-bar").size.height, px(compact.title_bar));

    vcx.simulate_keystrokes("escape");
    frames(vcx);
    let row = bounds(vcx, chat_row(11)).size.height;
    assert_eq!(row, declared(&app, vcx, 11));
    assert!(row < standard_row, "{row:?} vs {standard_row:?}");

    // And back.
    app.update(vcx, |app, cx| app.open_settings(Some(Page::Appearance), cx));
    frames(vcx);
    vcx.update(|window, cx| window.click("appearance-compact-mode", cx));
    frames(vcx);
    assert_eq!(QuillApp::load_appearance().density, Density::Standard);
    vcx.simulate_keystrokes("escape");
    frames(vcx);
    assert_eq!(bounds(vcx, chat_row(11)).size.height, standard_row);
    assert_eq!(
        bounds(vcx, "title-bar").size.height,
        px(quill::density::STANDARD.title_bar)
    );
}

#[gpui_kit::test]
fn the_chat_list_keeps_its_top_row_through_a_density_change(cx: &mut TestAppContext) {
    let _prefs = own_appearance_prefs("quill-density-top-row");
    let (app, mut vcx) = new_app(cx, 600.);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            crate::ui::demo::add_stress_chats(session, 60);
        }
        app.notify_sidebar(cx);
        cx.notify();
    });
    frames(vcx);
    // Scroll a quarter of the way into the twentieth item.
    let (row, offset) = app.read_with(vcx, |app, _| {
        let items = &app.chat_list.items;
        let above: f32 = items.iter().take(19).map(|i| f32::from(i.height())).sum();
        let item = items.get(19);
        let into = item.map_or(0., |i| f32::from(i.height()) / 4.);
        (item.and_then(|item| item.list_row()), above + into)
    });
    let Some(ListRow::Chat(chat)) = row else {
        panic!("the twentieth item is a chat: {row:?}");
    };
    app.update(vcx, |app, _| {
        app.chat_list
            .scroll
            .set_offset(gpui_kit::point(px(0.), px(-offset)));
    });
    frames(vcx);
    // How far the list's top edge is into that row, as a share of it.
    let top = |vcx: &mut VisualTestContext| {
        let list = app.read_with(vcx, |app, _| app.chat_list.scroll.bounds());
        let row = bounds(vcx, chat_row(chat.0));
        (list.top() - row.top()) / row.size.height
    };
    let before = top(vcx);
    assert!((before - 0.25).abs() < 0.01, "{before}");

    for density in [Density::Compact, Density::Standard] {
        set_density(&app, vcx, density);
        let now = top(vcx);
        assert!(
            (now - before).abs() < 0.01,
            "{density:?}: {now} vs {before}"
        );
    }
}

/// The top row is noted in one list and belongs to it: a density change
/// that lands while the list is not rebuilt (no frame yet, or the rows are
/// off screen) is never put back on top of another folder, category or tab.
#[gpui_kit::test]
fn a_noted_top_row_ends_with_the_list_it_was_noted_in(cx: &mut TestAppContext) {
    let _prefs = own_appearance_prefs("quill-density-top-row-view");
    let (app, mut vcx) = new_app(cx, 600.);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            crate::ui::demo::add_stress_chats(session, 60);
        }
        app.notify_sidebar(cx);
        cx.notify();
    });
    frames(vcx);
    let scroll_down = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.update(vcx, |app, _| {
            app.chat_list
                .scroll
                .set_offset(gpui_kit::point(px(0.), px(-1_000.)));
        });
        frames(vcx);
    };
    let noted = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| app.chat_list.keep_top.is_some())
    };
    scroll_down(&app, vcx);

    // Another category before the list is built again.
    app.update(vcx, |app, cx| {
        app.set_appearance(cx, |appearance| appearance.density = Density::Compact);
        assert!(app.chat_list.keep_top.is_some(), "the top row is noted");
        app.set_chat_list_view(crate::ui::app::ChatListFilter::Unread, None);
    });
    assert!(!noted(&app, vcx), "another view drops it");

    // The rows leave the screen (another tab) before they are built again.
    app.update(vcx, |app, _| {
        app.set_chat_list_view(crate::ui::app::ChatListFilter::All, None);
    });
    frames(vcx);
    scroll_down(&app, vcx);
    app.update(vcx, |app, cx| {
        app.set_appearance(cx, |appearance| appearance.density = Density::Standard);
        assert!(app.chat_list.keep_top.is_some(), "the top row is noted");
        app.chat_list
            .show_tab(crate::ui::chat_list_state::SidebarTab::Contacts);
    });
    assert!(!noted(&app, vcx), "leaving the rows drops it");
}

#[gpui_kit::test]
fn the_header_fit_reckons_with_the_sizes_the_kit_draws(cx: &mut TestAppContext) {
    let _prefs = own_appearance_prefs("quill-density-header");
    let (app, mut vcx) = new_app(cx, 800.);
    let vcx = &mut vcx;
    for density in [Density::Standard, Density::Compact] {
        set_density(&app, vcx, density);
        let m = density.metrics();
        // The window's left edge, the list's (inside the sidebar's inset),
        // and the menu button leading the header at the same inset.
        let window = bounds(vcx, "title-bar");
        let list = app.read_with(vcx, |app, _| app.chat_list.scroll.bounds());
        let menu = bounds(vcx, "main-menu");
        assert_eq!(menu.size.width, px(m.icon_button), "{density:?}");
        assert_eq!(
            list.left() - window.left(),
            px(m.sidebar_inset_x),
            "{density:?}"
        );
        assert_eq!(menu.left(), list.left(), "{density:?}");
    }
}
