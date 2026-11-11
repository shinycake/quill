use super::*;

/// A 1512 × 982 screen with the menu bar (33 pt) at the top.
fn screen() -> ViewRect {
    ViewRect::new(0.0, 0.0, 1512.0, 982.0)
}

fn work_without(top: f32, right: f32, bottom: f32, left: f32) -> ViewRect {
    let s = screen();
    ViewRect::new(left, top, s.w - left - right, s.h - top - bottom)
}

#[test]
fn a_window_over_a_bottom_dock_is_covered_by_the_dock_only() {
    let work = work_without(33.0, 0.0, 70.0, 0.0);
    let window = ViewRect::new(0.0, 33.0, 1512.0, 949.0);
    assert_eq!(
        screen_cover(window, work),
        ScreenCover {
            bottom: 70.0,
            ..ScreenCover::default()
        }
    );
}

#[test]
fn left_and_right_docks_cover_their_side() {
    let window = ViewRect::new(0.0, 33.0, 1512.0, 949.0);
    let left = screen_cover(window, work_without(33.0, 0.0, 0.0, 64.0));
    assert_eq!((left.left, left.right, left.bottom), (64.0, 0.0, 0.0));
    let right = screen_cover(window, work_without(33.0, 64.0, 0.0, 0.0));
    assert_eq!((right.left, right.right, right.bottom), (0.0, 64.0, 0.0));
}

#[test]
fn a_window_inside_the_work_area_is_not_covered() {
    let work = work_without(33.0, 0.0, 70.0, 0.0);
    let window = ViewRect::new(100.0, 80.0, 1100.0, 700.0);
    assert_eq!(screen_cover(window, work), ScreenCover::default());
}

#[test]
fn an_auto_hidden_dock_strip_is_left_to_the_margins() {
    // macOS keeps 4 pt out of the visible frame for a hidden Dock, and a
    // Windows frame counts its 7 px invisible border.
    let window = ViewRect::new(0.0, 33.0, 1512.0, 949.0);
    let mac = screen_cover(window, work_without(33.0, 0.0, 4.0, 0.0));
    assert_eq!(mac, ScreenCover::default());
    let windows = ViewRect::new(-7.0, 0.0, 1526.0, 989.0);
    let cover = screen_cover(windows, work_without(0.0, 0.0, 0.0, 0.0));
    assert_eq!(cover, ScreenCover::default());
}

#[test]
fn a_window_on_the_second_screen_meets_that_screens_work_area() {
    // Windows and X11 report every screen in one desktop space: the second
    // screen sits right of the first, with a taskbar 48 px tall.
    let second_work = ViewRect::new(1512.0, 0.0, 1920.0, 1080.0 - 48.0);
    let window = ViewRect::new(1600.0, 200.0, 1200.0, 880.0);
    let cover = screen_cover(window, second_work);
    assert_eq!(cover.bottom, 48.0);
    // The first screen's area does not meet that window: no cover, rather
    // than one made up from the wrong screen.
    let first_work = work_without(0.0, 0.0, 48.0, 0.0);
    assert_eq!(screen_cover(window, first_work), ScreenCover::default());
}

#[test]
fn a_window_hanging_off_the_screen_keeps_room_for_its_chrome() {
    let work = work_without(33.0, 0.0, 70.0, 0.0);
    let window = ViewRect::new(200.0, 600.0, 900.0, 800.0);
    let cover = screen_cover(window, work);
    // 488 pt hang below the work area; at most 40 % of the height counts.
    assert_eq!(cover.bottom, 320.0);
    let safe = cover.safe_area((900.0, 800.0));
    assert_eq!(safe, ViewRect::new(0.0, 0.0, 900.0, 480.0));
}

#[test]
fn nonsense_frames_give_no_cover() {
    let work = work_without(33.0, 0.0, 70.0, 0.0);
    let nan = ViewRect::new(f32::NAN, 0.0, 100.0, 100.0);
    assert_eq!(screen_cover(nan, work), ScreenCover::default());
    let empty = ViewRect::new(0.0, 0.0, 0.0, 0.0);
    assert_eq!(screen_cover(work, empty), ScreenCover::default());
}

fn home() -> ViewRect {
    // A 480 × 72 controller centered 32 pt above a 1000 × 800 area's bottom.
    ViewRect::new(260.0, 696.0, 480.0, 72.0)
}

fn area() -> ViewRect {
    ViewRect::new(0.0, 0.0, 1000.0, 800.0)
}

#[test]
fn dragging_moves_the_controller_by_the_pointer() {
    let mut place = ControllerPlace::default();
    place.press((500.0, 730.0), home(), area());
    assert!(place.drag_to((450.0, 430.0), home(), area()));
    assert_eq!(
        place.spot(),
        ControllerSpot::Moved {
            dx: -50.0,
            dy: -300.0
        }
    );
    assert!(place.release());
    assert!(!place.is_held());
    assert_eq!(place.offset(home(), area()), (-50.0, -300.0));
}

#[test]
fn a_click_on_the_panel_does_not_move_it() {
    let mut place = ControllerPlace::default();
    place.press((500.0, 730.0), home(), area());
    assert!(!place.drag_to((501.0, 731.0), home(), area()));
    place.release();
    assert_eq!(place.spot(), ControllerSpot::Home);
}

#[test]
fn near_home_the_controller_snaps_back_onto_it() {
    let mut place = ControllerPlace::default();
    place.press((500.0, 730.0), home(), area());
    place.drag_to((500.0, 430.0), home(), area());
    // Back to within 24 pt of home: on it exactly.
    assert!(place.drag_to((510.0, 715.0), home(), area()));
    assert_eq!(place.spot(), ControllerSpot::Home);
    place.release();
    assert_eq!(place.offset(home(), area()), (0.0, 0.0));
}

#[test]
fn reset_puts_a_moved_controller_home() {
    let mut place = ControllerPlace::default();
    place.press((500.0, 730.0), home(), area());
    place.drag_to((300.0, 300.0), home(), area());
    place.reset();
    assert_eq!(place, ControllerPlace::default());
}

#[test]
fn the_controller_stays_inside_the_safe_area() {
    let mut place = ControllerPlace::default();
    place.press((500.0, 730.0), home(), area());
    place.drag_to((-900.0, -900.0), home(), area());
    // Top-left corner, 8 pt in.
    assert_eq!(place.offset(home(), area()), (-252.0, -688.0));
    place.drag_to((5000.0, 5000.0), home(), area());
    assert_eq!(place.offset(home(), area()), (252.0, 24.0));
}

#[test]
fn a_moved_controller_follows_a_shrinking_window() {
    let mut place = ControllerPlace::default();
    place.press((500.0, 730.0), home(), area());
    place.drag_to((500.0, 30.0), home(), area());
    assert_eq!(place.offset(home(), area()), (0.0, -688.0));
    // The window gets shorter: home moves up with the bottom edge and the
    // controller stays on screen.
    let small_area = ViewRect::new(0.0, 0.0, 1000.0, 400.0);
    let small_home = ViewRect::new(260.0, 296.0, 480.0, 72.0);
    assert_eq!(place.offset(small_home, small_area), (0.0, -288.0));
}

#[test]
fn a_drag_without_a_press_does_nothing() {
    let mut place = ControllerPlace::default();
    assert!(!place.drag_to((0.0, 0.0), home(), area()));
    assert!(!place.release());
}
