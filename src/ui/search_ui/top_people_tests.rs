#![cfg(test)]
#![cfg(feature = "demo-capture")]
//! The top-people strip of the empty search through GPUI: the keys walk
//! the tiles and the results, a tile opens its chat, the header link
//! expands the grid, and the menu removes a person or disables the strip.

use crate::ui::app::QuillApp;
use crate::ui::chat_list_keys::SearchRow;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext, px, size,
};
use quill::ids::ChatId;
use quill::state::SearchConfirm;
use quill::top_people::StripView;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    open_sized(cx, 1100., 800.)
}

fn open_sized(
    cx: &mut TestAppContext,
    width: f32,
    height: f32,
) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-top-people");
    let handle = cx.open_window(size(px(width), px(height)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    app.update_in(&mut vcx, |app, window, cx| app.open_search_ui(window, cx));
    frames(&mut vcx);
    (app, vcx)
}

fn press(vcx: &mut VisualTestContext, keys: &str) {
    vcx.simulate_keystrokes(keys);
    frames(vcx);
}

fn selected(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<SearchRow> {
    app.update(vcx, |app, _| app.chat_list.keys.search_highlight())
}

fn strip(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Vec<ChatId> {
    app.update(vcx, |app, _| {
        app.session()
            .map(|s| s.search.search.top_chats.clone())
            .unwrap_or_default()
    })
}

#[gpui_kit::test]
fn down_enters_the_strip_and_walks_it_then_leaves_to_recent(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let tiles = strip(&app, vcx);
    assert!(tiles.len() >= 6);
    assert_eq!(selected(&app, vcx), None);
    press(vcx, "down");
    assert_eq!(
        selected(&app, vcx),
        tiles.first().map(|id| SearchRow::Top(*id))
    );
    press(vcx, "right");
    assert_eq!(
        selected(&app, vcx),
        tiles.get(1).map(|id| SearchRow::Top(*id))
    );
    press(vcx, "left");
    assert_eq!(
        selected(&app, vcx),
        tiles.first().map(|id| SearchRow::Top(*id))
    );
    // Down leaves a collapsed strip for the first recent chat.
    press(vcx, "down");
    assert_eq!(selected(&app, vcx), Some(SearchRow::Chat(ChatId(11))));
    // Up from the first recent chat goes back to the strip.
    press(vcx, "up");
    assert_eq!(
        selected(&app, vcx),
        tiles.first().map(|id| SearchRow::Top(*id))
    );
    // Up from the strip returns to the field.
    press(vcx, "up");
    assert_eq!(selected(&app, vcx), None);
}

#[gpui_kit::test]
fn enter_opens_the_chat_of_the_selected_tile(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let tiles = strip(&app, vcx);
    press(vcx, "down");
    press(vcx, "right");
    press(vcx, "enter");
    let open = app.update(vcx, |app, _| app.session().and_then(|s| s.open_chat));
    assert_eq!(open, tiles.get(1).copied());
}

#[gpui_kit::test]
fn left_and_right_keep_the_caret_once_text_is_typed(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    vcx.simulate_input("a");
    frames(vcx);
    press(vcx, "left");
    assert_eq!(selected(&app, vcx), None);
}

#[gpui_kit::test]
fn the_header_link_expands_and_collapses_the_grid(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    assert_eq!(view(&app, vcx), StripView::Initial);
    click(vcx, "top-toggle");
    assert_eq!(view(&app, vcx), StripView::Expanded);
    click(vcx, "top-toggle");
    assert_eq!(view(&app, vcx), StripView::Collapsed);
}

fn click(vcx: &mut VisualTestContext, selector: &'static str) {
    let bounds = vcx.debug_bounds(selector).expect(selector);
    vcx.simulate_click(bounds.center(), Modifiers::default());
    frames(vcx);
}

fn view(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> StripView {
    app.update(vcx, |app, _| {
        app.session()
            .map(|s| s.search.search.top_view)
            .unwrap_or_default()
    })
}

#[gpui_kit::test]
fn clicking_a_tile_opens_its_chat(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    click(vcx, "top-tile-32");
    let open = app.update(vcx, |app, _| app.session().and_then(|s| s.open_chat));
    assert_eq!(open, Some(ChatId(32)));
}

#[gpui_kit::test]
fn the_menu_removes_one_person_or_asks_before_disabling(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let tiles = strip(&app, vcx);
    let at = vcx.debug_bounds("top-tile-32").expect("tile").center();
    vcx.simulate_mouse_down(at, MouseButton::Right, Modifiers::default());
    frames(vcx);
    let menu = app.update(vcx, |app, _| app.chat_list.menu);
    assert_eq!(
        menu.map(|m| (m.chat_id, m.top_person)),
        Some((ChatId(32), true))
    );
    click(vcx, "top-menu-remove");
    let left = strip(&app, vcx);
    assert_eq!(left.len(), tiles.len() - 1);
    assert!(!left.contains(&ChatId(32)));
    assert!(app.update(vcx, |app, _| app.chat_list.menu.is_none()));

    let at = vcx.debug_bounds("top-tile-31").expect("tile").center();
    vcx.simulate_mouse_down(at, MouseButton::Right, Modifiers::default());
    frames(vcx);
    click(vcx, "top-menu-disable");
    let confirm = app.update(vcx, |app, _| {
        app.session().and_then(|s| s.search.search.confirm)
    });
    assert_eq!(confirm, Some(SearchConfirm::DisableTopChats));
    app.update(vcx, |app, cx| app.accept_search_confirm(cx));
    frames(vcx);
    assert!(strip(&app, vcx).is_empty());
}

fn open_chat(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<ChatId> {
    app.update(vcx, |app, _| app.session().and_then(|s| s.open_chat))
}

/// Press on a tile, drag it by `dx` with the left button down, release.
fn drag_from(vcx: &mut VisualTestContext, selector: &'static str, dx: f32) {
    let from = vcx.debug_bounds(selector).expect(selector).center();
    let to = from + gpui_kit::point(px(dx), px(0.));
    vcx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_move(to, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    frames(vcx);
}

#[gpui_kit::test]
fn a_drag_that_ends_on_its_tile_does_not_open_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    // Pressed and released over the same tile, but moved: a scroll.
    let before = open_chat(&app, vcx);
    drag_from(vcx, "top-tile-32", 12.);
    assert_eq!(open_chat(&app, vcx), before);
}

#[gpui_kit::test]
fn tiles_still_open_after_a_drag_and_an_expand(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let before = open_chat(&app, vcx);
    drag_from(vcx, "top-tile-33", -120.);
    assert_eq!(open_chat(&app, vcx), before);
    click(vcx, "top-toggle");
    std::thread::sleep(std::time::Duration::from_millis(300));
    frames(vcx);
    click(vcx, "top-tile-34");
    assert_eq!(open_chat(&app, vcx), Some(ChatId(34)));
}

/// The strip is as wide as the list as laid out less the sidebar's
/// padding, and the expanded grid shows every row it has.
fn strip_fits(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    let strip = vcx.debug_bounds("top-strip").expect("strip");
    let list = app.update(vcx, |app, _| app.list_width());
    assert_eq!(strip.size.width, list - px(25.));
    click(vcx, "top-toggle");
    std::thread::sleep(std::time::Duration::from_millis(300));
    frames(vcx);
    let strip = vcx.debug_bounds("top-strip").expect("strip");
    let ids = self::strip(app, vcx);
    let last = ids.last().expect("people");
    let tile = vcx
        .debug_bounds(Box::leak(format!("top-tile-{}", last.0).into_boxed_str()))
        .expect("last tile");
    assert!(tile.bottom() <= strip.bottom(), "{tile:?} in {strip:?}");
    assert!(tile.right() <= strip.right() + px(1.));
}

#[gpui_kit::test]
fn the_strip_fits_a_narrowed_list(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.frame.sidebar_width = px(300.);
        cx.notify();
    });
    frames(vcx);
    strip_fits(&app, vcx);
}

#[gpui_kit::test]
fn the_strip_fits_the_one_column_layout(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_sized(cx, 520., 800.);
    let vcx = &mut vcx;
    // One column shows the list only while no chat is open.
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.open_chat = None;
        }
        cx.notify();
    });
    frames(vcx);
    strip_fits(&app, vcx);
}

#[gpui_kit::test]
fn only_right_enters_the_strip_and_a_hover_does_not_count(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let tiles = strip(&app, vcx);
    press(vcx, "left");
    assert_eq!(selected(&app, vcx), None);
    press(vcx, "right");
    assert_eq!(
        selected(&app, vcx),
        tiles.first().map(|id| SearchRow::Top(*id))
    );
    // Back to the field, then rest the pointer on a tile: Up does nothing.
    press(vcx, "up");
    assert_eq!(selected(&app, vcx), None);
    let at = vcx.debug_bounds("top-tile-32").expect("tile").center();
    vcx.simulate_mouse_move(at, Option::<MouseButton>::None, Modifiers::default());
    frames(vcx);
    press(vcx, "up");
    assert_eq!(selected(&app, vcx), None);
}

#[gpui_kit::test]
fn walking_a_tall_grid_scrolls_only_to_the_tile(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_sized(cx, 1100., 380.);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.frame.sidebar_width = px(300.);
        cx.notify();
    });
    frames(vcx);
    click(vcx, "top-toggle");
    std::thread::sleep(std::time::Duration::from_millis(300));
    frames(vcx);
    for _ in 0..3 {
        press(vcx, "down");
    }
    let last = strip(&app, vcx).last().copied().expect("people");
    let (viewport, offset) = app.update(vcx, |app, _| {
        (
            app.search_ui.results_scroll.bounds(),
            app.search_ui.results_scroll.offset(),
        )
    });
    let tile = vcx
        .debug_bounds(Box::leak(format!("top-tile-{}", last.0).into_boxed_str()))
        .expect("last tile");
    assert!(
        tile.bottom() <= viewport.bottom(),
        "{tile:?} in {viewport:?}"
    );
    assert!(offset.y < px(0.), "the results scrolled down to it");
    // Walking back up shows the field again rather than jumping around.
    for _ in 0..3 {
        press(vcx, "up");
    }
    let top = app.update(vcx, |app, _| app.search_ui.results_scroll.offset().y);
    assert!(top >= offset.y);
}

#[gpui_kit::test]
fn the_menu_has_a_profile_row_for_everyone(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let at = vcx.debug_bounds("top-tile-32").expect("tile").center();
    vcx.simulate_mouse_down(at, MouseButton::Right, Modifiers::default());
    frames(vcx);
    assert!(vcx.debug_bounds("top-menu-profile").is_some());
    click(vcx, "top-menu-profile");
    assert!(app.update(vcx, |app, _| app.chat_list.menu.is_none()));
}
