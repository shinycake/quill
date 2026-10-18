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

fn focus_chat_list(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
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
    assert!(expected > 0, "the story menu has items");
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

/// A menu with no enabled item (the chat menu has none) keeps the focus on
/// its frame and still swallows the arrows.
#[gpui_kit::test]
fn a_menu_without_items_keeps_the_focus_on_its_frame(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.chat_list.menu = Some(crate::ui::menu_states::ChatMenuState {
            chat_id: quill::ids::ChatId(1),
            position: point(px(40.), px(80.)),
            submenu: None,
        });
        cx.notify();
    });
    vcx.run_until_parked();
    focus_chat_list(&app, vcx);
    vcx.simulate_keystrokes("down down up");
    assert!(in_menu(&app, vcx), "focus is on the menu frame");
    app.read_with(vcx, |app, _| {
        assert!(
            !app.chat_list.keys.has_list_selection(),
            "chat list untouched"
        );
    });
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
        let only = items.step(true, window).expect("the enabled item");
        assert_eq!(
            items.step(false, window),
            Some(only),
            "Up reaches the same one"
        );
    });
    let none = ContextMenuItems::default();
    none.begin();
    vcx.update(|window, _| {
        assert!(
            none.step(true, window).is_none(),
            "no enabled item, no target"
        );
        assert!(none.step(false, window).is_none());
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
