//! How a pinned row looks while it is being reordered: it lifts off the
//! list as a card with a shadow, and the slot it will land in shows as a
//! dashed outline. Both follow `PinReorder::lift`, so they grow in on pick
//! up and fade out as the row settles.

use super::*;

/// A row of the pinned drag. `dy` is its offset from its slot, `lift` how
/// far it is picked up (0 to 1).
pub(super) fn pin_drag_row(
    row: AnyElement,
    dy: f32,
    lift: f32,
    height: Pixels,
    cx: &App,
) -> AnyElement {
    if lift <= 0. {
        if dy == 0. {
            return row;
        }
        return div()
            .relative()
            .top(px(dy))
            .w_full()
            .child(row)
            .into_any_element();
    }
    let theme = cx.theme();
    let slot = div()
        .absolute()
        .inset_0()
        .rounded_md()
        .border_1()
        .border_dashed()
        .border_color(theme.primary.opacity(0.6 * lift))
        .bg(theme.primary.opacity(0.06 * lift));
    let shadow = |alpha: f32, y: f32, blur: f32| BoxShadow {
        color: gpui_kit::black().opacity(alpha * lift),
        offset: point(px(0.), px(y * lift)),
        blur_radius: px(blur * lift),
        spread_radius: px(0.),
        inset: false,
    };
    let card = div()
        .absolute()
        .left_0()
        .right_0()
        .top(px(dy))
        .h(height)
        .rounded_md()
        .border_1()
        .border_color(theme.border.opacity(lift))
        .bg(theme.sidebar)
        .shadow(vec![shadow(0.08, 1., 2.), shadow(0.16, 8., 24.)])
        .child(row);
    div()
        .relative()
        .w_full()
        .h(height)
        .child(slot)
        .child(deferred(card).priority(10))
        .into_any_element()
}
