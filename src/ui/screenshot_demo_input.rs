//! Input events that `QUILL_DEMO_CLICK` steps send to a screenshot demo.
#![cfg(feature = "demo-capture")]

use gpui_kit::gpui::{Modifiers, PinchEvent, ScrollDelta, ScrollWheelEvent, TouchPhase, point, px};

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

/// The `z:` step `x,y,delta`: one trackpad pinch centred on the point,
/// `delta` as the platform reports it (0.1 is a tenth more magnification,
/// negative pinches out). `None` when the step does not parse.
pub(crate) fn pinch_step(step: &str) -> Option<PinchEvent> {
    let parts: Vec<&str> = step.split(',').map(str::trim).collect();
    let [x, y, delta] = parts.as_slice() else {
        return None;
    };
    let delta: f32 = delta.parse().ok()?;
    if !delta.is_finite() {
        return None;
    }
    Some(PinchEvent {
        position: point(px(x.parse().ok()?), px(y.parse().ok()?)),
        delta,
        modifiers: Modifiers::default(),
        phase: TouchPhase::Moved,
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

    #[test]
    fn a_pinch_step_parses_its_centre_and_delta() {
        let pinch = pinch_step("800, 500, 0.6").expect("a step");
        assert_eq!(pinch.position, point(px(800.), px(500.)));
        assert_eq!(pinch.delta, 0.6);
        assert_eq!(pinch.phase, TouchPhase::Moved);
        assert!(pinch_step("800,500").is_none());
        assert!(pinch_step("800,500,x").is_none());
        assert!(pinch_step("800,500,inf").is_none());
    }
}
