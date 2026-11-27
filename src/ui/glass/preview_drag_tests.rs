use super::{PreviewLayout, PreviewPiece};
use gpui_kit::{Bounds, point, px, size};

fn at(x: f32, y: f32) -> gpui_kit::Point<gpui_kit::Pixels> {
    point(px(x), px(y))
}

#[test]
fn a_dragged_piece_follows_the_pointer_and_stays_on_release() {
    let mut layout = PreviewLayout::default();
    layout.press(PreviewPiece::Menu, at(100., 100.));
    assert_eq!(layout.dragging(), Some(PreviewPiece::Menu));
    assert!(layout.pointer_moved(at(130., 90.), true));
    assert_eq!(layout.offset(PreviewPiece::Menu), at(30., -10.));
    assert!(layout.release());
    assert_eq!(layout.dragging(), None);
    assert_eq!(layout.offset(PreviewPiece::Menu), at(30., -10.));
    assert!(
        !layout.pointer_moved(at(300., 300.), false),
        "nothing follows"
    );
    assert_eq!(layout.offset(PreviewPiece::Menu), at(30., -10.));
    assert_eq!(layout.offset(PreviewPiece::Tabs), at(0., 0.));
}

#[test]
fn a_second_drag_starts_from_where_the_first_left_the_piece() {
    let mut layout = PreviewLayout::default();
    layout.press(PreviewPiece::Slab, at(0., 0.));
    layout.pointer_moved(at(20., 20.), true);
    layout.release();
    layout.press(PreviewPiece::Slab, at(50., 50.));
    layout.pointer_moved(at(55., 45.), true);
    assert_eq!(layout.offset(PreviewPiece::Slab), at(25., 15.));
}

#[test]
fn a_move_with_the_button_up_ends_the_drag() {
    let mut layout = PreviewLayout::default();
    layout.press(PreviewPiece::Jump, at(10., 10.));
    assert!(layout.pointer_moved(at(40., 10.), false));
    assert_eq!(layout.dragging(), None);
    assert_eq!(layout.offset(PreviewPiece::Jump), at(0., 0.));
}

#[test]
fn the_grab_point_stays_inside_the_preview() {
    let mut layout = PreviewLayout::default();
    layout
        .area()
        .record(Bounds::new(at(0., 0.), size(px(400.), px(300.))));
    layout.press(PreviewPiece::Emoji, at(100., 100.));
    layout.pointer_moved(at(900., -50.), true);
    assert_eq!(layout.offset(PreviewPiece::Emoji), at(300., -100.));
}

#[test]
fn the_piece_pressed_last_draws_on_top() {
    let mut layout = PreviewLayout::default();
    assert_eq!(layout.order(), PreviewPiece::ALL);
    layout.press(PreviewPiece::Tabs, at(0., 0.));
    let order = layout.order();
    assert_eq!(order.last(), Some(&PreviewPiece::Tabs));
    assert_eq!(
        order.iter().filter(|p| **p == PreviewPiece::Tabs).count(),
        1,
        "every piece once: {order:?}"
    );
    assert_eq!(order.len(), PreviewPiece::ALL.len());
}
