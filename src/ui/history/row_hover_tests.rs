//! A row's corner buttons exist only under the pointer, and a reader of the
//! accessibility tree reaches them on every row. Over the demo; nothing
//! reaches Telegram.

use crate::ui::app::QuillApp;
use crate::ui::history::demo_chat::{A, frames, open, select};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    Entity, Modifiers, Pixels, Point, ScrollDelta, ScrollEvent, TestAppContext, TouchPhase,
    VisualTestContext, point, px,
};
use quill::ids::MessageId;

const LAST: i64 = 2_039;
const ABOVE: i64 = 2_038;

fn has_buttons(vcx: &mut VisualTestContext, id: i64) -> bool {
    vcx.update(|window, _| window.try_find(format!("message-actions-{id}")).is_some())
}

fn center(app: &Entity<QuillApp>, vcx: &VisualTestContext, id: i64) -> Point<Pixels> {
    app.read_with(vcx, |app, _| app.history.painted_row_center(MessageId(id)))
}

fn hover(vcx: &mut VisualTestContext, at: Point<Pixels>) {
    vcx.simulate_mouse_move(at, None, Modifiers::default());
    vcx.run_until_parked();
}

fn opened(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    let (app, mut vcx) = open(cx, 40);
    select(&app, &mut vcx, A);
    (app, vcx)
}

#[gpui_kit::test]
fn only_the_row_under_the_pointer_has_its_buttons(cx: &mut TestAppContext) {
    let (app, mut vcx) = opened(cx);
    let vcx = &mut vcx;
    assert!(!has_buttons(vcx, LAST), "nothing is under the pointer yet");

    let last = center(&app, vcx, LAST);
    hover(vcx, last);
    assert!(has_buttons(vcx, LAST));
    assert!(!has_buttons(vcx, ABOVE));
    assert_eq!(center(&app, vcx, LAST), last, "the row stays where it was");

    let above = center(&app, vcx, ABOVE);
    hover(vcx, above);
    assert!(has_buttons(vcx, ABOVE));
    assert!(!has_buttons(vcx, LAST));

    hover(vcx, point(px(4.), px(4.)));
    assert!(!has_buttons(vcx, ABOVE), "the pointer left the history");
}

#[gpui_kit::test]
fn a_row_that_scrolls_under_a_still_pointer_takes_the_buttons(cx: &mut TestAppContext) {
    let (app, mut vcx) = opened(cx);
    let vcx = &mut vcx;
    let at = center(&app, vcx, ABOVE);
    hover(vcx, at);
    assert!(has_buttons(vcx, ABOVE));

    let step = at.y - center(&app, vcx, ABOVE - 1).y;
    vcx.simulate_event(ScrollEvent {
        position: at,
        delta: ScrollDelta::Pixels(point(px(0.), step)),
        modifiers: Modifiers::default(),
        touch_phase: TouchPhase::Moved,
        source: gpui_kit::ScrollSource::Pointer,
    });
    vcx.run_until_parked();
    assert!(has_buttons(vcx, ABOVE - 1), "the row now under the pointer");
    assert!(!has_buttons(vcx, ABOVE), "the row it moved away from");
}

#[gpui_kit::test]
fn a_reader_of_the_tree_keeps_the_buttons_of_every_row(cx: &mut TestAppContext) {
    let (_app, mut vcx) = opened(cx);
    let vcx = &mut vcx;
    vcx.update(|window, _| window.set_a11y_forced(true));
    frames(vcx);
    assert!(has_buttons(vcx, LAST));
    assert!(has_buttons(vcx, ABOVE));
    let tree =
        vcx.update(|window, _| format!("{:?}", window.a11y_tree_snapshot().expect("a tree").nodes));
    let actions = quill::lang::tr(quill::lang::keys::Q_MEDIA_MESSAGE_ACTIONS);
    assert!(tree.contains(actions), "the tree names them");
}
