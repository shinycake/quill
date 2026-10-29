//! Coordinate mapping between GPUI's zoomed logical pixels and the
//! platform's: the helpers [`super::ZoomWindow`] uses, and input events.

use gpui_kit::{Bounds, FileDropEvent, Pixels, PlatformInput, Point, ScrollDelta, Size, px};

/// GPUI logical pixels → the platform's logical pixels.
pub(super) fn out(v: Pixels, zoom: f32) -> Pixels {
    px(f32::from(v) * zoom)
}

/// The platform's logical pixels → GPUI logical pixels.
pub(super) fn inn(v: Pixels, zoom: f32) -> Pixels {
    px(f32::from(v) / zoom)
}

pub(super) fn point_in(p: Point<Pixels>, zoom: f32) -> Point<Pixels> {
    p.map(|v| inn(v, zoom))
}

pub(super) fn point_out(p: Point<Pixels>, zoom: f32) -> Point<Pixels> {
    p.map(|v| out(v, zoom))
}

/// Half the height of a macOS traffic-light button frame, in points.
pub(super) const TRAFFIC_LIGHT_HALF: f32 = 8.;

/// Where to put the macOS traffic lights so they stay centered on the
/// zoomed title bar: the buttons themselves don't scale, so the point
/// that scales is their center, not their corner.
pub(super) fn traffic_light_out(p: Point<Pixels>, zoom: f32) -> Point<Pixels> {
    let y = (f32::from(p.y) + TRAFFIC_LIGHT_HALF) * zoom - TRAFFIC_LIGHT_HALF;
    Point::new(out(p.x, zoom), px(y))
}

pub(super) fn size_in(s: Size<Pixels>, zoom: f32) -> Size<Pixels> {
    s.map(|v| inn(v, zoom))
}

pub(super) fn size_out(s: Size<Pixels>, zoom: f32) -> Size<Pixels> {
    s.map(|v| out(v, zoom))
}

pub(super) fn bounds_in(b: Bounds<Pixels>, zoom: f32) -> Bounds<Pixels> {
    b.map(|v| inn(v, zoom))
}

pub(super) fn bounds_out(b: Bounds<Pixels>, zoom: f32) -> Bounds<Pixels> {
    b.map(|v| out(v, zoom))
}

/// Map a platform input event into the zoomed coordinate space: positions
/// and pixel scroll deltas shrink by `zoom` (content follows the fingers on
/// a trackpad); line deltas, keys and pinch amounts are unchanged.
pub(crate) fn zoom_input(input: PlatformInput, zoom: f32) -> PlatformInput {
    if (zoom - 1.).abs() < f32::EPSILON {
        return input;
    }
    let p = |p: Point<Pixels>| point_in(p, zoom);
    match input {
        PlatformInput::MouseDown(mut e) => {
            e.position = p(e.position);
            PlatformInput::MouseDown(e)
        }
        PlatformInput::MouseUp(mut e) => {
            e.position = p(e.position);
            PlatformInput::MouseUp(e)
        }
        PlatformInput::MousePressure(mut e) => {
            e.position = p(e.position);
            PlatformInput::MousePressure(e)
        }
        PlatformInput::MouseMove(mut e) => {
            e.position = p(e.position);
            PlatformInput::MouseMove(e)
        }
        PlatformInput::MouseExited(mut e) => {
            e.position = p(e.position);
            PlatformInput::MouseExited(e)
        }
        PlatformInput::Scroll(mut e) => {
            e.position = p(e.position);
            if let ScrollDelta::Pixels(delta) = e.delta {
                e.delta = ScrollDelta::Pixels(p(delta));
            }
            PlatformInput::Scroll(e)
        }
        PlatformInput::Pinch(mut e) => {
            e.position = p(e.position);
            PlatformInput::Pinch(e)
        }
        PlatformInput::LongPress(mut e) => {
            e.start_position = p(e.start_position);
            e.position = p(e.position);
            PlatformInput::LongPress(e)
        }
        PlatformInput::TouchDrag(mut e) => {
            e.start_position = p(e.start_position);
            e.position = p(e.position);
            PlatformInput::TouchDrag(e)
        }
        PlatformInput::BackGesture(mut e) => {
            e.start_position = p(e.start_position);
            PlatformInput::BackGesture(e)
        }
        PlatformInput::Touch(mut e) => {
            e.position = p(e.position);
            e.predicted_position = e.predicted_position.map(p);
            PlatformInput::Touch(e)
        }
        PlatformInput::FileDrop(event) => PlatformInput::FileDrop(match event {
            FileDropEvent::Entered { position, paths } => FileDropEvent::Entered {
                position: p(position),
                paths,
            },
            FileDropEvent::Pending { position } => FileDropEvent::Pending {
                position: p(position),
            },
            FileDropEvent::Submit { position } => FileDropEvent::Submit {
                position: p(position),
            },
            other => other,
        }),
        other @ (PlatformInput::KeyDown(_)
        | PlatformInput::KeyUp(_)
        | PlatformInput::ModifiersChanged(_)) => other,
    }
}
