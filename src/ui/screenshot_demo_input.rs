//! Input events that `QUILL_DEMO_CLICK` steps send to a screenshot demo.
#![cfg(feature = "demo-capture")]

use gpui_kit::gpui::{Modifiers, ScrollDelta, ScrollWheelEvent, TouchPhase, point, px};

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
