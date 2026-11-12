#![allow(clippy::string_slice, reason = "test code slices known-good literals")]
use super::*;
use gpui_kit::{FileDropEvent, MouseButton, MouseDownEvent, ScrollDelta, ScrollEvent, point, px};

#[test]
fn the_session_ending_is_a_quit() {
    let mut quit = noting_quit(Box::new(|| true));
    assert!(quit());
    assert!(crate::ui::startup_failure::quit_asked());
}

#[test]
fn a_refused_quit_is_not_a_quit() {
    crate::ui::startup_failure::note_quit_asked();
    let mut request = noting_refusal(Box::new(|| gpui_kit::QuitAnswer::Refuse));
    assert!(matches!(request(), gpui_kit::QuitAnswer::Refuse));
    assert!(!crate::ui::startup_failure::quit_asked());
    crate::ui::startup_failure::note_quit_asked();
    let mut allowed = noting_refusal(Box::new(|| gpui_kit::QuitAnswer::Allow));
    assert!(matches!(allowed(), gpui_kit::QuitAnswer::Allow));
    assert!(crate::ui::startup_failure::quit_asked());
    crate::ui::startup_failure::note_quit_refused();
}

#[test]
fn a_zoom_change_inside_the_resize_callback_runs_it_again() {
    let state = WindowZoom::register(1., Size::new(px(1000.), px(600.)), 1., None);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = seen.clone();
    let inner = Rc::downgrade(&state);
    *state.resize.borrow_mut() = Some(Box::new(move |size, scale| {
        log.borrow_mut().push((size, scale));
        // The first run changes the zoom while it is still running.
        if log.borrow().len() == 1
            && let Some(state) = inner.upgrade()
        {
            state.zoom.set(2.);
            state.fire_resize();
        }
    }));
    state.fire_resize();
    let seen = seen.borrow();
    assert_eq!(seen.len(), 2, "the nested change was not skipped");
    assert_eq!(seen[1], (Size::new(px(500.), px(300.)), 2.));
    assert_eq!(state.refire.get(), Refire::Idle);
    assert!(state.resize.borrow().is_some(), "the callback is back");
}

#[test]
fn a_resize_callback_that_panics_leaves_the_next_zoom_change_able_to_run_it() {
    let state = WindowZoom::register(1., Size::new(px(1000.), px(600.)), 1., None);
    let runs = Rc::new(Cell::new(0));
    let count = runs.clone();
    *state.resize.borrow_mut() = Some(Box::new(move |_, _| {
        count.set(count.get() + 1);
        if count.get() == 1 {
            panic!("a handler that fails once");
        }
    }));
    let first = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| state.fire_resize()));
    assert!(first.is_err());
    assert_eq!(state.refire.get(), Refire::Idle);
    assert!(state.resize.borrow().is_some(), "the callback is back");
    state.fire_resize();
    assert_eq!(runs.get(), 2, "the next change ran it again");
}

#[test]
fn zoom_for_percent_clamps_like_the_setting() {
    assert_eq!(zoom_for_percent(100), 1.);
    assert_eq!(zoom_for_percent(150), 1.5);
    assert_eq!(zoom_for_percent(0), 1.);
    assert_eq!(zoom_for_percent(999), 3.);
}

#[test]
fn geometry_round_trips_through_the_zoom() {
    let b = Bounds::new(point(px(30.), px(60.)), Size::new(px(300.), px(90.)));
    assert_eq!(bounds_in(bounds_out(b, 1.5), 1.5), b);
    assert_eq!(
        size_in(Size::new(px(1500.), px(900.)), 1.5),
        Size::new(px(1000.), px(600.))
    );
    assert_eq!(
        point_out(point(px(10.), px(20.)), 2.),
        point(px(20.), px(40.))
    );
    // kit's title bar: lights at (9, 9) on a 34 px bar, centered at 17.
    // At 200% the bar is 68 points high and their center moves to 34.
    let lights = traffic_light_out(point(px(9.), px(9.)), 2.);
    assert_eq!(lights, point(px(18.), px(26.)));
    assert_eq!(f32::from(lights.y) + TRAFFIC_LIGHT_HALF, 34.);
    assert_eq!(
        traffic_light_out(point(px(9.), px(9.)), 1.),
        point(px(9.), px(9.))
    );
}

#[test]
fn pointer_events_shrink_into_zoomed_space() {
    let down = PlatformInput::MouseDown(MouseDownEvent {
        button: MouseButton::Left,
        position: point(px(300.), px(150.)),
        ..Default::default()
    });
    let PlatformInput::MouseDown(e) = zoom_input(down, 1.5) else {
        panic!("event kind changed");
    };
    assert_eq!(e.position, point(px(200.), px(100.)));

    let scroll = PlatformInput::Scroll(ScrollEvent {
        position: point(px(40.), px(40.)),
        delta: ScrollDelta::Pixels(point(px(0.), px(-30.))),
        ..Default::default()
    });
    let PlatformInput::Scroll(e) = zoom_input(scroll, 2.) else {
        panic!("event kind changed");
    };
    assert_eq!(e.position, point(px(20.), px(20.)));
    let ScrollDelta::Pixels(delta) = e.delta else {
        panic!("delta kind changed");
    };
    assert_eq!(delta, point(px(0.), px(-15.)));

    let lines = PlatformInput::Scroll(ScrollEvent {
        delta: ScrollDelta::Lines(point(0., 3.)),
        ..Default::default()
    });
    let PlatformInput::Scroll(e) = zoom_input(lines, 2.) else {
        panic!("event kind changed");
    };
    assert!(matches!(e.delta, ScrollDelta::Lines(l) if l == point(0., 3.)));
}

#[test]
fn file_drops_shrink_and_exits_pass_through() {
    let drop = PlatformInput::FileDrop(FileDropEvent::Submit {
        position: point(px(90.), px(30.)),
    });
    let PlatformInput::FileDrop(FileDropEvent::Submit { position }) = zoom_input(drop, 3.) else {
        panic!("event kind changed");
    };
    assert_eq!(position, point(px(30.), px(10.)));
    assert!(matches!(
        zoom_input(PlatformInput::FileDrop(FileDropEvent::Exited), 3.),
        PlatformInput::FileDrop(FileDropEvent::Exited)
    ));
}

#[test]
fn zoom_change_relays_out_open_windows_and_skips_closed_ones() {
    set_initial_zoom(1.);
    let seen: Rc<RefCell<Vec<(Size<Pixels>, f32)>>> = Rc::default();
    let state = WindowZoom::register(1., Size::new(px(1200.), px(800.)), 2., None);
    let log = seen.clone();
    *state.resize.borrow_mut() = Some(Box::new(move |size, scale| {
        log.borrow_mut().push((size, scale));
    }));
    let closed = WindowZoom::register(1., Size::new(px(10.), px(10.)), 1., None);
    drop(closed);

    set_zoom(1.5);
    assert_eq!(current_zoom(), 1.5);
    assert_eq!(state.zoom.get(), 1.5);
    assert_eq!(
        seen.borrow().as_slice(),
        &[(Size::new(px(800.), px(800. / 1.5)), 3.)]
    );
    // The callback is back in place for the next change; an unchanged
    // zoom doesn't re-lay out.
    set_zoom(1.5);
    assert_eq!(seen.borrow().len(), 1);
    set_zoom(1.);
    assert_eq!(
        seen.borrow().last(),
        Some(&(Size::new(px(1200.), px(800.)), 2.))
    );
    assert!(WINDOWS.with(|l| l.borrow().iter().all(|w| w.strong_count() > 0)));
}

#[test]
fn sanitize_rejects_nonsense() {
    assert_eq!(sanitize(f32::NAN), 1.);
    assert_eq!(sanitize(10.), 4.);
    assert_eq!(sanitize(1.25), 1.25);
}

/// Every method of GPUI's `Platform` and `PlatformWindow` traits must be
/// forwarded, defaulted ones included (a vendored platform may override
/// a default, like `frame_waker`). After a GPUI upgrade, this compares
/// the vendored trait source with this file.
#[test]
fn forwards_every_trait_method() {
    let source = GPUI_PLATFORM_SOURCE;
    let this = include_str!("mod.rs");
    // Methods that only exist for GPUI's own tests, and the Wayland
    // layer-shell edge (behind a GPUI feature Quill doesn't enable).
    let skip = ["as_test", "set_exclusive_edge"];
    let mut missing = Vec::new();
    let names: Vec<String> = ["pub trait Platform: 'static", "pub trait PlatformWindow:"]
        .iter()
        .flat_map(|t| trait_methods(source, t))
        .collect();
    // Both traits together have well over a hundred methods; far fewer
    // means the parser lost track of the trait bodies.
    assert!(names.len() > 120, "parsed only {} methods", names.len());
    for name in names {
        if skip.contains(&name.as_str()) {
            continue;
        }
        if !this.contains(&format!("fn {name}(")) {
            missing.push(name);
        }
    }
    assert!(
        missing.is_empty(),
        "interface_zoom.rs doesn't forward: {missing:?}"
    );
}

fn trait_methods(source: &str, header: &str) -> Vec<String> {
    let Some(start) = source.find(header) else {
        panic!("trait `{header}` not found in gpui's platform.rs");
    };
    let mut depth = 0i32;
    let mut body = String::new();
    for ch in source[start..].chars() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        // Only direct members of the trait body.
        if depth == 1 {
            body.push(ch);
        } else if depth > 1 {
            body.push(' ');
        }
    }
    body.split("fn ")
        .skip(1)
        .filter_map(|rest| {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            (!name.is_empty()).then_some(name)
        })
        .collect()
}

/// GPUI's `platform.rs` as Quill builds it: gpui-pre is vendored
/// (`third_party/gpui-pre`, patched in through `[patch.crates-io]`), so
/// the cargo registry has no copy of it.
const GPUI_PLATFORM_SOURCE: &str = include_str!("../../../third_party/gpui-pre/src/platform.rs");
