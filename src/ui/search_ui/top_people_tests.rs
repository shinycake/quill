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
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-top-people");
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
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
