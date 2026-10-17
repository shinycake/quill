//! Input events that `QUILL_DEMO_CLICK` steps send to a screenshot demo.
#![cfg(feature = "demo-capture")]

use std::time::Duration;

use gpui_kit::gpui::{
    AnyWindowHandle, AsyncApp, KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, PlatformInput, Point, ScrollDelta,
    ScrollWheelEvent, TouchPhase, point, px,
};

/// Runs the `QUILL_DEMO_CLICK=step[;step…]` script against the demo window,
/// one step at a time; does nothing when the variable is unset.
///
/// Through the untyped handle: the typed one leases the root view while the
/// event dispatches, and a handler reading it (a kit button) panicked.
pub(crate) async fn run_steps(demo_window: AnyWindowHandle, cx: &mut AsyncApp) {
    let Ok(script) = std::env::var("QUILL_DEMO_CLICK") else {
        return;
    };
    // `QUILL_DEMO_CLICK_LOG=path` records when each step ran
    // (Unix ms), so a recording can be lined up with it.
    let mut step_log = std::env::var_os("QUILL_DEMO_CLICK_LOG").and_then(|path| {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .ok()
    });
    for step in script.split(';') {
        if let Some(log) = step_log.as_mut() {
            use std::io::Write;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or_default();
            let _ = writeln!(log, "{now} {}", step.trim());
        }
        run_step(step, demo_window, cx).await;
    }
}

async fn run_step(step: &str, demo_window: AnyWindowHandle, cx: &mut AsyncApp) {
    // `p:ms` waits, to pace a scripted recording.
    if let Some(ms) = step.strip_prefix("p:") {
        if let Ok(ms) = ms.trim().parse::<u64>() {
            pause(cx, ms).await;
        }
        return;
    }
    // `mid:x,y` clicks the middle button (press, short
    // pause, release), to start middle-click autoscroll.
    if let Some(at) = step.strip_prefix("mid:") {
        if let Some(position) = point_step(at) {
            dispatch(
                demo_window,
                cx,
                [
                    PlatformInput::MouseMove(MouseMoveEvent {
                        position,
                        pressed_button: None,
                        modifiers: Modifiers::default(),
                    }),
                    PlatformInput::MouseDown(MouseDownEvent {
                        button: MouseButton::Middle,
                        position,
                        modifiers: Modifiers::default(),
                        click_count: 1,
                        first_mouse: false,
                    }),
                ],
                false,
            );
            pause(cx, 60).await;
            dispatch(
                demo_window,
                cx,
                [PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Middle,
                    position,
                    modifiers: Modifiers::default(),
                    click_count: 1,
                })],
                false,
            );
        }
        return;
    }
    // `m:x,y` moves the pointer without clicking (hover).
    if let Some(hover) = step.strip_prefix("m:") {
        if let Some(position) = point_step(hover) {
            dispatch(
                demo_window,
                cx,
                [PlatformInput::MouseMove(MouseMoveEvent {
                    position,
                    pressed_button: None,
                    modifiers: Modifiers::default(),
                })],
                false,
            );
        }
        return;
    }
    // `k:key` presses one key (GPUI key names, e.g. `escape`).
    if let Some(key) = step.strip_prefix("k:") {
        let keystroke = Keystroke {
            modifiers: Modifiers::default(),
            key: key.trim().to_string(),
            key_char: None,
        };
        dispatch(
            demo_window,
            cx,
            [
                PlatformInput::KeyDown(KeyDownEvent {
                    keystroke: keystroke.clone(),
                    is_held: false,
                    prefer_character_input: false,
                }),
                PlatformInput::KeyUp(KeyUpEvent { keystroke }),
            ],
            false,
        );
        pause(cx, 120).await;
        return;
    }
    // `w:` sends one phased trackpad scroll event ([`wheel_step`]).
    if let Some(wheel) = step.strip_prefix("w:") {
        if let Some(event) = wheel_step(wheel) {
            dispatch(demo_window, cx, [PlatformInput::ScrollWheel(event)], false);
        }
        pause(cx, 40).await;
        return;
    }
    // `s:x,y,dy` scrolls by `dy` px at the point instead.
    if let Some(scroll) = step.strip_prefix("s:") {
        let parts: Vec<f32> = scroll
            .split(',')
            .filter_map(|v| v.trim().parse().ok())
            .collect();
        if let [x, y, dy] = parts[..] {
            dispatch(
                demo_window,
                cx,
                [PlatformInput::ScrollWheel(ScrollWheelEvent {
                    position: point(px(x), px(y)),
                    delta: ScrollDelta::Pixels(point(px(0.), px(dy))),
                    modifiers: Modifiers::default(),
                    touch_phase: TouchPhase::Moved,
                })],
                false,
            );
        }
        pause(cx, 400).await;
        return;
    }
    let Some(position) = point_step(step) else {
        return;
    };
    dispatch(
        demo_window,
        cx,
        [
            PlatformInput::MouseMove(MouseMoveEvent {
                position,
                pressed_button: None,
                modifiers: Modifiers::default(),
            }),
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position,
                modifiers: Modifiers::default(),
                click_count: 1,
                first_mouse: false,
            }),
        ],
        true,
    );
    // A real click spans frames: let one render between
    // press and release.
    pause(cx, 120).await;
    dispatch(
        demo_window,
        cx,
        [PlatformInput::MouseUp(MouseUpEvent {
            button: MouseButton::Left,
            position,
            modifiers: Modifiers::default(),
            click_count: 1,
        })],
        false,
    );
    pause(cx, 400).await;
}

/// Sends `events` to the window in order, then asks it to repaint when
/// `refresh` is set. A closed window is skipped.
fn dispatch<const N: usize>(
    demo_window: AnyWindowHandle,
    cx: &mut AsyncApp,
    events: [PlatformInput; N],
    refresh: bool,
) {
    let _ = demo_window.update(cx, |_, window, cx| {
        for event in events {
            window.dispatch_event(event, cx);
        }
        if refresh {
            window.refresh();
        }
    });
}

async fn pause(cx: &mut AsyncApp, ms: u64) {
    cx.background_executor()
        .timer(Duration::from_millis(ms))
        .await;
}

/// An `x,y` window point, the target of a click, hover or middle click.
fn point_step(step: &str) -> Option<Point<Pixels>> {
    let (x, y) = step.split_once(',')?;
    Some(point(
        px(x.trim().parse::<f32>().ok()?),
        px(y.trim().parse::<f32>().ok()?),
    ))
}

/// The `w:` step `x,y,dx,dy,s|m|e[,cmd]`: one phased trackpad scroll event
/// (started / moved / ended), to script a swipe. `cmd` holds Cmd (Ctrl off
/// macOS). `None` when the step does not parse.
pub(crate) fn wheel_step(step: &str) -> Option<ScrollWheelEvent> {
    let mut parts: Vec<&str> = step.split(',').map(str::trim).collect();
    let primary = parts.last() == Some(&"cmd");
    if primary {
        parts.pop();
    }
    let [x, y, dx, dy, phase] = parts.as_slice() else {
        return None;
    };
    let touch_phase = match *phase {
        "s" => TouchPhase::Started,
        "e" => TouchPhase::Ended,
        _ => TouchPhase::Moved,
    };
    Some(ScrollWheelEvent {
        position: point(px(x.parse().ok()?), px(y.parse().ok()?)),
        delta: ScrollDelta::Pixels(point(px(dx.parse().ok()?), px(dy.parse().ok()?))),
        modifiers: if primary {
            Modifiers::secondary_key()
        } else {
            Modifiers::default()
        },
        touch_phase,
    })
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wheel_step_parses_its_phase_and_the_optional_cmd() {
        let started = wheel_step("10, 20, 0, -70, s").expect("a step");
        assert_eq!(started.position, point(px(10.), px(20.)));
        assert!(matches!(
            started.delta,
            ScrollDelta::Pixels(delta) if delta == point(px(0.), px(-70.))
        ));
        assert_eq!(started.touch_phase, TouchPhase::Started);
        assert_eq!(started.modifiers, Modifiers::default());
        let zoom = wheel_step("10,20,0,40,m,cmd").expect("a step");
        assert_eq!(zoom.touch_phase, TouchPhase::Moved);
        assert_eq!(zoom.modifiers, Modifiers::secondary_key());
        assert!(wheel_step("10,20,0").is_none());
        assert!(wheel_step("10,20,x,40,e").is_none());
    }
}
