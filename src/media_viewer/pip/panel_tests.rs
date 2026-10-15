use super::{
    AspectMode, BORDER_SKIP, DEFAULT_SIZE, DragStep, MINIMAL_SIZE, PipPoint, PipPosition, PipRect,
    PipSize, RectPart, Sides, aspect, clamp_to_edges, constrained, count_position, drag_step,
    ease_out_circ, finish_drag, fit_system_resize, max_allowed_size, place_on_screen, resize_part,
    scaled, size_limits, slide_point, transformed, wayland_normalized,
};

const SCREEN: PipRect = PipRect::new(0, 25, 1440, 875);
const WIDE: PipSize = PipSize::new(1920, 1080);

fn ratio_of(rect: PipRect) -> f64 {
    f64::from(rect.w) / f64::from(rect.h)
}

#[test]
fn scaled_follows_qt() {
    let r = PipSize::new(16, 9);
    assert_eq!(
        scaled(r, PipSize::new(320, 320), AspectMode::Keep),
        PipSize::new(320, 180)
    );
    assert_eq!(
        scaled(r, PipSize::new(120, 120), AspectMode::Expand),
        PipSize::new(213, 120)
    );
    assert_eq!(
        scaled(
            PipSize::new(9, 16),
            PipSize::new(120, 120),
            AspectMode::Expand
        ),
        PipSize::new(120, 213)
    );
    // A degenerate ratio leaves the target alone.
    assert_eq!(
        scaled(PipSize::new(0, 9), PipSize::new(50, 60), AspectMode::Keep),
        PipSize::new(50, 60)
    );
    assert_eq!(aspect(0, 720), PipSize::new(1, 1));
    assert_eq!(aspect(1280, 720), PipSize::new(1280, 720));
}

#[test]
fn a_new_window_is_320_on_its_longer_side_snapped_top_left() {
    let position = PipPosition::default_on(SCREEN);
    let top_left = PipRect::new(0, 0, 1440, 860);
    let wide = place_on_screen(&PipPosition::default_on(top_left), top_left, WIDE);
    assert_eq!(
        wide.geometry,
        PipRect::new(BORDER_SKIP, BORDER_SKIP, DEFAULT_SIZE, 180)
    );
    // tdesktop places the default (0, 0, 320, 320) by its sides: a screen
    // whose usable area starts lower (the macOS menu bar) pulls it up to
    // that line rather than the snap line under it.
    let wide = place_on_screen(&position, SCREEN, WIDE);
    assert_eq!(
        wide.geometry,
        PipRect::new(BORDER_SKIP, 25, DEFAULT_SIZE, 180)
    );
    assert_eq!(wide.minimal, PipSize::new(213, MINIMAL_SIZE));
    let tall = place_on_screen(&position, SCREEN, PipSize::new(1080, 1920));
    assert_eq!(tall.geometry.size(), PipSize::new(180, DEFAULT_SIZE));
    assert_eq!(tall.minimal, PipSize::new(MINIMAL_SIZE, 213));
    // A square clip (and a broken one) is a square.
    let square = place_on_screen(&position, SCREEN, PipSize::new(0, 0));
    assert_eq!(square.geometry.size(), PipSize::new(320, 320));
}

#[test]
fn placement_clamps_to_the_minimal_and_screen_sizes() {
    let tiny = PipPosition {
        geometry: PipRect::new(300, 300, 40, 40),
        ..PipPosition::default()
    };
    let placed = place_on_screen(&tiny, SCREEN, WIDE);
    assert_eq!(placed.geometry.size(), PipSize::new(213, MINIMAL_SIZE));
    assert_eq!(placed.geometry.origin(), PipPoint::new(300, 300));

    let huge = PipPosition {
        geometry: PipRect::new(0, 0, 5000, 5000),
        ..PipPosition::default()
    };
    let placed = place_on_screen(&huge, SCREEN, WIDE);
    let fit = max_allowed_size(SCREEN.size());
    assert_eq!(placed.geometry.w, fit.w);
    assert!(placed.geometry.h <= fit.h);
    // Pushed back inside the screen.
    assert_eq!(placed.geometry.origin(), PipPoint::new(0, 25));
    assert_eq!(placed.maximal.w, fit.w);
}

#[test]
fn saved_attachment_and_snapping_are_restored_on_another_screen_size() {
    let small = PipRect::new(0, 0, 1280, 800);
    for (attached, snapped, origin) in [
        (Sides::RIGHT.with(Sides::BOTTOM), Sides::NONE, (960, 620)),
        (Sides::NONE, Sides::RIGHT.with(Sides::BOTTOM), (940, 600)),
        (Sides::LEFT, Sides::TOP, (0, 20)),
    ] {
        let position = PipPosition {
            attached,
            snapped,
            geometry: PipRect::new(700, 500, 320, 180),
            screen: SCREEN,
        };
        let placed = place_on_screen(&position, small, WIDE);
        assert_eq!(
            placed.geometry.origin(),
            PipPoint::new(origin.0, origin.1),
            "{attached:?} {snapped:?}"
        );
        // And reading the position back gives the same sides.
        assert_eq!(
            count_position(placed.geometry, small, false, false),
            (attached, snapped)
        );
    }
}

#[test]
fn a_move_near_an_edge_slides_onto_the_snap_line() {
    let start = PipRect::new(200, 200, 320, 180);
    // 14 px from the left snap line: inside the 16 px area.
    let near = drag_step(
        start,
        PipPoint::new(-166, 0),
        RectPart::Center,
        WIDE,
        SCREEN,
        Sides::NONE,
    );
    assert_eq!(near, Some(DragStep::Slide(PipPoint::new(BORDER_SKIP, 200))));
    // Far from every edge: the frame follows the pointer.
    let far = drag_step(
        start,
        PipPoint::new(100, 50),
        RectPart::Center,
        WIDE,
        SCREEN,
        Sides::NONE,
    );
    assert_eq!(far, Some(DragStep::Frame(PipRect::new(300, 250, 320, 180))));
    // Bottom-right corner area.
    let corner = clamp_to_edges(SCREEN, PipRect::new(1095, 690, 320, 180));
    assert_eq!(corner, PipPoint::new(1440 - 20 - 320, 900 - 20 - 180));
    // Exactly 16 px outside the line is not in the area.
    assert_eq!(
        clamp_to_edges(SCREEN, PipRect::new(36, 200, 320, 180)),
        PipPoint::new(36, 200)
    );
    assert_eq!(
        drag_step(
            start,
            PipPoint::new(1, 1),
            RectPart::Center,
            WIDE,
            PipRect::default(),
            Sides::NONE
        ),
        None
    );
}

#[test]
fn resizing_from_any_part_keeps_the_aspect_and_the_opposite_anchor() {
    let start = PipRect::new(400, 300, 320, 180);
    let grow = PipPoint::new(80, 45);
    for part in [
        RectPart::TopLeft,
        RectPart::Top,
        RectPart::TopRight,
        RectPart::Right,
        RectPart::BottomRight,
        RectPart::Bottom,
        RectPart::BottomLeft,
        RectPart::Left,
    ] {
        // Drag outward from that part.
        let delta = PipPoint::new(
            match part {
                RectPart::TopLeft | RectPart::BottomLeft | RectPart::Left => -grow.x,
                RectPart::Top | RectPart::Bottom => 0,
                _ => grow.x,
            },
            match part {
                RectPart::TopLeft | RectPart::TopRight | RectPart::Top => -grow.y,
                RectPart::Left | RectPart::Right => 0,
                _ => grow.y,
            },
        );
        let Some(DragStep::Frame(frame)) = drag_step(start, delta, part, WIDE, SCREEN, Sides::NONE)
        else {
            panic!("{part:?}: a resize sets the frame");
        };
        assert!(
            (ratio_of(frame) - 16.0 / 9.0).abs() < 0.02,
            "{part:?}: {frame:?}"
        );
        assert!(frame.w > start.w, "{part:?} grew: {frame:?}");
        match part {
            RectPart::TopLeft => {
                assert_eq!(
                    (frame.right(), frame.bottom()),
                    (start.right(), start.bottom())
                )
            }
            RectPart::TopRight => assert_eq!((frame.x, frame.bottom()), (start.x, start.bottom())),
            RectPart::BottomRight => assert_eq!(frame.origin(), start.origin()),
            RectPart::BottomLeft => assert_eq!((frame.right(), frame.y), (start.right(), start.y)),
            RectPart::Left => assert_eq!(frame.right(), start.right()),
            RectPart::Right => assert_eq!(frame.x, start.x),
            RectPart::Top => assert_eq!(frame.bottom(), start.bottom()),
            RectPart::Bottom => assert_eq!(frame.y, start.y),
            RectPart::Center => {}
        }
    }
}

#[test]
fn resizing_stops_at_the_minimal_and_maximal_sizes() {
    let start = PipRect::new(400, 300, 320, 180);
    let (minimal, maximal) = size_limits(WIDE, SCREEN.size());
    let shrunk = drag_step(
        start,
        PipPoint::new(-1000, -1000),
        RectPart::BottomRight,
        WIDE,
        SCREEN,
        Sides::NONE,
    );
    assert_eq!(
        shrunk,
        Some(DragStep::Frame(PipRect::from_parts(
            start.origin(),
            minimal
        )))
    );
    let grown = drag_step(
        start,
        PipPoint::new(5000, 5000),
        RectPart::BottomRight,
        WIDE,
        SCREEN,
        Sides::NONE,
    );
    let Some(DragStep::Frame(grown)) = grown else {
        panic!("a resize sets the frame");
    };
    assert!(grown.w <= maximal.w && grown.h <= maximal.h, "{grown:?}");
    // An edge drag of a window attached to the top grows downward only.
    let top = constrained(
        transformed(
            start,
            minimal,
            maximal,
            PipPoint::new(80, 0),
            RectPart::Right,
        ),
        minimal,
        maximal,
        WIDE,
        RectPart::Right,
        Sides::TOP,
    );
    assert_eq!(top.y, start.y);
}

#[test]
fn the_resize_bands_skip_attached_and_snapped_sides() {
    let size = PipSize::new(320, 180);
    let at = |x, y, ignore| resize_part(PipPoint::new(x, y), size, ignore);
    assert_eq!(at(2, 2, Sides::NONE), RectPart::TopLeft);
    assert_eq!(at(160, 2, Sides::NONE), RectPart::Top);
    assert_eq!(at(318, 90, Sides::NONE), RectPart::Right);
    assert_eq!(at(318, 178, Sides::NONE), RectPart::BottomRight);
    assert_eq!(at(2, 178, Sides::NONE), RectPart::BottomLeft);
    assert_eq!(at(160, 90, Sides::NONE), RectPart::Center);
    // Snapped to the top-left: those edges move the window.
    let snapped = Sides::LEFT.with(Sides::TOP);
    assert_eq!(at(2, 2, snapped), RectPart::Center);
    assert_eq!(at(2, 178, snapped), RectPart::Bottom);
}

#[test]
fn positions_are_counted_and_moves_or_wayland_count_nothing() {
    let snapped = PipRect::new(20, 45, 320, 180);
    assert_eq!(
        count_position(snapped, SCREEN, false, false),
        (Sides::NONE, Sides::LEFT.with(Sides::TOP))
    );
    let attached = PipRect::new(1440 - 320, 900 - 180, 320, 180);
    assert_eq!(
        count_position(attached, SCREEN, false, false),
        (Sides::RIGHT.with(Sides::BOTTOM), Sides::NONE)
    );
    assert_eq!(
        count_position(snapped, SCREEN, true, false),
        (Sides::NONE, Sides::NONE)
    );
    assert_eq!(
        count_position(snapped, SCREEN, false, true),
        (Sides::NONE, Sides::NONE)
    );
}

#[test]
fn a_finished_drag_pulls_the_window_back_on_screen() {
    let off = PipRect::new(1300, -40, 320, 180);
    assert_eq!(
        finish_drag(off, SCREEN, false),
        Some(PipPoint::new(1120, 25))
    );
    assert_eq!(
        finish_drag(PipRect::new(100, 100, 320, 180), SCREEN, false),
        None
    );
    assert_eq!(finish_drag(off, SCREEN, true), None);
}

#[test]
fn slides_ease_out_and_wayland_resizes_keep_the_aspect() {
    assert_eq!(ease_out_circ(0.0), 0.0);
    assert_eq!(ease_out_circ(1.0), 1.0);
    assert!(ease_out_circ(0.5) > 0.8);
    let from = PipPoint::new(0, 0);
    let to = PipPoint::new(100, -50);
    assert_eq!(slide_point(from, to, 1.0), to);
    assert_eq!(slide_point(from, to, 0.0), from);
    assert_eq!(
        wayland_normalized(PipSize::new(400, 400), WIDE),
        PipSize::new(400, 225)
    );
    assert_eq!(
        wayland_normalized(PipSize::new(400, 400), PipSize::new(9, 16)),
        PipSize::new(225, 400)
    );
    assert_eq!(
        wayland_normalized(PipSize::new(400, 100), WIDE),
        PipSize::new(177, 100)
    );
}

#[test]
fn a_system_resize_keeps_the_aspect_and_the_limits() {
    let screen = SCREEN.size();
    // A window manager stretched it: back to 16:9.
    assert_eq!(
        fit_system_resize(PipSize::new(400, 400), WIDE, screen),
        PipSize::new(400, 225)
    );
    // Under the minimum for the aspect (the window's own minimum is only
    // 120 × 120): up to 213 × 120.
    assert_eq!(
        fit_system_resize(PipSize::new(130, 130), WIDE, screen),
        PipSize::new(213, 120)
    );
    // Over the screen's limit.
    let big = fit_system_resize(PipSize::new(3000, 2000), WIDE, screen);
    assert!(big.w <= 1380 && big.h <= 815, "{big:?}");
    // Already right: unchanged.
    assert_eq!(
        fit_system_resize(PipSize::new(320, 180), WIDE, screen),
        PipSize::new(320, 180)
    );
}
