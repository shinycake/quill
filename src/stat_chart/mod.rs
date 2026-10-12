//! Statistics charts: the graph JSON TDLib hands over in
//! `statisticalGraphData.json_data`, and the range, axis and label math a
//! chart view needs. Ported from Telegram Desktop's `statistics/`
//! (`statistics_data_deserialize.cpp`, `data_statistics_chart.cpp`,
//! `chart_rulers_data.cpp`, `view/*`); the drawing itself lives in the UI.

mod axis;
mod format;
mod pie;
mod range;
mod slots;
pub(crate) mod xlsx;

pub use axis::*;
pub use format::*;
pub use pie::*;
pub use range::*;
pub use slots::*;
pub use xlsx::*;

#[cfg(test)]
mod export_tests;
#[cfg(test)]
mod tests;

use serde_json::Value;

/// How a graph is drawn. Telegram Desktop picks it per graph slot
/// (`info_statistics_inner_widget.cpp`), not from the JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChartKind {
    /// Lines on one scale.
    Linear,
    /// One step line per series (`BarChartView(false)`).
    Bar,
    /// Stacked bars.
    StackBar,
    /// Two lines, each on its own scale (left and right captions).
    DoubleLinear,
    /// Stacked areas summing to 100%.
    StackLinear,
}

/// The named chart colors (`statisticsChartLine*`); the JSON prefixes a
/// color with one of these keys (`"BLUE#3497ED"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorKey {
    Blue,
    Green,
    Red,
    Golden,
    LightBlue,
    LightGreen,
    Orange,
    Indigo,
    Purple,
    Cyan,
}

impl ColorKey {
    fn parse(key: &str) -> Option<Self> {
        Some(match key {
            "BLUE" => Self::Blue,
            "GREEN" => Self::Green,
            "RED" => Self::Red,
            "GOLDEN" => Self::Golden,
            "LIGHTBLUE" => Self::LightBlue,
            "LIGHTGREEN" => Self::LightGreen,
            "ORANGE" => Self::Orange,
            "INDIGO" => Self::Indigo,
            "PURPLE" => Self::Purple,
            "CYAN" => Self::Cyan,
            _ => return None,
        })
    }

    /// The palette color (`lib_ui/ui/colors.palette`), `0xRRGGBB`. The
    /// night themes keep the same values.
    pub fn rgb(self) -> u32 {
        match self {
            Self::Blue => 0x327fe5,
            Self::Green => 0x61c752,
            Self::Red => 0xe05356,
            Self::Golden => 0xeba52d,
            Self::LightBlue => 0x58a8ed,
            Self::LightGreen => 0x8fcf39,
            Self::Orange => 0xf28c39,
            Self::Indigo => 0x7f79f3,
            Self::Purple => 0x9f79e8,
            Self::Cyan => 0x40d0ca,
        }
    }
}

/// One series of a chart.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartLine {
    /// Column id in the JSON (`"y0"`).
    pub key: String,
    /// Legend and tooltip name; `-` becomes an em dash, as in tdesktop.
    pub name: String,
    pub color_key: Option<ColorKey>,
    /// The JSON's own color, `0xRRGGBB`.
    pub rgb: Option<u32>,
    pub y: Vec<i64>,
    pub min: i64,
    pub max: i64,
    /// Listed in the JSON's `hidden`: starts switched off in the legend.
    pub hidden_on_start: bool,
}

impl ChartLine {
    /// The color to draw with: the palette color for a known key, else
    /// the JSON's, else blue.
    pub fn color(&self) -> u32 {
        self.color_key
            .map(ColorKey::rgb)
            .or(self.rgb)
            .unwrap_or(ColorKey::Blue.rgb())
    }
}

/// A parsed graph (tdesktop `Data::StatisticalChart`).
#[derive(Debug, Clone, PartialEq)]
pub struct StatChart {
    /// X values in milliseconds.
    pub x: Vec<f64>,
    /// Each x as a fraction of the full span (0..=1).
    pub x_frac: Vec<f64>,
    pub lines: Vec<ChartLine>,
    /// Milliseconds between the first two points (a day when there is
    /// only one).
    pub time_step: f64,
    /// `subchart.show == false`: no range selector under the chart.
    pub footer_hidden: bool,
    /// The range shown first (`subchart.defaultZoom`), as x indices.
    pub default_zoom: (usize, usize),
    /// `percentage == true`: the tooltip adds each part's share.
    pub has_percentages: bool,
    /// The tooltip names a week, not a day (`xTooltipFormatter` with
    /// `'week'`).
    pub week_format: bool,
    pub min: i64,
    pub max: i64,
}

/// Why a graph's JSON could not be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartError {
    BadJson,
    NoColumns,
    EmptyColumn,
    /// A series has a different length than the x column.
    Mismatched,
}

/// One day in milliseconds.
pub const ONE_DAY_MS: f64 = 86_400_000.0;

/// Parses `statisticalGraphData.json_data` the way tdesktop's
/// `StatisticalChartFromJSON` does.
pub fn parse_chart(json: &str) -> Result<StatChart, ChartError> {
    let root: Value = serde_json::from_str(json).map_err(|_| ChartError::BadJson)?;
    let root = root.as_object().ok_or(ChartError::BadJson)?;
    let columns = root
        .get("columns")
        .and_then(Value::as_array)
        .filter(|columns| !columns.is_empty())
        .ok_or(ChartError::NoColumns)?;
    let hidden: Vec<&str> = root
        .get("hidden")
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let mut x = Vec::new();
    let mut lines = Vec::new();
    for column in columns {
        let column = column
            .as_array()
            .filter(|c| !c.is_empty())
            .ok_or(ChartError::EmptyColumn)?;
        let id = column[0].as_str().unwrap_or_default();
        let values = column[1..].iter().map(|v| v.as_f64().unwrap_or(0.0));
        if id == "x" {
            x = values.collect();
        } else {
            let y: Vec<i64> = values.map(|v| v.round() as i64).collect();
            lines.push(ChartLine {
                key: id.to_string(),
                name: String::new(),
                color_key: None,
                rgb: None,
                min: y.iter().copied().min().unwrap_or(i64::MAX),
                max: y.iter().copied().max().unwrap_or(0).max(0),
                y,
                hidden_on_start: hidden.contains(&id),
            });
        }
    }
    if x.is_empty() || lines.iter().any(|line| line.y.len() != x.len()) {
        return Err(ChartError::Mismatched);
    }
    let colors = root.get("colors").and_then(Value::as_object);
    let names = root.get("names").and_then(Value::as_object);
    for line in &mut lines {
        if let Some(color) = colors
            .and_then(|c| c.get(&line.key))
            .and_then(Value::as_str)
            && let Some(hash) = color.find('#')
        {
            line.color_key = ColorKey::parse(&color[..hash]);
            line.rgb = u32::from_str_radix(color[hash + 1..].get(..6).unwrap_or(""), 16).ok();
        }
        if let Some(name) = names.and_then(|n| n.get(&line.key)).and_then(Value::as_str) {
            line.name = name.replace('-', "\u{2014}");
        }
    }
    let mut min = lines.iter().map(|l| l.min).min().unwrap_or(i64::MAX);
    let mut max = lines.iter().map(|l| l.max).max().unwrap_or(0);
    if min == max {
        if min != 0 {
            min = 0;
        } else {
            max = 1;
        }
    }
    let subchart = root.get("subchart").and_then(Value::as_object);
    let footer_hidden = subchart
        .and_then(|s| s.get("show"))
        .and_then(Value::as_bool)
        .is_some_and(|show| !show);
    let default_zoom = default_zoom(&x, subchart.and_then(|s| s.get("defaultZoom")));
    let time_step = if x.len() > 1 {
        (x[1] - x[0]).max(1.0)
    } else {
        ONE_DAY_MS
    };
    Ok(StatChart {
        x_frac: x_fractions(&x),
        x,
        lines,
        time_step,
        footer_hidden,
        default_zoom,
        has_percentages: root
            .get("percentage")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        week_format: root
            .get("xTooltipFormatter")
            .and_then(Value::as_str)
            .is_some_and(|f| f.contains("'week'")),
        min,
        max,
    })
}

fn x_fractions(x: &[f64]) -> Vec<f64> {
    match x {
        [] => Vec::new(),
        [_] => vec![1.0],
        [first, .., last] => {
            let span = last - first;
            x.iter()
                .map(|v| if span > 0.0 { (v - first) / span } else { 0.0 })
                .collect()
        }
    }
}

/// `subchart.defaultZoom` (two x values) as indices; the whole chart when
/// absent or unmatched.
fn default_zoom(x: &[f64], zoom: Option<&Value>) -> (usize, usize) {
    let last = x.len().saturating_sub(1);
    let (mut min, mut max) = (0, last);
    if let Some(zoom) = zoom.and_then(Value::as_array).filter(|z| !z.is_empty()) {
        let first = zoom.first().and_then(Value::as_f64);
        let end = zoom.last().and_then(Value::as_f64);
        for (i, value) in x.iter().enumerate() {
            if Some(*value) == first {
                min = i;
            }
            if Some(*value) == end {
                max = i;
            }
        }
    }
    (min.min(max), min.max(max))
}

impl StatChart {
    /// Milliseconds between the first and last point.
    pub fn span(&self) -> f64 {
        match self.x.as_slice() {
            [first, .., last] => last - first,
            _ => 0.0,
        }
    }

    /// One time step as a fraction of the full span.
    pub fn step_fraction(&self) -> f64 {
        let span = self.span();
        if span > 0.0 {
            self.time_step / span
        } else {
            1.0
        }
    }

    /// The default range as fractions of the span.
    pub fn default_range(&self) -> Limits {
        let (min, max) = self.default_zoom;
        if self.x_frac.len() < 2 {
            return Limits::new(0.0, 1.0);
        }
        Limits::new(self.x_frac[min], self.x_frac[max])
    }
}
