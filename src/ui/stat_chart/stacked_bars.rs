//! Stacked columns (tdesktop `StackBarChartView`) as a kit plot.
//!
//! gpui-kit's `BarChart` draws one series and fits its own value domain,
//! so several of them cannot be layered into a stack. This plot is built
//! from the same kit primitives `BarChart` uses (`ScaleBand`,
//! `ScaleLinear`, the `Bar` shape with a `base`, `Grid`, `PlotAxis`,
//! `PlotLabel` and the kit `Tooltip`) and matches its look: dashed grid,
//! value labels in a gutter left of the plot, a highlight band on the
//! hovered column.

use super::series::Point;
use gpui_kit::component::plot::label::{PlotLabel, TEXT_GAP, TEXT_SIZE, Text, measure_text_width};
use gpui_kit::component::plot::scale::{Scale, ScaleBand, ScaleLinear};
use gpui_kit::component::plot::shape::Bar;
use gpui_kit::component::plot::tooltip::{CrossLine, Tooltip};
use gpui_kit::component::plot::{
    AxisText, Grid, IntoPlot, Plot, PlotAxis, PlotHover, TooltipState, axis_gutter,
};
use gpui_kit::component::*;
use gpui_kit::*;
use std::rc::Rc;

/// Space between columns, as a share of a column's slot (the kit
/// `BarChart` default for a revenue chart's columns).
const PADDING_INNER: f32 = 0.2;
const PADDING_OUTER: f32 = 0.1;

type TooltipBody = Rc<dyn Fn(&Point, &mut Window, &mut App) -> AnyElement>;
type TickFormat = Rc<dyn Fn(f64) -> String>;

#[derive(IntoPlot)]
pub(super) struct StackedBars {
    id: ElementId,
    data: Vec<Point>,
    /// The shown lines, bottom first, with their colors.
    lines: Vec<(usize, Hsla)>,
    domain: (f64, f64),
    value_ticks: usize,
    tick_format: TickFormat,
    /// Every how many columns a date is labeled.
    label_every: usize,
    tooltip: Option<TooltipBody>,
    /// Where the hover band has glided to, and how far it has faded in.
    hover: Option<(f32, f32)>,
    /// Width of the value labels' gutter, measured in `prepaint`.
    gutter: f32,
}

impl StackedBars {
    pub(super) fn new(id: impl Into<ElementId>, data: Vec<Point>) -> Self {
        Self {
            id: id.into(),
            data,
            lines: Vec::new(),
            domain: (0.0, 1.0),
            value_ticks: 6,
            tick_format: Rc::new(|v| format!("{v}")),
            label_every: 1,
            tooltip: None,
            hover: None,
            gutter: 0.,
        }
    }

    pub(super) fn lines(mut self, lines: Vec<(usize, Hsla)>) -> Self {
        self.lines = lines;
        self
    }

    /// The value axis from `min` to `max` with `ticks` labeled rulers.
    pub(super) fn domain(mut self, min: f64, max: f64, ticks: usize) -> Self {
        self.domain = (min, max);
        self.value_ticks = ticks.max(2);
        self
    }

    pub(super) fn tick_format(mut self, format: impl Fn(f64) -> String + 'static) -> Self {
        self.tick_format = Rc::new(format);
        self
    }

    pub(super) fn label_every(mut self, every: usize) -> Self {
        self.label_every = every.max(1);
        self
    }

    pub(super) fn tooltip(
        mut self,
        body: impl Fn(&Point, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.tooltip = Some(Rc::new(body));
        self
    }

    /// The height the columns use, above the date labels.
    fn plot_height(bounds: Bounds<Pixels>) -> f32 {
        (bounds.size.height.as_f32() - axis_gutter(px(TEXT_SIZE))).max(0.)
    }

    /// Each ruler's offset from the top and its caption, top first.
    fn rulers(&self, height: f32) -> Vec<(f32, String)> {
        let steps = self.value_ticks.saturating_sub(1).max(1);
        let (low, high) = self.domain;
        (0..=steps)
            .map(|i| {
                let share = i as f64 / steps as f64;
                let y = height * share as f32;
                (y, (self.tick_format)(high - (high - low) * share))
            })
            .collect()
    }

    fn band_scale(&self, bounds: Bounds<Pixels>) -> ScaleBand<usize> {
        ScaleBand::new(
            self.data.iter().map(|p| p.index),
            [self.gutter, bounds.size.width.as_f32()],
        )
        .padding_inner(PADDING_INNER)
        .padding_outer(PADDING_OUTER)
    }

    fn value_scale(&self, height: f32) -> ScaleLinear<f64> {
        ScaleLinear::new([self.domain.0, self.domain.1], [height, 0.])
    }
}

impl Plot for StackedBars {
    fn prepaint(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        _cx: &mut App,
    ) -> Vec<AnyElement> {
        let widest = self
            .rulers(Self::plot_height(bounds))
            .into_iter()
            .map(|(_, text)| measure_text_width(&text.into(), px(TEXT_SIZE), window))
            .fold(0., f32::max);
        self.gutter = widest + TEXT_GAP * 4.;
        Vec::new()
    }

    fn paint(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let height = Self::plot_height(bounds);
        let band = self.band_scale(bounds);
        let width = band.band_width();
        let value = self.value_scale(height);
        let (grid, border, muted) = {
            let theme = cx.theme();
            (theme.chart_grid, theme.border, theme.muted_foreground)
        };

        // The plot right of the label gutter.
        let plot = Bounds {
            origin: bounds.origin + point(px(self.gutter), px(0.)),
            size: size(bounds.size.width - px(self.gutter), bounds.size.height),
        };
        let rulers = self.rulers(height);
        // Rulers at even steps, the baseline left to the axis line.
        let rows: Vec<f32> = rulers
            .iter()
            .map(|(y, _)| *y)
            .filter(|y| *y < height)
            .collect();
        Grid::new()
            .y(rows)
            .stroke(grid)
            .dash_array(&[px(4.), px(2.)])
            .paint(&plot, window);

        let labels = self
            .data
            .iter()
            .enumerate()
            .filter(|(i, _)| i % self.label_every == 0)
            .filter_map(|(_, p)| {
                let x = band.tick(&p.index)? + width / 2. - self.gutter;
                Some(AxisText::new(p.label.clone(), px(x), muted).align(TextAlign::Center))
            });
        PlotAxis::new()
            .stroke(border)
            .x(height)
            .x_label(labels)
            .paint(&plot, window, cx);

        // Each line's band, bottom first; the top band gets the rounded
        // corners. The hovered column stays full, the others fade a little.
        let top_line = self.lines.last().map(|(k, _)| *k);
        let hovered = self.hover;
        for (k, color) in self.lines.iter().copied() {
            let base_scale = value.clone();
            let top_scale = value.clone();
            let cross_band = band.clone();
            let corners = if Some(k) == top_line {
                Corners {
                    top_left: px(2.),
                    top_right: px(2.),
                    bottom_left: px(0.),
                    bottom_right: px(0.),
                }
            } else {
                Corners::all(px(0.))
            };
            Bar::new()
                .data(self.data.iter())
                .band_width(width)
                .cross(move |p: &&Point| cross_band.tick(&p.index))
                .base(move |p: &&Point| base_scale.tick(&p.stack_base(k)).unwrap_or(height))
                .value(move |p: &&Point| top_scale.tick(&p.stack_top(k)))
                .fill(move |_: &&Point, frame: Bounds<f32>, _| {
                    let faded = hovered.is_some_and(|(center, focus)| {
                        let middle = frame.origin.x + frame.size.width / 2.;
                        focus > 0. && (middle - center).abs() > frame.size.width / 2.
                    });
                    if faded { color.opacity(0.6) } else { color }
                })
                .corner_radii(corners)
                .paint(&bounds, window, cx);
        }

        // Ruler captions in the gutter, right-aligned against the plot and
        // centered on their lines, as the kit charts place them.
        let texts = rulers
            .into_iter()
            .map(|(y, text)| {
                let top = (y - TEXT_SIZE / 2.).clamp(0., (height - TEXT_SIZE).max(0.));
                Text::new(text, point(self.gutter - TEXT_GAP * 2., top), muted)
                    .align(TextAlign::Right)
            })
            .collect();
        PlotLabel::new(texts).paint(&bounds, window, cx);
    }

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn tooltip_state(
        &self,
        position: gpui_kit::Point<Pixels>,
        bounds: Bounds<Pixels>,
        _cx: &App,
    ) -> Option<TooltipState> {
        if position.y.as_f32() > Self::plot_height(bounds) || position.x.as_f32() < self.gutter {
            return None;
        }
        let band = self.band_scale(bounds);
        let index = band.nearest_index(position.x.as_f32());
        let p = self.data.get(index)?;
        let center = band.tick(&p.index)? + band.band_width() / 2.;
        Some(TooltipState::new(
            index,
            point(px(center), position.y),
            vec![],
        ))
    }

    fn hover(&mut self, hover: Option<&PlotHover>, window: &mut Window, cx: &mut App) {
        self.hover = hover.map(|hover| {
            let target = hover.state().cross_line.x;
            let center = hover.glide(("stacked-bars", "band"), target, window, cx);
            (center.as_f32(), hover.progress())
        });
    }

    fn tooltip(
        &self,
        state: &TooltipState,
        cursor: gpui_kit::Point<Pixels>,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        let body = self.tooltip.as_ref()?;
        let p = self.data.get(state.index)?;
        let width = self.band_scale(bounds).band_width();
        let center = self
            .hover
            .map_or(state.cross_line.x, |(center, _)| px(center));
        let cross_line = CrossLine::new(gpui_kit::point(center, state.cross_line.y))
            .span(0., Self::plot_height(bounds))
            .band(px(width));
        Some(
            Tooltip::new(cursor, bounds.size)
                .glide(false)
                .gap(px(8.))
                .cross_line(cross_line)
                .child(body(p, window, cx))
                .into_any_element(),
        )
    }
}
