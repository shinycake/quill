//! The indicator's geometry for one frame (`Indicator::paintEvent`).

use super::*;

/// Indicator geometry for one frame (`Indicator::paintEvent`), in px from
/// the indicator's top-left; the indicator is [`MAX_HEIGHT`] tall and
/// stands on the history's bottom edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IndicatorLayout {
    /// The capsule with its arrow (absent once fully ready).
    pub capsule: Option<Capsule>,
    /// The next chat's name pill top and opacity (only while turning
    /// ready).
    pub name: Option<(f32, f32)>,
    /// The photo (or the "nothing left" check) box and opacity.
    pub avatar: Option<(Rect, f32)>,
    /// The badge's scale (0 hides it).
    pub badge_scale: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Capsule {
    pub rect: Rect,
    pub radius: f32,
    pub alpha: f32,
    /// Arrow centre, drawn size (`ARROW_SIZE` at full) and opacity.
    pub arrow: (f32, f32, f32, f32),
}

impl IndicatorLayout {
    /// `width`: the content width the indicator is centred in.
    pub fn compute(state: IndicatorState, width: f32) -> Self {
        let h = MAX_HEIGHT;
        let offset = state.offset;
        let card = EXPAND;
        let progress = (offset / card).clamp(0., 1.);
        let release = state.release;
        let alpha = progress;
        let cx = width / 2.;
        let circle_radius = AVATAR / 2.;
        let width_radius =
            (circle_radius * (offset - SKIP) / (card - SKIP)).clamp(0., circle_radius);
        let width_radius2 = (circle_radius * progress)
            .min(offset / 2. - SKIP * progress)
            .max(0.);
        let size = (width_radius2 * 2. - INSET) * (1. - release) + AVATAR * release;
        let capsule = (release < 1.)
            .then(|| {
                let top = h - offset;
                let bottom = h + (-SKIP * (1. - release) + (-offset + AVATAR) * release);
                let mut rect = Rect {
                    x: cx - width_radius,
                    y: top,
                    w: 2. * width_radius,
                    h: bottom - top,
                };
                let mut bg_alpha = alpha;
                if release > 0. {
                    let inset = INSET * release;
                    rect = Rect {
                        x: rect.x + inset,
                        y: rect.y + inset,
                        w: rect.w - 2. * inset,
                        h: rect.h - 2. * inset,
                    };
                    bg_alpha = alpha * (1. - release);
                }
                let arrow_cy = h + (-offset + ARROW_SIZE + SKIP * (1. - progress) - RISE * release);
                (rect.w > 0. && rect.h > 0.).then_some(Capsule {
                    rect,
                    radius: width_radius.min(rect.w / 2.).min(rect.h / 2.),
                    alpha: bg_alpha,
                    arrow: (cx, arrow_cy, ARROW_SIZE * progress, alpha * (1. - release)),
                })
            })
            .flatten();
        let name = (release > 0.).then(|| {
            let y = h + (NAME_TOP * (1. - release) - RISE * release) + state.bounce_offset;
            (y - PADDING, alpha * release)
        });
        let avatar = (size >= 1.).then(|| {
            let top = h
                + ((-SKIP - SKIP * progress - size) * (1. - release)
                    + (-offset.min(card) + PADDING) * release)
                + state.bounce_offset;
            (
                Rect {
                    x: cx - size / 2.,
                    y: top,
                    w: size,
                    h: size,
                },
                alpha,
            )
        });
        Self {
            capsule,
            name,
            avatar,
            badge_scale: release,
        }
    }
}

/// The arrow's two strokes for a centre and drawn size (`PaintArrow`): the
/// shaft and the head, each a polyline.
pub fn arrow_strokes(cx: f32, cy: f32, size: f32) -> [Vec<(f32, f32)>; 2] {
    let scale = size / ARROW_SIZE;
    let map = |x: f32, y: f32| (cx + scale * (x - 12.), cy - ARROW_RAISE + scale * (y + 8.));
    [
        vec![map(12.5, 4.), map(12.5, 22.)],
        vec![map(3.5, 12.), map(12.5, 3.5), map(21.5, 12.)],
    ]
}

/// The check drawn when no channel is left (`PaintCheck`).
pub fn check_stroke(cx: f32, cy: f32, size: f32) -> Vec<(f32, f32)> {
    let s = size / 2.4;
    vec![
        (cx - s, cy),
        (cx - s / 3., cy + s * 0.7),
        (cx + s, cy - s * 0.7),
    ]
}

/// The unread badge text (`999+` past 999).
pub fn badge_text(count: i32) -> String {
    if count > 999 {
        "999+".into()
    } else {
        count.to_string()
    }
}
