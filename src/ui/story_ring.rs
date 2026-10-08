//! Story ring around a chat-list avatar (tdesktop
//! `Dialogs::Row::PaintCornerBadgeFrame`, `dialogs/dialogs_row.cpp:520-580`).
//! Geometry lives in `quill::story_ring`; this paints it.

use super::chat_theme::accent_strong;
use gpui_kit::*;
use quill::story_ring::{READ_WIDTH, RingArc, StoryRing, UNREAD_WIDTH, arcs};

/// Unread arcs: the accent, lighter towards the top right
/// (`Ui::UnreadStoryOutlineGradient` runs top-right to bottom-left).
fn unread_fill() -> Background {
    let base: Hsla = accent_strong().into();
    let light = hsla(
        (base.h + 0.07).fract(),
        base.s,
        (base.l + 0.14).min(0.85),
        base.a,
    );
    linear_gradient(
        225.,
        linear_color_stop(light, 0.),
        linear_color_stop(base, 1.),
    )
}

/// Read arcs: `dialogsUnreadBgMuted`, the muted grey of read badges.
fn read_fill(muted: Hsla) -> Background {
    muted.opacity(0.55).into()
}

fn arc_points(center: Point<Pixels>, radius: f32, arc: &RingArc) -> Vec<Point<Pixels>> {
    let steps = ((arc.sweep / 4.).ceil() as usize).max(2);
    (0..=steps)
        .map(|step| {
            // Counter-clockwise from 3 o'clock; screen y points down.
            let angle = (arc.start + arc.sweep * step as f32 / steps as f32).to_radians();
            point(
                center.x + px(radius * angle.cos()),
                center.y - px(radius * angle.sin()),
            )
        })
        .collect()
}

const PAD: f32 = 2.;

/// `inner(size)` builds the avatar at the shrunken size; the ring is
/// painted over a `size` box so rows keep their layout. `muted` is the
/// read-arc colour (theme muted foreground).
pub(super) fn with_story_ring(
    inner: impl FnOnce(f32) -> AnyElement,
    ring: StoryRing,
    size: f32,
    muted: Hsla,
) -> AnyElement {
    let avatar = StoryRing::avatar_size(size);
    let segments = arcs(ring);
    div()
        .relative()
        .flex_none()
        .size(px(size))
        .flex()
        .items_center()
        .justify_center()
        .child(inner(avatar))
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let center = bounds.center();
                    // The stroke is centred on the photo's edge, like
                    // `outline = QRectF(0, 0, photoSize, photoSize)`.
                    let radius = size / 2.;
                    for arc in &segments {
                        let width = if arc.unread { UNREAD_WIDTH } else { READ_WIDTH };
                        let mut path = PathBuilder::stroke(px(width));
                        path.add_polygon(&arc_points(center, radius, arc), arc.sweep >= 360.);
                        if let Ok(path) = path.build() {
                            window.paint_path(
                                path,
                                if arc.unread {
                                    unread_fill()
                                } else {
                                    read_fill(muted)
                                },
                            );
                        }
                    }
                },
            )
            // The stroke straddles the photo's edge, so the canvas has a
            // little room on every side.
            .absolute()
            .top(px(-PAD))
            .left(px(-PAD))
            .size(px(size + 2. * PAD)),
        )
        .into_any_element()
}
