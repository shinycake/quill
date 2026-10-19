#![cfg(test)]
#![cfg(feature = "demo-capture")]
//! A forum row through GPUI: the three-line layout with the topics line and
//! the unread-topic badge, and that only the jump chip is clickable on that
//! line.

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Bounds, Entity, Pixels, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    // Another test on this thread may have left a zoom behind.
    crate::ui::interface_zoom::set_initial_zoom(1.);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-forum-row-topics");
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
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

/// The bounds of an element that is in view; fails when it is missing or
/// clipped away.
fn shown(vcx: &mut VisualTestContext, id: (&'static str, u64)) -> Bounds<Pixels> {
    vcx.update(|window, _| {
        let target = window.find(id);
        assert!(target.visible(), "{id:?} is not visible");
        target.bounds()
    })
}

fn click(vcx: &mut VisualTestContext, id: (&'static str, u64)) {
    vcx.update(|window, cx| window.click(id, cx));
    frames(vcx);
}

fn opened(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> (Option<ChatId>, Option<i32>) {
    app.update(vcx, |app, _| {
        app.session()
            .map(|s| (s.open_chat, s.open_topic))
            .unwrap_or_default()
    })
}

#[gpui_kit::test]
fn a_forum_row_is_three_lines_and_a_plain_row_keeps_its_height(cx: &mut TestAppContext) {
    let (_app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let forum = shown(vcx, ("chat-row", 71));
    let plain = shown(vcx, ("chat-row", 11));
    let line = quill::chatlist_style::chat_row_height_px(false, 3);
    assert_eq!(f32::from(forum.size.height), line);
    assert_eq!(
        f32::from(plain.size.height),
        quill::chatlist_style::chat_row_height_px(false, 2)
    );
    // The topics line sits between the name and the message line.
    let titles = shown(vcx, ("forum-titles", 71));
    assert!(titles.origin.y > forum.origin.y + px(20.));
    assert!(titles.bottom() < forum.bottom() - px(14.));
    // Titles that do not fit end in an ellipsis inside the row, never past it.
    assert!(titles.right() <= forum.right());
}

#[gpui_kit::test]
fn the_badge_counts_unread_topics_not_messages(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let (unread_topics, messages) = app.update(&mut vcx, |app, _| {
        let session = app.session().expect("session");
        (
            session.chat_row_forum(ChatId(71)).map(|f| f.unread_topics),
            session
                .chats
                .get(&71)
                .map(|chat| chat.unread_count)
                .unwrap_or_default(),
        )
    });
    // Two topics have unread messages; the chat itself counts none.
    assert_eq!(unread_topics, Some(2));
    assert_eq!(messages, 0);
    let badge = shown(&mut vcx, ("unread-pill", 71));
    let titles = shown(&mut vcx, ("forum-titles", 71));
    // The badge is on the topics line, at its right.
    assert!(badge.origin.x >= titles.right());
    assert!(badge.origin.y >= titles.origin.y - px(4.));
    assert!(badge.bottom() <= titles.bottom() + px(8.));
}

#[gpui_kit::test]
fn a_click_on_the_topics_line_opens_the_chat_not_a_topic(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    click(vcx, ("forum-titles", 72));
    assert_eq!(opened(&app, vcx), (Some(ChatId(72)), None));
}

#[gpui_kit::test]
fn the_jump_chip_spans_both_lines_and_opens_the_topic_of_the_last_message(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let title = shown(vcx, ("forum-jump", 71));
    let message = shown(vcx, ("forum-jump-message", 71));
    // The message part sits under the title part, in the same row.
    assert!(message.origin.y >= title.bottom());
    assert!(message.origin.x <= title.origin.x + px(2.));
    click(vcx, ("forum-jump", 71));
    assert_eq!(opened(&app, vcx), (Some(ChatId(71)), Some(6)));
    // The message part opens it too (on another forum's chip after a reset).
    click(vcx, ("forum-titles", 72));
    assert_eq!(opened(&app, vcx), (Some(ChatId(72)), None));
    click(vcx, ("forum-jump-message", 71));
    assert_eq!(opened(&app, vcx), (Some(ChatId(71)), Some(6)));
}

#[gpui_kit::test]
fn a_press_on_the_chip_still_starts_the_long_press_peek(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let at = shown(vcx, ("forum-jump", 71)).center();
    vcx.simulate_mouse_down(
        at,
        gpui_kit::MouseButton::Left,
        gpui_kit::Modifiers::default(),
    );
    let pressed = app.update(vcx, |app, _| {
        app.chat_list.preview_press.map(|(chat, _)| chat)
    });
    assert_eq!(pressed, Some(ChatId(71)));
}

/// The library resolves a per-process temp folder in every test process, so no
/// test (window or not, in any order) reaches the owner's real preferences.
/// Fails if the real-folder branch of `safe_app_root()` is ever taken.
#[gpui_kit::test]
fn tests_never_resolve_the_real_app_root(cx: &mut TestAppContext) {
    let (_app, _vcx) = open(cx);
    let temp = std::env::temp_dir();
    let root = quill::settings::safe_app_root().expect("an app root");
    assert!(root.starts_with(&temp), "{root:?} is outside {temp:?}");
    assert!(QuillApp::appearance_paths().root.starts_with(&temp));
    assert!(!quill::settings::real_app_root_was_resolved());
}

#[gpui_kit::test]
fn selection_mode_toggles_the_row_from_its_topics_line_and_chip(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let before = opened(&app, vcx);
    app.update_in(vcx, |app, _, cx| app.toggle_chat_selected(ChatId(72), cx));
    frames(vcx);
    click(vcx, ("forum-titles", 71));
    click(vcx, ("forum-jump", 71));
    let (selected, now) = app.update(vcx, |app, _| {
        (
            app.chat_list.selected.clone(),
            app.session().map(|s| (s.open_chat, s.open_topic)),
        )
    });
    // Titles toggled the check, the chip toggled it back: neither opened anything.
    assert!(selected.contains(&72));
    assert!(!selected.contains(&71));
    assert_eq!(now, Some(before));
    click(vcx, ("forum-titles", 71));
    assert!(app.update(vcx, |app, _| app.chat_list.selected.contains(&71)));
}
