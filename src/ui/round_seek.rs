//! The seek ring on a round video message playing with sound (tdesktop
//! `VideoMessageSeek`): a thin progress arc while it plays; paused, a
//! shaded edge, a track and a draggable dot spring in, and dragging along
//! the edge seeks.

use super::app::QuillApp;
use gpui_kit::*;
use std::f32::consts::{FRAC_PI_2, TAU};

/// tdesktop `chat.style`: `radialLine`, `historyVideoMessageSeekLine`,
/// `historyVideoMessageSeekInset`, the dot sizes and the grab band (for
/// its 240 px round; scaled to ours).
const LINE: f32 = 3.;
const SEEK_LINE: f32 = 5.;
const SEEK_INSET: f32 = 18.;
const DOT_MIN: f32 = 6.;
const DOT: f32 = 16.;
const DOT_GRABBED: f32 = 22.;
const GRAB_BAND: f32 = 40. / 240.;
const PROGRESS_OPACITY: f32 = 0.72;
const TRACK_OPACITY: f32 = 0.2;
const SHADOW_OPACITY: f32 = 0.16;
const SHADOW_INNER: f32 = 0.7;

/// Where along the ring `point` is, clockwise from the top, `0.0..1.0`.
fn ring_fraction(center: Point<Pixels>, point: Point<Pixels>) -> f32 {
    let (dx, dy) = ((point.x - center.x) / px(1.), (point.y - center.y) / px(1.));
    let angle = dx.atan2(-dy);
    (if angle < 0. { angle + TAU } else { angle }) / TAU
}

/// Points along the circle from the top, clockwise, for `fraction` of it.
fn arc_points(center: Point<Pixels>, radius: f32, fraction: f32) -> Vec<Point<Pixels>> {
    let steps = ((fraction * 160.).ceil() as usize).max(2);
    (0..=steps)
        .map(|step| {
            let angle = -FRAC_PI_2 + TAU * fraction * step as f32 / steps as f32;
            point(
                center.x + px(radius * angle.cos()),
                center.y + px(radius * angle.sin()),
            )
        })
        .collect()
}

fn stroke(window: &mut Window, points: &[Point<Pixels>], width: f32, color: Hsla, closed: bool) {
    let mut path = PathBuilder::stroke(px(width));
    path.add_polygon(points, closed);
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}

/// The ring over a sounding round video: `shown` and `grabbed` are the
/// seek ring's spring and the dot's grab (see `InlineFrame`).
/// tdesktop's edge shade: clear to 70% of the radius, then a radial
/// ramp to 16% black at the rim. One image per size.
fn edge_shade(edge: u32) -> std::sync::Arc<RenderImage> {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::sync::Arc;
    thread_local! {
        static SHADES: RefCell<HashMap<u32, Arc<RenderImage>>> = RefCell::new(HashMap::new());
    }
    SHADES.with(|shades| {
        shades
            .borrow_mut()
            .entry(edge)
            .or_insert_with(|| {
                let radius = edge as f32 / 2.0;
                let mut image = image::RgbaImage::new(edge, edge);
                for (x, y, pixel) in image.enumerate_pixels_mut() {
                    let dx = x as f32 + 0.5 - radius;
                    let dy = y as f32 + 0.5 - radius;
                    let distance = (dx * dx + dy * dy).sqrt() / radius;
                    let ramp = ((distance - SHADOW_INNER) / (1.0 - SHADOW_INNER)).clamp(0.0, 1.0);
                    // Anti-aliased rim.
                    let inside = (radius - distance * radius + 0.5).clamp(0.0, 1.0);
                    let alpha = SHADOW_OPACITY * ramp * inside;
                    *pixel = image::Rgba([0, 0, 0, (alpha * 255.0).round() as u8]);
                }
                Arc::new(RenderImage::new(smallvec::SmallVec::from_buf([
                    image::Frame::new(image),
                ])))
            })
            .clone()
    })
}

pub(super) fn round_seek_overlay(
    chat_id: i64,
    message_id: i64,
    progress: f32,
    shown: f32,
    grabbed: f32,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let app = cx.entity().downgrade();
    let shade = (shown > 0.).then(|| {
        img(ImageSource::Render(edge_shade(440)))
            .absolute()
            .inset_0()
            .size_full()
            .opacity(shown.clamp(0., 1.))
    });
    let ring = canvas(
        move |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
        move |bounds, hitbox, window, _| {
            let center = bounds.center();
            let diameter = bounds.size.width / px(1.);
            let radius = diameter / 2.;
            let scale = diameter / 240.;
            let opacity = shown.clamp(0., 1.);
            let line = LINE + (SEEK_LINE - LINE) * opacity;
            let inset = 1.5 * LINE + (SEEK_INSET * scale - 1.5 * LINE) * shown;
            let arc_radius = radius - inset;
            if opacity > 0. {
                stroke(
                    window,
                    &arc_points(center, arc_radius, 1.0),
                    line,
                    white().opacity(TRACK_OPACITY * opacity),
                    true,
                );
            }
            if progress > 0. {
                stroke(
                    window,
                    &arc_points(center, arc_radius, progress),
                    line,
                    white().opacity(PROGRESS_OPACITY),
                    false,
                );
            }
            if opacity > 0. {
                let dot = (DOT_MIN + (DOT - DOT_MIN) * shown + (DOT_GRABBED - DOT) * grabbed)
                    * scale.max(0.75);
                let angle = -FRAC_PI_2 + TAU * progress;
                let at = point(
                    center.x + px(arc_radius * angle.cos()),
                    center.y + px(arc_radius * angle.sin()),
                );
                window.paint_quad(
                    fill(
                        Bounds::centered_at(at, size(px(dot), px(dot))),
                        white().opacity(opacity),
                    )
                    .corner_radii(px(dot / 2.)),
                );
            }

            // Press within the band along the edge and drag to seek; a
            // press there doesn't toggle playback.
            let band = diameter * GRAB_BAND;
            let in_band = move |position: Point<Pixels>| {
                let (dx, dy) = (
                    (position.x - center.x) / px(1.),
                    (position.y - center.y) / px(1.),
                );
                let distance = (dx * dx + dy * dy).sqrt();
                distance <= radius && distance >= radius - band
            };
            {
                let (app, hitbox) = (app.clone(), hitbox.clone());
                window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                    if phase != DispatchPhase::Bubble
                        || event.button != MouseButton::Left
                        || !hitbox.is_hovered(window)
                        || !in_band(event.position)
                    {
                        return;
                    }
                    let fraction = ring_fraction(center, event.position);
                    let seeking = app
                        .update(cx, |this, cx| {
                            let took = this
                                .inline_videos
                                .borrow_mut()
                                .seek_to(chat_id, message_id, fraction);
                            cx.notify();
                            took
                        })
                        .unwrap_or(false);
                    if seeking {
                        cx.stop_propagation();
                    }
                });
            }
            {
                let app = app.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                    if phase != DispatchPhase::Capture
                        || event.pressed_button != Some(MouseButton::Left)
                    {
                        return;
                    }
                    let fraction = ring_fraction(center, event.position);
                    let _ = app.update(cx, |this, cx| {
                        let mut videos = this.inline_videos.borrow_mut();
                        if videos.is_seeking(chat_id, message_id) {
                            videos.seek_to(chat_id, message_id, fraction);
                            drop(videos);
                            cx.notify();
                        }
                    });
                });
            }
            window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                if phase != DispatchPhase::Capture {
                    return;
                }
                let _ = app.update(cx, |this, cx| {
                    let mut videos = this.inline_videos.borrow_mut();
                    if videos.is_seeking(chat_id, message_id) {
                        videos.end_seek(chat_id, message_id);
                        drop(videos);
                        cx.notify();
                    }
                });
            });
        },
    )
    .absolute()
    .inset_0()
    .size_full();
    div()
        .absolute()
        .inset_0()
        .children(shade)
        .child(ring)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::ring_fraction;
    use gpui_kit::{point, px};

    #[test]
    fn ring_fraction_runs_clockwise_from_the_top() {
        let center = point(px(100.), px(100.));
        let at = |x: f32, y: f32| ring_fraction(center, point(px(x), px(y)));
        assert!(at(100., 0.).abs() < 1e-4);
        assert!((at(200., 100.) - 0.25).abs() < 1e-4);
        assert!((at(100., 200.) - 0.5).abs() < 1e-4);
        assert!((at(0., 100.) - 0.75).abs() < 1e-4);
    }
}
