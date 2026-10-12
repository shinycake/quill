//! The tooltip over a chart's hovered point: the date and each shown
//! line's value, with its share on a percentage chart. On a zoomable
//! chart it carries a chevron and a click opens that day
//! (`point_details_widget.cpp`).

use super::paint::plot_rect_top;
use gpui_kit::assets::IconName;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::stat_chart::{ChartKind, StatChart, format_decimal, percentages};
use std::rc::Rc;

#[allow(clippy::too_many_arguments)]
pub(super) fn details(
    id: &str,
    chart: &Rc<StatChart>,
    kind: ChartKind,
    index: usize,
    enabled: &[bool],
    // `Ok`: left edge from the plot's left; `Err`: right edge from its
    // right.
    place: Result<Pixels, Pixels>,
    on_click: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    opacity: f32,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let offset = quill::local_time::utc_offset_at(quill::local_time::now_unix());
    let values: Vec<i64> = chart.lines.iter().map(|l| l.y[index]).collect();
    let shares = (chart.has_percentages || kind == ChartKind::StackLinear)
        .then(|| percentages(&values, enabled));
    // Zooming needs something to zoom into (tdesktop: a positive value).
    let zoomable = on_click.is_some() && values.iter().any(|v| *v > 0);
    let title = chart.point_title(index, offset);
    let mut header = div()
        .flex()
        .items_center()
        .justify_between()
        .gap_3()
        .child(div().font_semibold().child(title.clone()));
    if zoomable {
        header = header.child(
            Icon::new(IconName::ChevronRight)
                .size(px(12.))
                .text_color(theme.muted_foreground),
        );
    }
    let mut body = div()
        .id(ElementId::Name(format!("{id}-details").into()))
        .debug_selector(|| format!("{id}-details"))
        .absolute()
        .top(px(plot_rect_top()))
        .map(|d| match place {
            Ok(left) => d.left(left),
            Err(right) => d.right(right),
        })
        .occlude()
        .flex()
        .flex_col()
        .gap_1()
        .min_w(px(140.))
        .px_3()
        .py_2()
        .rounded_md()
        .border_1()
        .border_color(theme.border)
        .bg(theme.popover)
        .text_color(theme.popover_foreground)
        .shadow_md()
        .text_xs()
        .child(header);
    for (k, line) in chart.lines.iter().enumerate() {
        if !enabled.get(k).copied().unwrap_or(true) {
            continue;
        }
        let color: Hsla = rgb(line.color()).into();
        let mut name = div().flex().gap_1().min_w_0();
        if let Some(shares) = shares.as_ref() {
            name = name.child(div().font_semibold().child(shares[k].clone()));
        }
        name = name.child(div().truncate().child(line.name.clone()));
        body = body.child(
            div().flex().justify_between().gap_3().child(name).child(
                div()
                    .font_semibold()
                    .text_color(color)
                    .child(format_decimal(line.y[index])),
            ),
        );
    }
    if zoomable && let Some(on_click) = on_click {
        body = body
            .role(gpui_kit::Role::Button)
            .aria_label(SharedString::from(format!("Zoom into {title}")))
            .hover(|style| style.bg(theme.accent))
            .on_click(move |_, window, cx| on_click(window, cx));
    }
    body.opacity(opacity).into_any_element()
}

/// The total of the hovered pie slice over the selected days, at the
/// plot's top right (tdesktop `PaintDetails`).
pub(super) fn pie_details(id: &str, name: &str, value: i64, color: u32, cx: &App) -> AnyElement {
    let theme = cx.theme();
    div()
        .id(ElementId::Name(format!("{id}-pie-details").into()))
        .debug_selector(|| format!("{id}-pie-details"))
        .absolute()
        .top(px(plot_rect_top()))
        .right(px(4.))
        .flex()
        .gap_3()
        .px_3()
        .py_2()
        .rounded_md()
        .border_1()
        .border_color(theme.border)
        .bg(theme.popover)
        .text_color(theme.popover_foreground)
        .shadow_md()
        .text_xs()
        .child(div().child(name.to_string()))
        .child(
            div()
                .font_semibold()
                .text_color(rgb(color))
                .child(format_decimal(value)),
        )
        .into_any_element()
}
