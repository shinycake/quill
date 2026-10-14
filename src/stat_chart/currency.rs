//! Revenue graphs: a graph whose `yTickFormatter` names TON or Stars
//! (tdesktop `statistics_data_deserialize.cpp`) counts in that currency,
//! and the revenue screens show its values in the currency and in US
//! dollars (`chart_rulers_view.cpp`, `point_details_widget.cpp`).

use super::{StatChart, format_decimal, format_short};
use serde_json::Value;

/// Nanotons per TON: TON revenue graphs count nanotons.
const NANOS: i64 = 1_000_000_000;

/// The currency a revenue graph counts in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ChartCurrency {
    #[default]
    None,
    /// Nanotons.
    Ton,
    /// Whole Stars (`XTR`).
    Stars,
}

/// The currency named by the graph's `yTickFormatter`.
pub(super) fn currency_of(root: &serde_json::Map<String, Value>) -> ChartCurrency {
    match root.get("yTickFormatter").and_then(Value::as_str) {
        Some(format) if format.contains("TON") => ChartCurrency::Ton,
        Some(format) if format.contains("XTR") => ChartCurrency::Stars,
        _ => ChartCurrency::None,
    }
}

/// A value-axis label in a narrow chart: three significant digits,
/// grouped (`1,500`, `4.2`, `0.105`), short from 10,000 up (`45K`).
pub fn axis_caption(value: f64) -> String {
    if !value.is_finite() || value.abs() <= 1e-9 {
        return "0".into();
    }
    if value.abs() >= 10_000.0 {
        return format_short(value.round() as i64);
    }
    let magnitude = value.abs().log10().floor() as i32;
    let decimals = (2 - magnitude).clamp(0, 9) as usize;
    // Halves round away from zero (1,498.5 reads 1,499), not to even.
    let scale = 10f64.powi(decimals as i32);
    let rounded = (value * scale).round() / scale;
    let text = format!("{rounded:.decimals$}");
    let text = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        text
    };
    let (whole, fraction) = text.split_once('.').unwrap_or((text.as_str(), ""));
    let grouped = whole
        .parse::<i64>()
        .map(format_decimal)
        .unwrap_or_else(|_| whole.to_string());
    let grouped = if whole == "-0" {
        "-0".to_string()
    } else {
        grouped
    };
    if fraction.is_empty() {
        grouped
    } else {
        format!("{grouped}.{fraction}")
    }
}

/// The top of a value axis split into `intervals` even steps that each
/// read as a round number (1, 1.5, 2, 2.5, 3, 4, 5, 6 or 8 times a power
/// of ten) and
/// together reach `max`.
pub fn nice_axis_max(max: f64, intervals: u32) -> f64 {
    if !max.is_finite() || max <= 0.0 || intervals == 0 {
        return max.max(0.0);
    }
    let raw = max / f64::from(intervals);
    let power = 10f64.powf(raw.log10().floor());
    let step = [1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0]
        .into_iter()
        .map(|m| m * power)
        .find(|step| *step >= raw)
        .unwrap_or(10.0 * power);
    step * f64::from(intervals)
}

/// `≈$1,234.56` for `units` of the currency at `cents_per_unit`.
pub fn usd_label(units: f64, cents_per_unit: f64) -> String {
    let usd = units * cents_per_unit / 100.0;
    let text = format!("{:.2}", usd.abs());
    let (whole, fraction) = text.split_once('.').unwrap_or((text.as_str(), "00"));
    let whole = whole
        .parse::<i64>()
        .map(format_decimal)
        .unwrap_or_else(|_| whole.to_string());
    format!("\u{2248}${whole}.{fraction}")
}

impl StatChart {
    /// Whether the value axis steps in whole numbers: everything but TON
    /// amounts, which are often fractions of one.
    pub fn whole_values(&self) -> bool {
        self.currency != ChartCurrency::Ton
    }

    /// A raw y value in the currency's units (TON for nanotons).
    pub fn currency_units(&self, value: f64) -> f64 {
        match self.currency {
            ChartCurrency::Ton => value / NANOS as f64,
            _ => value,
        }
    }

    /// A raw y value written out in full in the currency: `1.5` for
    /// 1,500,000,000 nanotons, `1,998` for Stars (tdesktop's tooltip).
    pub fn currency_amount(&self, raw: i64) -> String {
        match self.currency {
            ChartCurrency::Ton => {
                let whole = format_decimal(raw / NANOS);
                let fraction = format!("{:09}", (raw % NANOS).unsigned_abs());
                let fraction = fraction.trim_end_matches('0');
                let sign = if raw < 0 && raw > -NANOS { "-" } else { "" };
                if fraction.is_empty() {
                    format!("{sign}{whole}")
                } else {
                    format!("{sign}{whole}.{fraction}")
                }
            }
            _ => format_decimal(raw),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ChartCurrency, axis_caption, nice_axis_max, usd_label};
    use crate::stat_chart::parse_chart;

    fn chart(formatter: &str) -> crate::stat_chart::StatChart {
        let json = format!(
            r##"{{"columns":[["x",1,2],["y0",1500000000,0]],"types":{{"x":"x","y0":"bar"}},
            "names":{{"y0":"Rewards"}},"colors":{{"y0":"#000000"}},"yTickFormatter":"{formatter}"}}"##
        );
        match parse_chart(&json) {
            Ok(chart) => chart,
            Err(error) => panic!("fixture parses: {error:?}"),
        }
    }

    #[test]
    fn the_formatter_names_the_currency() {
        assert_eq!(chart("TON").currency, ChartCurrency::Ton);
        assert_eq!(chart("XTR").currency, ChartCurrency::Stars);
        assert_eq!(chart("").currency, ChartCurrency::None);
    }

    #[test]
    fn ton_amounts_read_in_whole_ton() {
        let ton = chart("TON");
        assert_eq!(ton.currency_units(1_500_000_000.0), 1.5);
        assert_eq!(ton.currency_amount(1_500_000_000), "1.5");
        assert_eq!(ton.currency_amount(2_000_000_000), "2");
        assert_eq!(ton.currency_amount(-5), "-0.000000005");
        assert_eq!(chart("XTR").currency_amount(1998), "1,998");
    }

    #[test]
    fn dollars_follow_tdesktop() {
        assert_eq!(usd_label(1.5, 300.0), "\u{2248}$4.50");
        assert_eq!(usd_label(1_000.0, 130.0), "\u{2248}$1,300.00");
    }

    #[test]
    fn axis_captions_stay_short() {
        assert_eq!(axis_caption(0.0), "0");
        assert_eq!(axis_caption(4.19699), "4.2");
        assert_eq!(axis_caption(1.049247), "1.05");
        assert_eq!(axis_caption(0.10512), "0.105");
        assert_eq!(axis_caption(1498.5), "1,499");
        assert_eq!(axis_caption(45_150.0), "45.1K");
    }

    #[test]
    fn axis_tops_are_round() {
        assert_eq!(nice_axis_max(56_900.0, 4), 60_000.0);
        assert_eq!(nice_axis_max(4.19699, 4), 6.0);
        assert_eq!(nice_axis_max(31_000.0, 4), 32_000.0);
        assert_eq!(nice_axis_max(1998.0, 4), 2000.0);
        assert_eq!(nice_axis_max(0.0, 4), 0.0);
    }
}
