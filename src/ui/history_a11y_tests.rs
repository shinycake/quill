//! Screen-reader focus in the history: the pure focus steps, and the focus
//! model against the "ready-chats" demo (needs the `demo-capture` feature
//! for gpui-kit's test support).
use super::{A11yItem, A11yKey, a11y_items, default_focus, step};
use crate::ui::actions::{HistoryToTop, SelectionFocusNewer, SelectionFocusOlder};
use crate::ui::app::QuillApp;
use crate::ui::history_row::HistoryRow;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use quill::ids::MessageId;

#[test]
fn keys_step_like_telegram_desktop() {
    assert_eq!(step(5, None, A11yKey::Up), Some(4));
    assert_eq!(step(5, None, A11yKey::Down), Some(4));
    assert_eq!(step(5, Some(2), A11yKey::Up), Some(1));
    assert_eq!(step(5, Some(0), A11yKey::Up), None);
    assert_eq!(step(5, Some(4), A11yKey::Down), None);
    assert_eq!(step(5, Some(4), A11yKey::PageUp(3)), Some(1));
    assert_eq!(step(5, Some(1), A11yKey::PageDown(10)), Some(4));
    assert_eq!(step(5, None, A11yKey::PageUp(3)), None);
    assert_eq!(step(5, Some(3), A11yKey::Home), Some(0));
    assert_eq!(step(5, Some(3), A11yKey::End), Some(4));
    assert_eq!(step(0, None, A11yKey::End), None);
}

#[test]
fn default_focus_is_after_the_unread_bar_else_the_last() {
    let plain = [A11yItem::Row(0), A11yItem::Row(1), A11yItem::Row(2)];
    assert_eq!(default_focus(&plain), Some(2));
    let with_bar = [
        A11yItem::Row(0),
        A11yItem::Bar(1),
        A11yItem::Row(1),
        A11yItem::Row(2),
    ];
    assert_eq!(default_focus(&with_bar), Some(2));
    // A bar that is the last child is focused itself.
    assert_eq!(
        default_focus(&[A11yItem::Row(0), A11yItem::Bar(1)]),
        Some(1)
    );
    assert_eq!(default_focus(&[]), None);
}

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    app.update(&mut vcx, |app, _| {
        app.message_ui.a11y.force_screen_reader = true
    });
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
    vcx.run_until_parked();
    (app, vcx)
}

fn focus_history(app: &Entity<QuillApp>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        // Focus events only reach an active window.
        window.activate_window();
        let handle = app.read(cx).message_ui.history_focus.clone();
        window.focus(&handle, cx);
    });
    cx.run_until_parked();
    // Focus-in listeners run when the next frame finds the new focus path.
    cx.update(|window, cx| window.render_frame(cx));
    cx.run_until_parked();
}

/// The message id of every accessibility child (`None` for the bar).
fn children(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> Vec<Option<MessageId>> {
    app.read_with(cx, |app, _| {
        a11y_items(&app.history.rows)
            .into_iter()
            .map(|item| match item {
                A11yItem::Row(ix) => app.history.rows.get(ix).and_then(HistoryRow::first_id),
                A11yItem::Bar(_) => None,
            })
            .collect()
    })
}

fn focused(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> Option<Option<MessageId>> {
    app.read_with(cx, |app, _| {
        let items = a11y_items(&app.history.rows);
        let at = app.history_a11y_current(&items)?;
        Some(match items.get(at)? {
            A11yItem::Row(ix) => app.history.rows.get(*ix).and_then(HistoryRow::first_id),
            A11yItem::Bar(_) => None,
        })
    })
}

#[gpui_kit::test]
fn focus_entering_the_history_picks_a_message_and_arrows_move_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let kids = children(&app, vcx);
    assert!(kids.len() >= 3, "the demo shows {} children", kids.len());
    assert_eq!(focused(&app, vcx), None);

    focus_history(&app, vcx);
    // After the unread bar, else the newest message.
    let expected = app.read_with(vcx, |app, _| {
        default_focus(&a11y_items(&app.history.rows)).expect("children")
    });
    let at = expected;
    assert_eq!(focused(&app, vcx), kids.get(at).copied());

    // Up moves to the older child without starting a selection.
    vcx.dispatch_action(SelectionFocusOlder);
    vcx.run_until_parked();
    assert_eq!(focused(&app, vcx), kids.get(at - 1).copied());
    let chat = app.read_with(vcx, |app, _| app.session().and_then(|s| s.open_chat));
    let chat = chat.expect("the demo opens a chat");
    assert!(!app.read_with(vcx, |app, _| app.selecting_in(chat)));
    vcx.dispatch_action(SelectionFocusNewer);
    vcx.run_until_parked();
    assert_eq!(focused(&app, vcx), kids.get(at).copied());

    // Home goes to the first loaded child.
    vcx.dispatch_action(HistoryToTop);
    vcx.run_until_parked();
    assert_eq!(focused(&app, vcx), kids.first().copied());

    // The focused message has a spoken name ending in its delivery state.
    let name = app.read_with(vcx, |app, _| {
        let focus = app.message_ui.selection_focus.expect("a focused message");
        let row = app
            .history
            .rows
            .iter()
            .find(|row| row.contains(focus))
            .expect("the focused row");
        app.history_row_a11y_name(row)
    });
    let status = name.lines().rev().find(|line| {
        line.starts_with("Received") || line.starts_with("Sent") || line.starts_with("Sending")
    });
    assert!(status.is_some(), "no delivery line in {name:?}");
}

#[gpui_kit::test]
fn without_a_screen_reader_arrows_keep_their_ordinary_meaning(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, _| {
        app.message_ui.a11y.force_screen_reader = false
    });
    focus_history(&app, vcx);
    assert_eq!(focused(&app, vcx), None);
    vcx.dispatch_action(SelectionFocusOlder);
    vcx.run_until_parked();
    assert_eq!(focused(&app, vcx), None);
}

#[gpui_kit::test]
fn the_first_messages_of_a_focused_empty_chat_take_focus(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    // Focus enters a history list that shows no messages yet (a thread,
    // topic or Saved view whose first page is still loading).
    let rows = app.update(vcx, |app, _| std::mem::take(&mut app.history.rows));
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| app.history_a11y_focus_in(window, cx));
    });
    let armed = app.read_with(vcx, |app, _| app.message_ui.a11y.announce_first);
    assert!(armed.is_some());
    assert_eq!(focused(&app, vcx), None);
    // The first slice arrives: the newest message (or the one after the
    // unread bar) is focused and read out.
    app.update(vcx, |app, cx| {
        app.history.rows = rows;
        app.history_a11y_rows_changed(cx);
    });
    let expected = app.read_with(vcx, |app, _| default_focus(&a11y_items(&app.history.rows)));
    let kids = children(&app, vcx);
    assert_eq!(
        focused(&app, vcx),
        expected.and_then(|at| kids.get(at).copied())
    );
    assert!(app.read_with(vcx, |app, _| app.message_ui.a11y.announce_first.is_none()));
}

#[gpui_kit::test]
fn new_messages_in_a_chat_that_shows_some_move_nothing(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    focus_history(&app, vcx);
    vcx.dispatch_action(HistoryToTop);
    vcx.run_until_parked();
    let before = focused(&app, vcx);
    assert!(before.is_some());
    // A rebuild (a new message arriving) leaves the focus where it was.
    app.update(vcx, |app, cx| app.history_a11y_rows_changed(cx));
    assert_eq!(focused(&app, vcx), before);
}
