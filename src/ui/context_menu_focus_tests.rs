//! Keyboard reach of the archive row and story tile context menus: Tab and
//! Down land on an item inside the menu, Down and Up wrap, Escape closes the
//! menu and gives the focus back (the playlist track menu is covered in
//! `playlist_manage_tests`; needs the `demo-capture` feature for gpui-kit's
//! test support).
use crate::ui::app::QuillApp;
use crate::ui::context_menu_keys::{ContextMenuItems, context_menu_item};
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::assets::IconName;
use gpui_kit::component::Root;
use gpui_kit::{
    AppContext, Entity, FocusHandle, Focusable, TestAppContext, VisualTestContext, point, px, size,
};

pub(super) fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
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
    let app: Entity<QuillApp> = slot
        .borrow()
        .clone()
        .expect("the window builder fills the slot");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

/// After Tab the focus sits on an item inside the menu, not on its frame.
pub(super) fn tab_lands_on_an_item(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    vcx.simulate_keystrokes("tab");
    let (inside, on_frame) = vcx.update(|window, cx| {
        let menu = app.read(cx).frame.context_menu_focus.clone();
        (menu.contains_focused(window, cx), menu.is_focused(window))
    });
    assert!(inside, "focus stays in the menu");
    assert!(!on_frame, "Tab moved off the menu frame onto an item");
}

#[gpui_kit::test]
fn tab_reaches_the_archive_menu_items(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.chat_list.archive_menu = Some(point(px(40.), px(80.)));
        cx.notify();
    });
    tab_lands_on_an_item(&app, vcx);
}

#[gpui_kit::test]
fn tab_reaches_the_story_menu_items(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.open_story_menu(1, point(px(40.), px(80.)), cx)
    });
    tab_lands_on_an_item(&app, vcx);
}

fn focused(vcx: &mut VisualTestContext) -> Option<FocusHandle> {
    vcx.update(|window, cx| window.focused(cx))
}

/// Press `key` until the focus comes back to where it started; the number of
/// presses is the number of items in the menu.
fn cycle_len(vcx: &mut VisualTestContext, key: &str) -> usize {
    vcx.simulate_keystrokes(key);
    let first = focused(vcx).expect("a focused item");
    let mut seen = 1;
    while seen < 50 {
        vcx.simulate_keystrokes(key);
        if focused(vcx).as_ref() == Some(&first) {
            return seen;
        }
        seen += 1;
    }
    panic!("focus never wrapped");
}

/// Items the archive menu draws: collapse, move to main menu, mark all read
/// (only with unread chats), how it works, settings.
fn archive_items(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> usize {
    vcx.read(|cx| {
        let unread = app
            .read(cx)
            .session()
            .and_then(|s| s.archive_row_summary())
            .is_some_and(|summary| summary.unread_chats > 0);
        4 + usize::from(unread)
    })
}

fn open_archive_menu(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update_in(vcx, |app, _, cx| {
        app.chat_list.archive_menu = Some(point(px(40.), px(80.)));
        cx.notify();
    });
    vcx.run_until_parked();
}

fn in_menu(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    vcx.update(|window, cx| {
        app.read(cx)
            .frame
            .context_menu_focus
            .contains_focused(window, cx)
    })
}

pub(super) fn focus_chat_list(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update_in(vcx, |app, window, cx| {
        window.focus(&app.focus_sidebar.clone(), cx)
    });
}

#[gpui_kit::test]
fn down_moves_between_archive_menu_items_and_wraps(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_archive_menu(&app, vcx);
    vcx.simulate_keystrokes("down");
    let first = focused(vcx).expect("Down focuses an item");
    let on_frame =
        vcx.update(|window, cx| app.read(cx).frame.context_menu_focus.is_focused(window));
    assert!(!on_frame, "Down left the menu frame for an item");
    vcx.simulate_keystrokes("down");
    let second = focused(vcx).expect("second item");
    assert_ne!(first, second, "Down moved to the next item");
    vcx.simulate_keystrokes("up");
    assert_eq!(focused(vcx), Some(first.clone()), "Up came back");
    // Up from the first item wraps to the last; Down from there comes back.
    vcx.simulate_keystrokes("up");
    let last = focused(vcx).expect("wrapped item");
    assert_ne!(last, first, "Up from the first item wrapped");
    vcx.simulate_keystrokes("down");
    assert_eq!(focused(vcx), Some(first), "Down from the last item wrapped");
    let expected = archive_items(&app, vcx);
    assert_eq!(cycle_len(vcx, "down"), expected);
    assert!(in_menu(&app, vcx), "focus never leaves the menu");
}

#[gpui_kit::test]
fn down_cycles_every_story_menu_item(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.open_story_menu(1, point(px(40.), px(80.)), cx)
    });
    vcx.run_until_parked();
    let expected = app.read_with(vcx, |app, _| app.story_menu_entries(1).len());
    let drawn = app.read_with(vcx, |app, _| app.frame.context_menu_items.enabled_count());
    assert_eq!(
        drawn, expected,
        "every story menu entry is a focusable item"
    );
    assert_eq!(cycle_len(vcx, "down"), expected);
    assert_eq!(cycle_len(vcx, "up"), expected);
}

/// The menu takes the arrows even when the focus is outside it: they move
/// into the menu and never reach the chat list under it.
#[gpui_kit::test]
fn arrows_enter_the_menu_instead_of_reaching_the_chat_list(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_archive_menu(&app, vcx);
    focus_chat_list(&app, vcx);
    assert!(!in_menu(&app, vcx), "focus starts outside the menu");
    vcx.simulate_keystrokes("down");
    assert!(in_menu(&app, vcx), "Down moved the focus into the menu");
    app.read_with(vcx, |app, _| {
        assert!(
            !app.chat_list.keys.has_list_selection(),
            "the chat list selection did not move"
        );
    });
    focus_chat_list(&app, vcx);
    let items = archive_items(&app, vcx);
    assert_eq!(
        cycle_len(vcx, "up"),
        items,
        "Up from outside enters the menu"
    );
}

pub(super) fn open_chat_menu(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update_in(vcx, |app, _, cx| {
        app.chat_list.menu = Some(crate::ui::menu_states::ChatMenuState::at(
            quill::ids::ChatId(11),
            point(px(40.), px(80.)),
        ));
        cx.notify();
    });
    vcx.run_until_parked();
}

fn open_message_menu(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update_in(vcx, |app, _, cx| {
        app.message_ui.menu = Some(crate::ui::menu_states::MessageMenuState {
            chat_id: quill::ids::ChatId(11),
            message_id: quill::ids::MessageId(101),
            position: point(px(500.), px(200.)),
        });
        cx.notify();
    });
    vcx.run_until_parked();
}

/// A menu row as the last paint drew it.
#[derive(Debug, Clone, PartialEq)]
struct ScreenRow {
    label: String,
    focused: bool,
}

/// Every focusable menu item the last frame painted, top to bottom (then left
/// to right), read from the accessibility tree rather than from the focus
/// list the keyboard walks. Disabled rows carry the same role but no focus
/// binding (`focused()` is `None`; a plain div has no a11y disabled flag), so
/// they are left out, as the keyboard skips them.
fn rows_on_screen(vcx: &mut VisualTestContext) -> Vec<ScreenRow> {
    let mut rows: Vec<(f32, f32, ScreenRow)> = vcx.update(|window, _| {
        gpui_base::test_support::snapshots(window)
            .into_iter()
            .filter(|row| {
                row.role() == Some(gpui_kit::Role::MenuItem)
                    && row.visible()
                    && row.focused().is_some()
            })
            .map(|row| {
                let bounds = row.bounds();
                let screen = ScreenRow {
                    label: row.label().unwrap_or_default().to_string(),
                    focused: row.focused() == Some(true),
                };
                (
                    f32::from(bounds.origin.y),
                    f32::from(bounds.origin.x),
                    screen,
                )
            })
            .collect()
    });
    rows.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    rows.into_iter().map(|(_, _, row)| row).collect()
}

fn labels_on_screen(vcx: &mut VisualTestContext) -> Vec<String> {
    rows_on_screen(vcx)
        .into_iter()
        .map(|row| row.label)
        .collect()
}

fn focused_label(vcx: &mut VisualTestContext) -> Option<String> {
    rows_on_screen(vcx)
        .into_iter()
        .find(|row| row.focused)
        .map(|row| row.label)
}

/// Rows the menu registered as focus targets.
fn drawn(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> usize {
    app.read_with(vcx, |app, _| app.frame.context_menu_items.enabled_count())
}

/// Down walks the rows exactly as the menu draws them, top to bottom, then
/// wraps to the first; Up walks them back. The expected sequence and the row
/// count come from what the frame painted, so a wrong sort key for any row
/// shows up as a row out of place.
fn assert_walks_in_drawing_order(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    let on_screen = labels_on_screen(vcx);
    assert!(on_screen.len() > 1, "the menu draws several rows");
    assert_eq!(
        drawn(app, vcx),
        on_screen.len(),
        "every drawn row is a focus target"
    );
    let mut down = Vec::new();
    for _ in 0..on_screen.len() {
        vcx.simulate_keystrokes("down");
        down.push(focused_label(vcx).expect("Down focuses a row"));
    }
    assert_eq!(down, on_screen, "Down visits the rows in drawing order");
    vcx.simulate_keystrokes("down");
    assert_eq!(
        focused_label(vcx),
        on_screen.first().cloned(),
        "Down from the last row wraps to the first"
    );
    let mut up = Vec::new();
    for _ in 0..on_screen.len() {
        vcx.simulate_keystrokes("up");
        up.push(focused_label(vcx).expect("Up focuses a row"));
    }
    let mut backwards = on_screen.clone();
    backwards.reverse();
    assert_eq!(up, backwards, "Up from the first row visits them backwards");
    assert!(in_menu(app, vcx), "focus never leaves the menu");
}

/// The chat row menu: Tab and Down reach its rows, in drawing order.
#[gpui_kit::test]
fn the_chat_menu_is_reachable_by_keyboard(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_chat_menu(&app, vcx);
    tab_lands_on_an_item(&app, vcx);
    focus_chat_list(&app, vcx);
    assert_walks_in_drawing_order(&app, vcx);
    app.read_with(vcx, |app, _| {
        assert!(
            !app.chat_list.keys.has_list_selection(),
            "chat list untouched"
        );
    });
}

/// The message menu: Tab and Down reach its rows, in drawing order (its
/// builders push rows in one order and the menu sorts them into another).
#[gpui_kit::test]
fn the_message_menu_is_reachable_by_keyboard(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_message_menu(&app, vcx);
    tab_lands_on_an_item(&app, vcx);
    focus_chat_list(&app, vcx);
    assert_walks_in_drawing_order(&app, vcx);
}

/// Tab on the last row stays in the menu: the overlay's focus trap wraps it
/// to the first row, and Shift-Tab from the first goes to the last, so the
/// chat list and the composer never get it.
fn assert_tab_stays_in_the_menu(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    let rows = labels_on_screen(vcx);
    vcx.simulate_keystrokes("up");
    assert_eq!(
        focused_label(vcx).as_ref(),
        rows.last(),
        "Up from outside lands on the last row"
    );
    vcx.simulate_keystrokes("tab");
    assert!(in_menu(app, vcx), "Tab off the last row stays in the menu");
    assert_eq!(
        focused_label(vcx).as_ref(),
        rows.first(),
        "Tab wraps to the first row"
    );
    vcx.simulate_keystrokes("shift-tab");
    assert!(in_menu(app, vcx), "Shift-Tab off the first row stays in");
    assert_eq!(
        focused_label(vcx).as_ref(),
        rows.last(),
        "Shift-Tab wraps to the last row"
    );
    for _ in 0..=rows.len() {
        vcx.simulate_keystrokes("tab");
        assert!(in_menu(app, vcx), "repeated Tab never leaves the menu");
    }
    app.read_with(vcx, |app, _| {
        assert!(
            !app.chat_list.keys.has_list_selection(),
            "the chat list took no key"
        );
        assert!(app.context_menu_open(), "Tab did not close the menu");
    });
}

#[gpui_kit::test]
fn tab_cannot_leave_the_chat_or_message_menu(cx: &mut TestAppContext) {
    for message in [false, true] {
        let (app, mut vcx) = new_app(cx);
        let vcx = &mut vcx;
        if message {
            open_message_menu(&app, vcx);
        } else {
            open_chat_menu(&app, vcx);
        }
        focus_chat_list(&app, vcx);
        assert_tab_stays_in_the_menu(&app, vcx);
    }
}

/// Move the focus to the first row whose label starts with `prefix` and
/// press `key` on it.
fn activate_row(vcx: &mut VisualTestContext, prefix: &str, key: &str) {
    let rows = labels_on_screen(vcx);
    assert!(
        rows.iter().any(|label| label.starts_with(prefix)),
        "no row starting with {prefix:?} in {rows:?}"
    );
    for _ in 0..rows.len() {
        vcx.simulate_keystrokes("down");
        if focused_label(vcx).is_some_and(|label| label.starts_with(prefix)) {
            vcx.simulate_keystrokes(key);
            // A keyboard click completes on key up, which the keystroke
            // simulation does not send.
            vcx.simulate_event(gpui_kit::KeyUpEvent {
                keystroke: gpui_kit::Keystroke::parse_with_os(key, crate::ui::host_os::HOST_OS)
                    .expect("a known key"),
            });
            vcx.run_until_parked();
            return;
        }
    }
    panic!("Down never reached a row starting with {prefix:?}");
}

/// Enter and Space run the focused row like a click: the chat menu's
/// "Mark as read" or "Mark as unread" entry closes the menu, and so does the
/// message menu's Reply.
#[gpui_kit::test]
fn enter_and_space_run_the_focused_menu_row(cx: &mut TestAppContext) {
    for (message, prefix) in [(false, "Mark as"), (true, "Reply")] {
        for key in ["enter", "space"] {
            let (app, mut vcx) = new_app(cx);
            let vcx = &mut vcx;
            if message {
                open_message_menu(&app, vcx);
            } else {
                open_chat_menu(&app, vcx);
            }
            activate_row(vcx, prefix, key);
            app.read_with(vcx, |app, _| {
                assert!(
                    !app.context_menu_open(),
                    "{key} ran {prefix:?} and closed the menu"
                );
            });
        }
    }
}

/// A menu with nothing to focus (an unknown chat, or a message that is gone)
/// keeps the focus on its frame, swallows the arrows, and Enter does nothing.
#[gpui_kit::test]
fn a_menu_without_items_keeps_the_focus_on_its_frame(cx: &mut TestAppContext) {
    for message in [false, true] {
        let (app, mut vcx) = new_app(cx);
        let vcx = &mut vcx;
        app.update_in(vcx, |app, _, cx| {
            if message {
                app.message_ui.menu = Some(crate::ui::menu_states::MessageMenuState {
                    chat_id: quill::ids::ChatId(1),
                    message_id: quill::ids::MessageId(987_654),
                    position: point(px(500.), px(200.)),
                });
            } else {
                app.chat_list.menu = Some(crate::ui::menu_states::ChatMenuState::at(
                    quill::ids::ChatId(987_654),
                    point(px(40.), px(80.)),
                ));
            }
            cx.notify();
        });
        vcx.run_until_parked();
        assert_eq!(drawn(&app, vcx), 0, "the menu has no focus target");
        focus_chat_list(&app, vcx);
        vcx.simulate_keystrokes("down down up enter");
        assert!(in_menu(&app, vcx), "focus is on the menu frame");
        app.read_with(vcx, |app, _| {
            assert!(
                !app.chat_list.keys.has_list_selection(),
                "chat list untouched"
            );
        });
    }
}

#[gpui_kit::test]
fn an_all_disabled_menu_has_no_target(cx: &mut TestAppContext) {
    let (_app, mut vcx) = new_app(cx);
    let items = ContextMenuItems::default();
    vcx.update(|window, cx| {
        items.begin();
        for i in 0..3usize {
            let _ = context_menu_item(
                ("disabled-item", i),
                IconName::Info,
                "Item",
                gpui_kit::black(),
                false,
                &items,
                cx,
            );
        }
        assert_eq!(items.enabled_count(), 0, "no row has a handle");
        assert!(items.step(true, None, window).is_none());
        assert!(items.step(false, None, window).is_none());
    });
}

/// A submenu's keys stay between its parent row and the next row, even
/// past the 255th item.
#[test]
fn submenu_keys_stay_between_their_parent_and_the_next_row() {
    use crate::ui::context_menu_keys::{MAX_SUBMENU_ROWS, row_key, submenu_key};
    assert!(row_key(30) < submenu_key(30, 0));
    assert!(submenu_key(30, 0) < submenu_key(30, 1));
    for index in [MAX_SUBMENU_ROWS - 1, MAX_SUBMENU_ROWS, 1_000, usize::MAX] {
        let key = submenu_key(30, index);
        assert!(key > row_key(30), "after the parent at {index}");
        assert!(key < row_key(31), "before the next row at {index}");
    }
    assert_eq!(
        submenu_key(30, MAX_SUBMENU_ROWS),
        submenu_key(30, MAX_SUBMENU_ROWS - 1),
        "rows past the end share the last key"
    );
}

#[gpui_kit::test]
fn escape_closes_the_chat_and_message_menus_and_restores_focus(cx: &mut TestAppContext) {
    for message in [false, true] {
        let (app, mut vcx) = new_app(cx);
        let vcx = &mut vcx;
        let before = focused(vcx).expect("the app has focus");
        if message {
            open_message_menu(&app, vcx);
        } else {
            open_chat_menu(&app, vcx);
        }
        vcx.simulate_keystrokes("down down");
        assert!(in_menu(&app, vcx));
        vcx.simulate_keystrokes("escape");
        vcx.run_until_parked();
        app.read_with(vcx, |app, _| {
            assert!(!app.context_menu_open(), "Escape closed the menu");
        });
        assert_eq!(focused(vcx), Some(before), "focus went back");
    }
}

#[gpui_kit::test]
fn disabled_items_are_never_focused(cx: &mut TestAppContext) {
    let (_app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let items = ContextMenuItems::default();
    vcx.update(|_, cx| {
        items.begin();
        for (i, enabled) in [false, true, false].into_iter().enumerate() {
            let _ = context_menu_item(
                ("test-item", i),
                IconName::Info,
                "Item",
                gpui_kit::black(),
                enabled,
                &items,
                cx,
            );
        }
    });
    assert_eq!(
        items.enabled_count(),
        1,
        "only the enabled item has a handle"
    );
    vcx.update(|window, _| {
        let only = items.step(true, None, window).expect("the enabled item");
        assert_eq!(
            items.step(false, None, window),
            Some(only),
            "Up reaches the same one"
        );
    });
    let none = ContextMenuItems::default();
    none.begin();
    vcx.update(|window, _| {
        assert!(
            none.step(true, None, window).is_none(),
            "no enabled item, no target"
        );
        assert!(none.step(false, None, window).is_none());
    });
}

/// Without a menu the arrows are not intercepted: the chat list takes them.
#[gpui_kit::test]
fn arrows_still_reach_the_chat_list_without_a_menu(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    focus_chat_list(&app, vcx);
    vcx.run_until_parked();
    vcx.simulate_keystrokes("down");
    app.read_with(vcx, |app, _| {
        assert!(
            app.chat_list.keys.has_list_selection(),
            "Down selected a chat row"
        );
    });
}

#[gpui_kit::test]
fn escape_closes_the_menu_and_restores_focus(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let before = focused(vcx).expect("the app has focus");
    app.update_in(vcx, |app, _, cx| {
        app.chat_list.archive_menu = Some(point(px(40.), px(80.)));
        cx.notify();
    });
    vcx.run_until_parked();
    vcx.simulate_keystrokes("down");
    vcx.simulate_keystrokes("escape");
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| {
        assert!(
            app.chat_list.archive_menu.is_none(),
            "Escape closed the menu"
        );
    });
    assert_eq!(
        focused(vcx),
        Some(before),
        "focus went back to where it was"
    );
}
