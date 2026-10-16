#![cfg(test)]
#![cfg(feature = "demo-capture")]
//! Chat list keyboard navigation through GPUI's real key dispatch: the
//! arrows, page keys, Home, End, Enter and Escape, in the list and in the
//! search results.

use crate::ui::app::QuillApp;
use crate::ui::chat_list_keys::{ListRow, SearchRow};
use crate::ui::chat_row::ChatListItem;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open(cx: &mut TestAppContext, demo: &str, height: f32) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named(demo);
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

/// The ready-chats demo plus `extra` stress chats, so the list scrolls.
fn open_long(
    cx: &mut TestAppContext,
    height: f32,
    extra: usize,
) -> (Entity<QuillApp>, VisualTestContext) {
    let (app, mut vcx) = open(cx, "ready-chats", height);
    app.update(&mut vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            crate::ui::demo::add_stress_chats(session, extra);
        }
        app.notify_sidebar(cx);
        cx.notify();
    });
    frames(&mut vcx);
    (app, vcx)
}

fn focus_list(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update_in(vcx, |app, window, cx| window.focus(&app.focus_sidebar, cx));
    frames(vcx);
}

fn press(vcx: &mut VisualTestContext, keys: &str) {
    vcx.simulate_keystrokes(keys);
    frames(vcx);
}

/// The chat rows as listed, top to bottom.
fn listed(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Vec<ChatId> {
    app.update(vcx, |app, _| {
        app.chat_list
            .items
            .iter()
            .filter_map(|item| match item {
                ChatListItem::Chat { id, .. } => Some(*id),
                _ => None,
            })
            .collect()
    })
}

fn highlighted(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<ChatId> {
    let ids = listed(app, vcx);
    app.update(vcx, |app, _| {
        ids.into_iter()
            .find(|id| app.chat_list.keys.list_highlighted(ListRow::Chat(*id)))
    })
}

fn open_chat(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<ChatId> {
    app.update(vcx, |app, _| app.session().and_then(|s| s.open_chat))
}

fn list_focused(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.update_in(vcx, |app, window, _| app.focus_sidebar.is_focused(window))
}

fn composer_focused(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.update_in(vcx, |app, window, cx| {
        app.composer.read(cx).focus_handle(cx).is_focused(window)
    })
}

#[gpui_kit::test]
fn down_and_up_move_the_highlight(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-chats", 800.);
    let vcx = &mut vcx;
    let ids = listed(&app, vcx);
    assert!(ids.len() >= 3, "fixture lists {} chats", ids.len());
    focus_list(&app, vcx);
    assert_eq!(highlighted(&app, vcx), None);
    press(vcx, "down");
    assert_eq!(highlighted(&app, vcx), ids.first().copied());
    press(vcx, "down");
    press(vcx, "down");
    assert_eq!(highlighted(&app, vcx), ids.get(2).copied());
    press(vcx, "up");
    assert_eq!(highlighted(&app, vcx), ids.get(1).copied());
    // The keys only move the highlight: nothing opened.
    assert!(list_focused(&app, vcx));
}

#[gpui_kit::test]
fn up_from_nothing_selects_the_first_row(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-chats", 800.);
    let vcx = &mut vcx;
    let ids = listed(&app, vcx);
    focus_list(&app, vcx);
    press(vcx, "up");
    assert_eq!(highlighted(&app, vcx), ids.first().copied());
    press(vcx, "up");
    assert_eq!(highlighted(&app, vcx), ids.first().copied());
}

#[gpui_kit::test]
fn home_and_end_jump_to_the_ends(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-chats", 800.);
    let vcx = &mut vcx;
    let ids = listed(&app, vcx);
    focus_list(&app, vcx);
    press(vcx, "down");
    press(vcx, "end");
    assert_eq!(highlighted(&app, vcx), ids.last().copied());
    press(vcx, "home");
    assert_eq!(highlighted(&app, vcx), ids.first().copied());
}

#[gpui_kit::test]
fn end_scrolls_the_virtual_list_to_the_row(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_long(cx, 420., 40);
    let vcx = &mut vcx;
    focus_list(&app, vcx);
    press(vcx, "down");
    let top = app.update(vcx, |app, _| app.chat_list.scroll.offset().y);
    assert_eq!(top, px(0.));
    press(vcx, "end");
    let bottom = app.update(vcx, |app, _| app.chat_list.scroll.offset().y);
    assert!(bottom < px(0.), "End did not scroll the list ({bottom:?})");
    press(vcx, "home");
    let back = app.update(vcx, |app, _| app.chat_list.scroll.offset().y);
    assert_eq!(back, px(0.));
}

#[gpui_kit::test]
fn page_keys_move_by_a_page(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_long(cx, 420., 40);
    let vcx = &mut vcx;
    let ids = listed(&app, vcx);
    focus_list(&app, vcx);
    let page = app.update(vcx, |app, _| {
        crate::ui::chat_list_keys::rows_per_page(
            app.chat_list.scroll.bounds().size.height,
            crate::ui::chat_row::chat_row_height(&[], app.appearance.preview_lines),
        )
    });
    assert!(page >= 2, "page of {page} rows");
    // Nothing selected: Page Down selects the first row and walks on.
    press(vcx, "pagedown");
    let expected = ids.get((page - 1).min(ids.len() - 1)).copied();
    assert_eq!(highlighted(&app, vcx), expected);
    press(vcx, "pageup");
    assert_eq!(highlighted(&app, vcx), ids.first().copied());
    let scrolled = app.update(vcx, |app, _| app.chat_list.scroll.offset().y);
    assert_eq!(scrolled, px(0.));
}

#[gpui_kit::test]
fn enter_opens_the_highlighted_chat_and_focuses_the_composer(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-chats", 800.);
    let vcx = &mut vcx;
    let ids = listed(&app, vcx);
    focus_list(&app, vcx);
    press(vcx, "down");
    press(vcx, "down");
    let target = ids.get(1).copied();
    assert_ne!(open_chat(&app, vcx), target);
    press(vcx, "enter");
    assert_eq!(open_chat(&app, vcx), target);
    assert!(composer_focused(&app, vcx));
    // tdesktop keeps the selection on the row it opened.
    assert_eq!(highlighted(&app, vcx), target);
}

#[gpui_kit::test]
fn enter_with_nothing_highlighted_opens_the_first_chat(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-chats", 800.);
    let vcx = &mut vcx;
    let ids = listed(&app, vcx);
    focus_list(&app, vcx);
    press(vcx, "enter");
    assert_eq!(open_chat(&app, vcx), ids.first().copied());
    assert_eq!(highlighted(&app, vcx), ids.first().copied());
}

#[gpui_kit::test]
fn escape_from_the_composer_closes_the_chat_and_focuses_the_list(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-chats", 800.);
    let vcx = &mut vcx;
    let ids = listed(&app, vcx);
    focus_list(&app, vcx);
    press(vcx, "down");
    press(vcx, "down");
    press(vcx, "enter");
    let opened = ids.get(1).copied();
    assert_eq!(open_chat(&app, vcx), opened);
    assert!(composer_focused(&app, vcx));
    press(vcx, "escape");
    assert_eq!(open_chat(&app, vcx), None);
    assert!(list_focused(&app, vcx));
    // The chat it came from stays highlighted, so Down continues from it.
    assert_eq!(highlighted(&app, vcx), opened);
    press(vcx, "down");
    assert_eq!(highlighted(&app, vcx), ids.get(2).copied());
}

#[gpui_kit::test]
fn escape_in_the_list_goes_back_to_the_open_chat(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-chats", 800.);
    let vcx = &mut vcx;
    focus_list(&app, vcx);
    press(vcx, "enter");
    let opened = open_chat(&app, vcx);
    assert!(opened.is_some());
    focus_list(&app, vcx);
    press(vcx, "escape");
    assert_eq!(open_chat(&app, vcx), opened);
    assert!(composer_focused(&app, vcx));
}

#[gpui_kit::test]
fn arrows_in_the_composer_leave_the_list_alone(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-chats-composer", 800.);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        app.composer.update(cx, |input, cx| input.focus(window, cx));
    });
    frames(vcx);
    press(vcx, "down");
    press(vcx, "end");
    assert_eq!(highlighted(&app, vcx), None);
    assert!(composer_focused(&app, vcx));
}

#[gpui_kit::test]
fn a_mouse_move_over_the_list_hands_the_selection_back(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-chats", 800.);
    let vcx = &mut vcx;
    focus_list(&app, vcx);
    press(vcx, "down");
    assert!(highlighted(&app, vcx).is_some());
    let bounds = app.update(vcx, |app, _| app.chat_list.scroll.bounds());
    vcx.simulate_mouse_move(bounds.center(), None, gpui_kit::Modifiers::default());
    frames(vcx);
    assert_eq!(highlighted(&app, vcx), None);
}

#[gpui_kit::test]
fn the_list_shows_focus_only_from_the_keyboard(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-chats", 800.);
    let vcx = &mut vcx;
    focus_list(&app, vcx);
    let pointer = app.update_in(vcx, |app, window, _| app.chat_list_focus_visible(window));
    assert!(!pointer);
    press(vcx, "down");
    let keyboard = app.update_in(vcx, |app, window, _| app.chat_list_focus_visible(window));
    assert!(keyboard);
}

fn search_highlight(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<SearchRow> {
    app.update(vcx, |app, _| app.chat_list.keys.search_highlight())
}

#[gpui_kit::test]
fn arrows_in_the_search_field_walk_the_results_and_enter_opens_one(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-chats", 800.);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| app.open_search_ui(window, cx));
    frames(vcx);
    // The demo filters its chats by title locally.
    vcx.simulate_input("a");
    frames(vcx);
    let order = app.update(vcx, |app, _| app.chat_list.keys.search_rows());
    assert!(order.len() >= 2, "{} search rows", order.len());
    press(vcx, "down");
    assert_eq!(search_highlight(&app, vcx), order.first().copied());
    press(vcx, "down");
    assert_eq!(search_highlight(&app, vcx), order.get(1).copied());
    press(vcx, "up");
    assert_eq!(search_highlight(&app, vcx), order.first().copied());
    let target = match order.first().copied().expect("a result") {
        SearchRow::Chat(id) | SearchRow::PublicChat(id) | SearchRow::Message(id, _) => id,
    };
    press(vcx, "enter");
    assert_eq!(open_chat(&app, vcx), Some(target));
    assert!(!app.update(vcx, |app, _| app.search_is_open()));
}
