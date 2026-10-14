//! What the tooltip over a chart's point says: the date, then each shown
//! line's value with its share on a percentage chart; a chevron when a
//! click zooms in (tdesktop `point_details_widget.cpp`). The kit charts
//! draw the box, the crosshair and the dots; the same body reads under
//! the header for the point picked with the arrow keys.

use super::series::Point;
use gpui_kit::assets::IconName;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::stat_chart::{StatChart, format_decimal, percentages};

/// The tooltip body for `point`: each line's share too when `shares`,
/// the chevron when `zoomable`.
pub(super) fn body(
    chart: &StatChart,
    point: &Point,
    enabled: &[bool],
    shares: bool,
    zoomable: bool,
    cx: &App,
) -> Div {
    let theme = cx.theme();
    let offset = quill::local_time::utc_offset_at(quill::local_time::now_unix());
    let title = chart.point_title(point.index, offset);
    let raw: Vec<i64> = chart
        .lines
        .iter()
        .map(|line| line.y.get(point.index).copied().unwrap_or(0))
        .collect();
    let shares = shares.then(|| percentages(&raw, enabled));
    let mut header = div()
        .flex()
        .items_center()
        .justify_between()
        .gap_3()
        .child(div().font_semibold().child(title));
    if zoomable {
        header = header.child(
            Icon::new(IconName::ChevronRight)
                .size(px(12.))
                .text_color(theme.muted_foreground),
        );
    }
    let mut body = div()
        .flex()
        .flex_col()
        .gap_1()
        .min_w(px(150.))
        .text_xs()
        .child(header);
    for (k, line) in chart.lines.iter().enumerate() {
        if !enabled.get(k).copied().unwrap_or(true) {
            continue;
        }
        let color: Hsla = rgb(line.color()).into();
        let mut name = div()
            .flex()
            .items_center()
            .gap_1p5()
            .min_w_0()
            // A short stroke of the line's color keys the row.
            .child(
                div()
                    .flex_none()
                    .w(px(10.))
                    .h(px(2.))
                    .rounded_full()
                    .bg(color),
            );
        if let Some(share) = shares.as_ref().and_then(|s| s.get(k)) {
            name = name.child(div().font_semibold().child(share.clone()));
        }
        name = name.child(
            div()
                .truncate()
                .text_color(theme.muted_foreground)
                .child(line.name.clone()),
        );
        body = body.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_4()
                .child(name)
                .child(
                    div()
                        .flex_none()
                        .font_semibold()
                        .child(format_decimal(raw.get(k).copied().unwrap_or(0))),
                ),
        );
    }
    body
}
