//! Keyboard control of the chat menu's Mute submenu: Right and Enter open
//! it and focus its first row, Left and Escape close only it and give the
//! focus back to the parent row, Up and Down stay inside it. A submenu the
//! pointer opened takes the keys the same way, from the parent row.
use super::context_menu_focus_tests::{focus_chat_list, new_app, open_chat_menu};
use super::is_submenu_key;
use crate::ui::app::QuillApp;
use crate::ui::menu_states::ChatMenuSubmenu;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use quill::lang::{keys, tr};

/// A menu row as the last paint drew it.
struct Row {
    label: String,
    focused: bool,
    top: f32,
    left: f32,
}

fn rows(vcx: &mut VisualTestContext) -> Vec<Row> {
    let mut rows: Vec<Row> = vcx.update(|window, _| {
        gpui_base::test_support::snapshots(window)
            .into_iter()
            .filter(|row| {
                row.role() == Some(gpui_kit::Role::MenuItem)
                    && row.visible()
                    && row.focused().is_some()
            })
            .map(|row| Row {
                label: row.label().unwrap_or_default().to_string(),
                focused: row.focused() == Some(true),
                top: f32::from(row.bounds().origin.y),
                left: f32::from(row.bounds().origin.x),
            })
            .collect()
    });
    rows.sort_by(|a, b| a.top.total_cmp(&b.top).then(a.left.total_cmp(&b.left)));
    rows
}

/// Rows in the menu's own column and in the submenu panel's. The panel opens
/// to the right of the menu, so its rows have a larger left edge.
const COLUMN_GAP: f32 = 1.;

/// The row holding the focus. A parent row reports itself focused while a
/// row of its submenu is, so the innermost (rightmost) focused row wins; two
/// focused rows in one column would be a bug.
fn focused_label(vcx: &mut VisualTestContext) -> Option<String> {
    let focused: Vec<Row> = rows(vcx).into_iter().filter(|r| r.focused).collect();
    for (at, a) in focused.iter().enumerate() {
        assert!(
            focused
                .iter()
                .skip(at + 1)
                .all(|b| (a.left - b.left).abs() > COLUMN_GAP),
            "two rows of one column are focused: {:?}",
            focused.iter().map(|r| &r.label).collect::<Vec<_>>()
        );
    }
    focused
        .into_iter()
        .max_by(|a, b| a.left.total_cmp(&b.left))
        .map(|r| r.label)
}

/// The labels of the rows drawn in the submenu panel, top to bottom.
fn submenu_labels(vcx: &mut VisualTestContext) -> Vec<String> {
    let all = rows(vcx);
    let menu_left = all.iter().map(|r| r.left).fold(f32::MAX, f32::min);
    all.into_iter()
        .filter(|r| r.left > menu_left + COLUMN_GAP)
        .map(|r| r.label)
        .collect()
}

fn menu_labels(vcx: &mut VisualTestContext) -> Vec<String> {
    let all = rows(vcx);
    let menu_left = all.iter().map(|r| r.left).fold(f32::MAX, f32::min);
    all.into_iter()
        .filter(|r| r.left <= menu_left + COLUMN_GAP)
        .map(|r| r.label)
        .collect()
}

fn mute_label() -> String {
    tr(keys::Q_CHAT_MUTE_NOTIFICATIONS).to_string()
}

fn submenu(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<ChatMenuSubmenu> {
    app.read_with(vcx, |app, _| {
        app.chat_list.menu.as_ref().and_then(|menu| menu.submenu)
    })
}

fn menu_open(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| app.context_menu_open())
}

/// Down until the Mute row has the focus.
fn focus_mute_row(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    open_chat_menu(app, vcx);
    focus_chat_list(app, vcx);
    let count = menu_labels(vcx).len();
    for _ in 0..count {
        vcx.simulate_keystrokes("down");
        if focused_label(vcx) == Some(mute_label()) {
            return;
        }
    }
    panic!("Down never reached the Mute row in {:?}", menu_labels(vcx));
}

fn settle(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..4 {
        vcx.update(|window, cx| window.render_frame(cx));
        vcx.run_until_parked();
    }
}

#[gpui_kit::test]
fn right_opens_the_submenu_and_focuses_its_first_row(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    focus_mute_row(&app, vcx);
    assert_eq!(submenu(&app, vcx), None, "folded before the key");
    vcx.simulate_keystrokes("right");
    settle(vcx);
    assert_eq!(submenu(&app, vcx), Some(ChatMenuSubmenu::Mute));
    let drawn = submenu_labels(vcx);
    assert!(drawn.len() > 1, "the submenu draws several rows");
    assert_eq!(
        focused_label(vcx).as_ref(),
        drawn.first(),
        "the first row of the submenu has the focus"
    );
}

#[gpui_kit::test]
fn enter_on_the_parent_row_opens_the_submenu_too(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    focus_mute_row(&app, vcx);
    vcx.simulate_keystrokes("enter");
    settle(vcx);
    assert_eq!(submenu(&app, vcx), Some(ChatMenuSubmenu::Mute));
    let drawn = submenu_labels(vcx);
    assert!(drawn.len() > 1, "the submenu draws several rows");
    assert_eq!(focused_label(vcx).as_ref(), drawn.first());
}

#[gpui_kit::test]
fn left_returns_to_the_parent_row(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    focus_mute_row(&app, vcx);
    vcx.simulate_keystrokes("right");
    settle(vcx);
    vcx.simulate_keystrokes("left");
    settle(vcx);
    assert_eq!(submenu(&app, vcx), None, "Left closed the submenu");
    assert!(menu_open(&app, vcx), "the menu stays open");
    assert_eq!(focused_label(vcx), Some(mute_label()), "parent row focused");
}

#[gpui_kit::test]
fn escape_in_the_submenu_closes_only_the_submenu(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    focus_mute_row(&app, vcx);
    vcx.simulate_keystrokes("right down");
    settle(vcx);
    assert_eq!(submenu(&app, vcx), Some(ChatMenuSubmenu::Mute));
    vcx.simulate_keystrokes("escape");
    settle(vcx);
    assert_eq!(submenu(&app, vcx), None);
    assert!(menu_open(&app, vcx), "Escape left the menu open");
    assert_eq!(focused_label(vcx), Some(mute_label()));
    vcx.simulate_keystrokes("escape");
    settle(vcx);
    assert!(!menu_open(&app, vcx), "a second Escape closes the menu");
}

#[gpui_kit::test]
fn down_and_up_wrap_inside_the_submenu(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    focus_mute_row(&app, vcx);
    vcx.simulate_keystrokes("right");
    settle(vcx);
    let drawn = submenu_labels(vcx);
    assert!(drawn.len() > 1);
    let mut down = vec![focused_label(vcx).expect("first row")];
    for _ in 1..drawn.len() {
        vcx.simulate_keystrokes("down");
        down.push(focused_label(vcx).expect("Down stays on a row"));
    }
    assert_eq!(down, drawn, "Down walks the rows as drawn on screen");
    vcx.simulate_keystrokes("down");
    assert_eq!(
        focused_label(vcx).as_ref(),
        drawn.first(),
        "Down from the last submenu row wraps to its first"
    );
    vcx.simulate_keystrokes("up");
    assert_eq!(
        focused_label(vcx).as_ref(),
        drawn.last(),
        "Up from the first wraps to the last, never to the menu"
    );
    assert_eq!(submenu(&app, vcx), Some(ChatMenuSubmenu::Mute));
}

/// The pointer opened the submenu (as hovering the Mute row does) while the
/// focus stays on the parent row.
fn hover_open_with_focus_on_mute(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    focus_mute_row(app, vcx);
    app.update_in(vcx, |app, _, cx| {
        app.open_chat_submenu(ChatMenuSubmenu::Mute, cx);
    });
    settle(vcx);
    assert_eq!(focused_label(vcx), Some(mute_label()), "focus stays put");
}

/// tdesktop forwards every key to an open submenu first (popup_menu.cpp:577):
/// Down from the parent row enters it at the first row, then walks it.
#[gpui_kit::test]
fn down_from_the_parent_row_enters_a_hover_opened_submenu(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    hover_open_with_focus_on_mute(&app, vcx);
    let drawn = submenu_labels(vcx);
    assert!(drawn.len() > 1);
    vcx.simulate_keystrokes("down");
    assert_eq!(focused_label(vcx).as_ref(), drawn.first(), "first row");
    vcx.simulate_keystrokes("down");
    assert_eq!(focused_label(vcx).as_ref(), drawn.get(1), "then the next");
    assert_eq!(submenu(&app, vcx), Some(ChatMenuSubmenu::Mute));
}

/// With no submenu row selected Up takes the last one (`Menu::handleKeyPress`).
#[gpui_kit::test]
fn up_from_the_parent_row_enters_a_hover_opened_submenu_at_the_end(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    hover_open_with_focus_on_mute(&app, vcx);
    let drawn = submenu_labels(vcx);
    vcx.simulate_keystrokes("up");
    assert_eq!(focused_label(vcx).as_ref(), drawn.last());
}

#[gpui_kit::test]
fn left_from_the_parent_row_closes_a_hover_opened_submenu(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    hover_open_with_focus_on_mute(&app, vcx);
    vcx.simulate_keystrokes("left");
    settle(vcx);
    assert_eq!(submenu(&app, vcx), None);
    assert!(menu_open(&app, vcx));
    assert_eq!(focused_label(vcx), Some(mute_label()));
}

#[gpui_kit::test]
fn escape_from_the_parent_row_closes_only_a_hover_opened_submenu(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    hover_open_with_focus_on_mute(&app, vcx);
    vcx.simulate_keystrokes("escape");
    settle(vcx);
    assert_eq!(submenu(&app, vcx), None);
    assert!(menu_open(&app, vcx), "the menu stays open");
    vcx.simulate_keystrokes("escape");
    settle(vcx);
    assert!(!menu_open(&app, vcx), "a second Escape closes the menu");
}

/// With the submenu closed, Down on the menu's own rows visits those only.
#[gpui_kit::test]
fn the_menus_own_rows_are_walked_while_the_submenu_is_closed(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_chat_menu(&app, vcx);
    focus_chat_list(&app, vcx);
    let own = menu_labels(vcx);
    let mut walked = Vec::new();
    for _ in 0..own.len() {
        vcx.simulate_keystrokes("down");
        walked.push(focused_label(vcx).expect("a row"));
    }
    assert_eq!(walked, own, "Down visits the menu's rows in drawing order");
}

/// Quill moves into the submenu on Right from its parent row; tdesktop hands
/// Right to the submenu, where it does nothing.
#[gpui_kit::test]
fn right_from_the_parent_row_enters_a_hover_opened_submenu(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    hover_open_with_focus_on_mute(&app, vcx);
    let drawn = submenu_labels(vcx);
    assert!(drawn.len() > 1, "the submenu draws several rows");
    vcx.simulate_keystrokes("right");
    settle(vcx);
    assert_eq!(focused_label(vcx).as_ref(), drawn.first(), "first row");
    assert_eq!(submenu(&app, vcx), Some(ChatMenuSubmenu::Mute));
}

/// Enter on the parent row of a hover-opened submenu moves into it.
#[gpui_kit::test]
fn enter_from_the_parent_row_enters_a_hover_opened_submenu(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    hover_open_with_focus_on_mute(&app, vcx);
    let drawn = submenu_labels(vcx);
    assert!(drawn.len() > 1, "the submenu draws several rows");
    vcx.simulate_keystrokes("enter");
    settle(vcx);
    assert_eq!(focused_label(vcx).as_ref(), drawn.first(), "first row");
    assert_eq!(submenu(&app, vcx), Some(ChatMenuSubmenu::Mute));
}

/// A drawn submenu with no live row takes Down and Enter and leaves the focus
/// on its parent row. The rows are dropped from the frame's claims, as if the
/// submenu had drawn none.
#[gpui_kit::test]
fn a_drawn_submenu_without_live_rows_keeps_the_focus_on_its_parent(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    hover_open_with_focus_on_mute(&app, vcx);
    app.update_in(vcx, |app, _, _| {
        let items = &app.frame.context_menu_items;
        let rows = items
            .keys
            .borrow()
            .iter()
            .filter(|k| is_submenu_key(**k))
            .count();
        assert!(rows > 0, "the submenu drew rows before they were dropped");
        items.keys.borrow_mut().retain(|k| !is_submenu_key(*k));
    });
    for key in ["down", "enter"] {
        vcx.simulate_keystrokes(key);
        assert_eq!(
            focused_label(vcx),
            Some(mute_label()),
            "{key} leaves the focus on the parent row"
        );
        assert_eq!(submenu(&app, vcx), Some(ChatMenuSubmenu::Mute), "{key}");
    }
}

/// A submenu marked open whose parent row is not drawn (here Add to folder,
/// absent without folders) is closed by the next key, which then acts as if
/// none were open.
fn open_stale_folders_submenu(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    focus_mute_row(app, vcx);
    app.update_in(vcx, |app, _, cx| {
        app.open_chat_submenu(ChatMenuSubmenu::Folders, cx);
    });
    settle(vcx);
    assert_eq!(submenu(app, vcx), Some(ChatMenuSubmenu::Folders));
}

#[gpui_kit::test]
fn down_with_a_stale_submenu_clears_it_and_walks_the_menu(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_stale_folders_submenu(&app, vcx);
    vcx.simulate_keystrokes("down");
    settle(vcx);
    assert_eq!(submenu(&app, vcx), None, "the stale state is cleared");
    assert!(menu_open(&app, vcx));
    let own = menu_labels(vcx);
    let at = own
        .iter()
        .position(|l| *l == mute_label())
        .expect("Mute row");
    assert_eq!(
        focused_label(vcx).as_ref(),
        own.get(at + 1),
        "Down moved on the menu's own rows"
    );
}

#[gpui_kit::test]
fn escape_with_a_stale_submenu_closes_the_menu(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_stale_folders_submenu(&app, vcx);
    vcx.simulate_keystrokes("escape");
    settle(vcx);
    assert_eq!(submenu(&app, vcx), None);
    assert!(
        !menu_open(&app, vcx),
        "nothing is open, so Escape closes the menu"
    );
}

/// The first live row of a submenu is the first one that claimed a key (a
/// disabled row never claims one), not the one at index 0; a submenu with no
/// live row has none.
#[gpui_kit::test]
fn the_first_submenu_row_is_the_first_live_one(cx: &mut TestAppContext) {
    use crate::ui::context_menu_keys::{ContextMenuItems, MenuKeyTarget, row_key, submenu_key};
    use gpui_kit::{InteractiveElement, div};
    let (_app, mut vcx) = new_app(cx);
    let items = ContextMenuItems::default();
    vcx.update(|_, cx| {
        items.begin();
        items.mark_submenu_parent(30, ChatMenuSubmenu::Mute);
        assert_eq!(items.first_submenu_key(row_key(30)), None, "no rows drawn");
        // Only rows 2 and 3 are drawn enabled; rows 0 and 1 claim nothing.
        for index in [2usize, 3] {
            let _ = div()
                .id(("row", index))
                .menu_key_target(submenu_key(30, index), &items, cx);
        }
        assert_eq!(
            items.first_submenu_key(row_key(30)),
            Some(submenu_key(30, 2))
        );
        assert_eq!(items.first_submenu_key(row_key(45)), None, "other parent");
    });
}
