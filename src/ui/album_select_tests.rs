//! Albums select and forward as Telegram Desktop's do, driven with real
//! pointer events against the `ready-albums` demo.

use crate::ui::HistoryRow;
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Bounds, Entity, Focusable, Modifiers, MouseButton, Pixels, Point, TestAppContext,
    VisualTestContext, point, px, size,
};
use quill::ids::{ChatId, MessageId};

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
                Some(ScreenshotDemo::named("ready-albums")),
            )
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
    vcx.run_until_parked();
    (app, vcx)
}

/// The own album row (the newest): its messages and painted bounds.
fn own_album(
    app: &Entity<QuillApp>,
    cx: &mut VisualTestContext,
) -> (Vec<MessageId>, Bounds<Pixels>) {
    app.read_with(cx, |app, _| {
        let painted = app
            .history
            .hit_rows
            .try_borrow()
            .map(|rows| rows.clone())
            .unwrap_or_default();
        painted
            .into_iter()
            .filter_map(|(ix, bounds)| match app.history.rows.get(ix)? {
                HistoryRow::Album { messages, .. } if messages.first()?.is_outgoing => {
                    Some((messages.iter().map(|m| m.id).collect(), bounds))
                }
                _ => None,
            })
            .next()
            .expect("the own album is painted")
    })
}

/// Where a tile of `id` was painted.
fn tile_center(app: &Entity<QuillApp>, cx: &mut VisualTestContext, id: MessageId) -> Point<Pixels> {
    app.read_with(cx, |app, _| app.history.tile_hits.center_of(id))
        .expect("the tile is painted")
}

fn chat(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> ChatId {
    app.read_with(cx, |app, _| app.session().and_then(|s| s.open_chat_id()))
        .expect("the demo opens a chat")
}

fn selected(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> Vec<MessageId> {
    app.read_with(cx, |app, _| {
        app.share
            .forward
            .draft()
            .map(|draft| draft.message_ids.clone())
            .unwrap_or_default()
    })
}

fn click(cx: &mut VisualTestContext, at: Point<Pixels>) {
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
}

#[gpui_kit::test]
fn a_tile_toggles_alone_and_the_row_toggles_the_album(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let chat = chat(&app, vcx);
    let (album, bounds) = own_album(&app, vcx);
    assert_eq!(album.len(), 2);
    // The empty side of the row, left of the outgoing bubble.
    let blank = point(bounds.left() + px(8.), bounds.bottom() - px(10.));

    // Selecting from the row's blank side outside selection mode does
    // nothing; the menu's Select over the album picks it whole.
    app.update_in(vcx, |app, _, cx| {
        app.select_group(chat, app.album_of(chat, album[0]), cx)
    });
    assert_eq!(selected(&app, vcx), album);
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }

    // A click on one tile deselects that photo alone.
    let first_tile = tile_center(&app, vcx, album[0]);
    click(vcx, first_tile);
    assert_eq!(selected(&app, vcx), vec![album[1]]);

    // A click beside the tiles selects the album whole again.
    click(vcx, blank);
    assert_eq!(selected(&app, vcx), album);
    // And deselects it whole, ending selection mode.
    click(vcx, blank);
    assert!(selected(&app, vcx).is_empty());
}

#[gpui_kit::test]
fn right_click_beside_the_tiles_selects_and_forwards_the_album(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let (album, bounds) = own_album(&app, vcx);
    let blank = point(bounds.left() + px(8.), bounds.bottom() - px(10.));
    vcx.simulate_mouse_down(blank, MouseButton::Right, Modifiers::default());
    vcx.run_until_parked();
    let scope = app.read_with(vcx, |app, _| app.message_ui.menu.open().map(|o| o.scope));
    assert_eq!(
        scope,
        Some(crate::ui::message_menu_state::MenuScope::WholeAlbum)
    );
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.update(|window, cx| window.click("menu-forward", cx));
    vcx.run_until_parked();
    // The whole album waits for a chat, in one forward.
    assert_eq!(selected(&app, vcx), album);
    assert!(app.read_with(vcx, |app, _| app.share.forward.picker_open()));
}

#[gpui_kit::test]
fn right_click_on_a_tile_is_that_photo_alone(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let (album, _) = own_album(&app, vcx);
    let second = tile_center(&app, vcx, album[1]);
    vcx.simulate_mouse_down(second, MouseButton::Right, Modifiers::default());
    vcx.run_until_parked();
    let open = app.read_with(vcx, |app, _| {
        app.message_ui
            .menu
            .open()
            .map(|o| (o.at.message_id, o.scope))
    });
    assert_eq!(
        open,
        Some((album[1], crate::ui::message_menu_state::MenuScope::OneItem))
    );
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.update(|window, cx| window.click("menu-select", cx));
    vcx.run_until_parked();
    assert_eq!(selected(&app, vcx), vec![album[1]]);
}
