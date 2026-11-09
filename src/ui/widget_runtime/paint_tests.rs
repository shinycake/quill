use super::*;
use serde_json::json;

const WHITE: u64 = 0xffff_ffff;
const RED: u64 = 0xffff_0000;

fn painted(list: serde_json::Value, damage: Option<&[Rect]>, frame: Option<Pixmap>) -> Pixmap {
    let resources = Resources::default();
    let mut frame = frame.unwrap_or_else(|| Pixmap::new(40, 40).unwrap());
    let list = list.as_array().unwrap().clone();
    paint(&resources, &list, 2.0, damage, g::argb(WHITE), &mut frame);
    frame
}

fn rgb(pixmap: &Pixmap, x: u32, y: u32) -> (u8, u8, u8) {
    let px = pixmap.pixel(x, y).unwrap();
    (px.red(), px.green(), px.blue())
}

#[test]
fn a_box_fills_at_device_scale_over_the_bubble() {
    let frame = painted(json!([{"k": 0, "r": [5, 5, 5, 5], "c": RED}]), None, None);
    assert_eq!(rgb(&frame, 15, 15), (255, 0, 0));
    assert_eq!(
        rgb(&frame, 25, 25),
        (255, 255, 255),
        "10 CSS px is 20 device px"
    );
}

#[test]
fn a_clip_and_a_hidden_group_keep_paint_out() {
    let frame = painted(
        json!([
            {"k": 9, "r": [0, 0, 5, 20]},
            {"k": 0, "r": [0, 0, 20, 20], "c": RED},
            {"k": 10},
            {"k": 12, "m": [1, 0, 0, 1, 0, 0], "o": 1, "h": 1},
            {"k": 0, "r": [10, 10, 10, 10], "c": RED},
            {"k": 13}
        ]),
        None,
        None,
    );
    assert_eq!(rgb(&frame, 4, 4), (255, 0, 0));
    assert_eq!(rgb(&frame, 15, 4), (255, 255, 255));
    assert_eq!(rgb(&frame, 30, 30), (255, 255, 255));
}

#[test]
fn group_opacity_blends_once_and_moves_with_its_matrix() {
    let frame = painted(
        json!([
            {"k": 12, "m": [1, 0, 0, 1, 10, 0], "o": 0.5},
            {"k": 0, "r": [0, 0, 5, 5], "c": RED},
            {"k": 0, "r": [0, 0, 5, 5], "c": RED},
            {"k": 13}
        ]),
        None,
        None,
    );
    let (r, g, b) = rgb(&frame, 25, 5);
    assert_eq!(r, 255);
    assert!(
        (120..=135).contains(&g) && g == b,
        "half red over white: {g}"
    );
    assert_eq!(rgb(&frame, 5, 5), (255, 255, 255));
}

#[test]
fn damage_repaints_only_its_rects() {
    let first = painted(json!([{"k": 0, "r": [0, 0, 20, 20], "c": RED}]), None, None);
    let damage = [Rect::from_xywh(0.0, 0.0, 5.0, 5.0).unwrap()];
    let second = painted(json!([]), Some(&damage), Some(first));
    assert_eq!(
        rgb(&second, 4, 4),
        (255, 255, 255),
        "inside the damage: cleared"
    );
    assert_eq!(
        rgb(&second, 30, 30),
        (255, 0, 0),
        "outside: the last frame stays"
    );
    let untouched = painted(json!([]), Some(&[]), Some(second.clone()));
    assert_eq!(untouched.data(), second.data());
}

#[test]
fn a_box_shadow_shows_around_its_box_but_not_under_it() {
    let frame = painted(
        json!([{"k": 14, "r": [5, 5, 10, 10], "c": 0xff00_0000u64, "a": [0, 0, 4, 2]}]),
        None,
        None,
    );
    assert!(rgb(&frame, 8, 20).0 < 250, "the shadow left of the box");
    assert_eq!(rgb(&frame, 20, 20), (255, 255, 255), "not under the box");
}

#[test]
fn a_linear_gradient_runs_between_its_points() {
    let frame = painted(
        json!([{"k": 6, "r": [0, 0, 20, 20], "a": [0, 0, 20, 0], "s": [0, RED, 1, 0xff00_00ffu64]}]),
        None,
        None,
    );
    let (left, right) = (rgb(&frame, 1, 10), rgb(&frame, 38, 10));
    assert!(left.0 > 240 && left.2 < 15);
    assert!(right.2 > 240 && right.0 < 15);
}

#[test]
fn a_damaged_repaint_of_the_same_list_matches_a_whole_one() {
    let list = json!([
        {"k": 0, "r": [0, 0, 20, 20], "c": 0xfff3_f4f6u64},
        {"k": 0, "r": [2, 2, 10, 10], "c": RED, "l": [0, 0, 20, 20]},
        {"k": 9, "r": [0, 0, 15, 15]},
        {"k": 0, "r": [4, 4, 4, 4], "c": 0xff00_00ffu64},
        {"k": 10}
    ]);
    let whole = painted(list.clone(), None, None);
    let damage = [Rect::from_xywh(3.0, 3.0, 6.0, 6.0).unwrap()];
    let again = painted(list, Some(&damage), Some(whole.clone()));
    assert_eq!(again.data(), whole.data());
}

/// A clipped canvas operation in a blend other than over is drawn on a
/// copy and taken inside the clip only: tiny-skia's masked `Clear` and
/// `Source` change pixels outside the mask too.
#[test]
fn a_masked_take_changes_only_what_the_mask_covers() {
    let mut target = Pixmap::new(40, 40).unwrap();
    target.fill(g::argb(RED));
    let mut drawn = Pixmap::new(40, 40).unwrap();
    drawn.fill(g::argb(0xff00_00ff));
    let mut mask = Mask::new(40, 40).unwrap();
    let inside = PathBuilder::from_rect(Rect::from_xywh(10.0, 10.0, 10.0, 10.0).unwrap());
    mask.fill_path(&inside, FillRule::Winding, false, Transform::identity());
    super::effects::take_masked(&mut target, &drawn, &mask);
    assert_eq!(rgb(&target, 15, 15), (0, 0, 255));
    assert_eq!(rgb(&target, 2, 15), (255, 0, 0));
    assert_eq!(rgb(&target, 30, 2), (255, 0, 0));
}
