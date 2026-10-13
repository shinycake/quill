use super::*;
use serde_json::json;

const DAY: f64 = ONE_DAY_MS;
/// 2024-10-01 00:00 UTC in milliseconds.
const OCT_1: f64 = 1_727_740_800_000.0;

fn days(n: usize) -> Vec<f64> {
    (0..n).map(|i| OCT_1 + i as f64 * DAY).collect()
}

fn chart_json(
    x: &[f64],
    lines: &[(&str, &str, &str, Vec<i64>)],
    extra: serde_json::Value,
) -> String {
    let mut columns = vec![{
        let mut c = vec![json!("x")];
        c.extend(x.iter().map(|v| json!(v)));
        c
    }];
    let mut types = serde_json::Map::new();
    let mut names = serde_json::Map::new();
    let mut colors = serde_json::Map::new();
    types.insert("x".into(), json!("x"));
    for (id, name, color, y) in lines {
        let mut c = vec![json!(id)];
        c.extend(y.iter().map(|v| json!(v)));
        columns.push(c);
        types.insert((*id).into(), json!("line"));
        names.insert((*id).into(), json!(name));
        colors.insert((*id).into(), json!(color));
    }
    let mut root = json!({"columns": columns, "types": types, "names": names, "colors": colors});
    if let Some(extra) = extra.as_object() {
        for (k, v) in extra {
            root[k] = v.clone();
        }
    }
    root.to_string()
}

fn linear() -> StatChart {
    parse_chart(&chart_json(
        &days(10),
        &[(
            "y0",
            "Followers",
            "BLUE#3497ED",
            vec![10, 12, 15, 11, 20, 25, 22, 30, 28, 35],
        )],
        json!({}),
    ))
    .expect("parses")
}

#[test]
fn parses_columns_names_colors_and_hidden_lines() {
    let chart = parse_chart(&chart_json(
        &days(3),
        &[
            ("y0", "Views", "GREEN#61c752", vec![1, 2, 3]),
            ("y1", "Shares-Out", "#ff8800", vec![4, 5, 6]),
        ],
        json!({"hidden": ["y1"], "percentage": true,
               "xTooltipFormatter": "function(x){return 'week' + x}"}),
    ))
    .expect("parses");
    assert_eq!(chart.x.len(), 3);
    assert_eq!(chart.x_frac, vec![0.0, 0.5, 1.0]);
    assert_eq!(chart.time_step, DAY);
    assert_eq!(chart.lines[0].color_key, Some(ColorKey::Green));
    assert_eq!(chart.lines[0].color(), ColorKey::Green.rgb());
    assert_eq!(chart.lines[1].color_key, None);
    assert_eq!(chart.lines[1].color(), 0xff8800);
    assert_eq!(chart.lines[1].name, "Shares\u{2014}Out");
    assert!(!chart.lines[0].hidden_on_start);
    assert!(chart.lines[1].hidden_on_start);
    assert!(chart.has_percentages);
    assert!(chart.week_format);
    assert_eq!((chart.min, chart.max), (1, 6));
    assert_eq!(chart.default_zoom, (0, 2));
}

#[test]
fn every_graph_shape_parses() {
    // Line, step, bar, stacked bar, stacked percentage area: the JSON
    // shape is the same, only `types` / flags differ.
    for (types, flags) in [
        ("line", json!({})),
        ("step", json!({})),
        ("bar", json!({"stacked": true})),
        ("area", json!({"stacked": true, "percentage": true})),
        ("line", json!({"y_scaled": true})),
    ] {
        let mut root: serde_json::Value = serde_json::from_str(&chart_json(
            &days(4),
            &[
                ("y0", "A", "RED#e05356", vec![1, 2, 3, 4]),
                ("y1", "B", "CYAN#40d0ca", vec![4, 3, 2, 1]),
            ],
            flags,
        ))
        .unwrap();
        root["types"]["y0"] = json!(types);
        root["types"]["y1"] = json!(types);
        let chart = parse_chart(&root.to_string()).expect(types);
        assert_eq!(chart.lines.len(), 2);
        assert_eq!(chart.lines[1].y, vec![4, 3, 2, 1]);
    }
}

#[test]
fn bad_json_is_an_error() {
    assert_eq!(parse_chart("not json"), Err(ChartError::BadJson));
    assert_eq!(parse_chart("[]"), Err(ChartError::BadJson));
    assert_eq!(parse_chart("{}"), Err(ChartError::NoColumns));
    assert_eq!(
        parse_chart(r#"{"columns":[[]]}"#),
        Err(ChartError::EmptyColumn)
    );
    assert_eq!(
        parse_chart(r#"{"columns":[["x",1,2],["y0",1]]}"#),
        Err(ChartError::Mismatched)
    );
    assert_eq!(
        parse_chart(r#"{"columns":[["y0",1]]}"#),
        Err(ChartError::Mismatched)
    );
}

#[test]
fn flat_series_still_has_a_span() {
    let chart = parse_chart(r#"{"columns":[["x",1,2],["y0",5,5]]}"#).unwrap();
    assert_eq!((chart.min, chart.max), (0, 5));
    let chart = parse_chart(r#"{"columns":[["x",1,2],["y0",0,0]]}"#).unwrap();
    assert_eq!((chart.min, chart.max), (0, 1));
}

#[test]
fn default_zoom_and_hidden_footer_come_from_subchart() {
    let x = days(10);
    let chart = parse_chart(&chart_json(
        &x,
        &[("y0", "A", "#000000", vec![1; 10])],
        json!({"subchart": {"show": false, "defaultZoom": [x[7], x[3]]}}),
    ))
    .unwrap();
    assert!(chart.footer_hidden);
    assert_eq!(chart.default_zoom, (3, 7));
    let range = chart.default_range();
    assert!((range.min - 3.0 / 9.0).abs() < 1e-9);
    assert!((range.max - 7.0 / 9.0).abs() < 1e-9);
}

#[test]
fn visible_indices_follow_the_range() {
    let chart = linear();
    assert_eq!(chart.visible_indices(Limits::new(0.0, 1.0)), (0, 9));
    // 0.25 of 9 steps = 2.25 → first point at index 3; 0.6 → 5.4 → 5.
    assert_eq!(chart.visible_indices(Limits::new(0.25, 0.6)), (3, 5));
    // A range between two points still names one.
    assert_eq!(chart.visible_indices(Limits::new(0.5, 0.52)), (5, 5));
}

#[test]
fn height_limits_track_enabled_lines_and_kind() {
    let chart = parse_chart(&chart_json(
        &days(4),
        &[
            ("y0", "A", "#000000", vec![10, 40, 20, 30]),
            ("y1", "B", "#000000", vec![1, 2, 3, 4]),
        ],
        json!({}),
    ))
    .unwrap();
    let both = [true, true];
    let limits = height_limits(&chart, ChartKind::Linear, &both, (0, 3));
    assert_eq!(limits.ranged, Limits::new(1.0, 40.0));
    let limits = height_limits(&chart, ChartKind::Linear, &[false, true], (2, 3));
    assert_eq!(limits.ranged, Limits::new(3.0, 4.0));
    // Stacked bars: sums from zero.
    let limits = height_limits(&chart, ChartKind::StackBar, &both, (0, 1));
    assert_eq!(limits.ranged, Limits::new(0.0, 42.0));
    assert_eq!(limits.full, Limits::new(0.0, 42.0));
    // Percentage areas: always 0..100.
    let limits = height_limits(&chart, ChartKind::StackLinear, &both, (0, 3));
    assert_eq!(limits.ranged, Limits::new(0.0, 100.0));
    // Double linear: the small line is stretched to the big one (×10).
    assert_eq!(
        line_ratios(&chart, ChartKind::DoubleLinear),
        vec![1.0, 10.0]
    );
    let limits = height_limits(&chart, ChartKind::DoubleLinear, &[false, true], (0, 3));
    assert_eq!(limits.ranged, Limits::new(10.0, 40.0));
}

/// A start index past the end used to slice `y[start..=end]` backwards and
/// panic.
#[test]
fn height_limits_survive_a_start_past_the_end() {
    let chart = parse_chart(&chart_json(
        &days(4),
        &[("y0", "A", "#000000", vec![10, 40, 20, 30])],
        json!({}),
    ))
    .unwrap();
    for kind in [
        ChartKind::Linear,
        ChartKind::StackBar,
        ChartKind::DoubleLinear,
    ] {
        let limits = height_limits(&chart, kind, &[true], (9, 2));
        assert!(limits.ranged.span() >= 0.0, "{kind:?}");
        let limits = height_limits(&chart, kind, &[true], (9, 99));
        assert!(limits.ranged.span() >= 0.0, "{kind:?}");
    }
}

#[test]
fn nearest_point_and_bar_under_the_pointer() {
    let chart = linear();
    let full = Limits::new(0.0, 1.0);
    assert_eq!(nearest_index(&chart, full, 0.0), Some(0));
    assert_eq!(nearest_index(&chart, full, 1.0), Some(9));
    assert_eq!(nearest_index(&chart, full, 0.5), Some(5));
    assert_eq!(nearest_index(&chart, full, 0.49), Some(4));
    assert_eq!(nearest_index(&chart, full, 1.5), None);
    // In a zoomed range the pointer maps inside it.
    let zoomed = Limits::new(0.5, 1.0);
    assert_eq!(nearest_index(&chart, zoomed, 0.0), Some(5));
    // Bars: ten columns across 900 px.
    let (left, step) = bar_columns(&chart, full, 900.0, 0);
    assert!((step - (900.0 - 100.0) / 9.0).abs() < 1e-6);
    assert_eq!(left, 0.0);
    assert_eq!(bar_index_at(&chart, full, 900.0, 1.0), Some(0));
    assert_eq!(bar_index_at(&chart, full, 900.0, 885.0), Some(9));
    assert_eq!(bar_index_at(&chart, full, 900.0, -1.0), None);
}

#[test]
fn the_last_shown_line_stays_on() {
    let mut enabled = vec![true, true];
    assert!(toggle_line(&mut enabled, 0));
    assert_eq!(enabled, vec![false, true]);
    assert!(!toggle_line(&mut enabled, 1));
    assert_eq!(enabled, vec![false, true]);
    assert!(toggle_line(&mut enabled, 0));
    assert!(!toggle_line(&mut enabled, 5));
}

#[test]
fn range_selector_drags_and_clamps() {
    let mut selector = RangeSelector::new(220.0, Limits::new(0.0, 1.0));
    assert_eq!(selector.handles(), (0.0, 210.0));
    assert_eq!(selector.range(), Limits::new(0.0, 1.0));
    // Left handle to the middle.
    assert_eq!(selector.grab_at(5.0), SelectorGrab::Left);
    selector.drag(SelectorGrab::Left, 100.0, 0.0);
    assert_eq!(selector.range(), Limits::new(0.5, 1.0));
    // The handles never cross: min gap of 5 px.
    selector.drag(SelectorGrab::Right, 0.0, 0.0);
    assert_eq!(selector.handles(), (100.0, 115.0));
    // The window moves as a whole and stops at the edges.
    selector.drag(SelectorGrab::Right, 160.0, 0.0);
    let width = selector.handles().1 - selector.handles().0;
    let grab = selector.grab_at(130.0);
    assert_eq!(grab, SelectorGrab::Middle);
    let offset = selector.grab_offset(grab, 130.0);
    selector.drag(grab, 0.0, offset);
    assert_eq!(selector.handles().0, 0.0);
    assert_eq!(selector.handles().1 - selector.handles().0, width);
    selector.drag(grab, 1000.0, offset);
    assert_eq!(selector.handles().1, 210.0);
    assert_eq!(selector.handles().1 - selector.handles().0, width);
    // A click outside centers the window there.
    let mut selector = RangeSelector::new(220.0, Limits::new(0.0, 0.25));
    assert_eq!(selector.grab_at(150.0), SelectorGrab::Outside);
    selector.center_on(150.0);
    let (left, right) = selector.handles();
    assert!((left + (right + FOOTER_SIDE - left) / 2.0 - 150.0).abs() < 1e-9);
    // Resizing keeps the range.
    let before = selector.range();
    selector.resize(440.0);
    assert!((selector.range().min - before.min).abs() < 1e-9);
    assert!((selector.range().max - before.max).abs() < 1e-9);
}

#[test]
fn rulers_pick_round_steps() {
    let lines = rulers(Limits::new(0.0, 100.0), 0.0);
    let values: Vec<f64> = lines.iter().map(|r| r.value).collect();
    assert_eq!(values, vec![0.0, 20.0, 40.0, 60.0, 80.0, 100.0]);
    // Small spans use unit or double steps.
    let values: Vec<f64> = rulers(Limits::new(3.0, 6.0), 0.0)
        .iter()
        .map(|r| r.value)
        .collect();
    assert_eq!(values, vec![3.0, 4.0, 5.0, 6.0]);
    let values: Vec<f64> = rulers(Limits::new(0.0, 9.0), 0.0)
        .iter()
        .map(|r| r.value)
        .collect();
    assert_eq!(values, vec![0.0, 2.0, 4.0, 6.0, 8.0, 10.0]);
    // A flat range still gets rulers.
    assert_eq!(rulers(Limits::new(5.0, 5.0), 0.0).len(), 3);
    // Big values shorten from 10,000.
    let lines = rulers(Limits::new(0.0, 50_000.0), 0.0);
    assert_eq!(lines[1].caption, "10K");
    let lines = rulers(Limits::new(0.0, 5_000.0), 0.0);
    assert_eq!(lines[1].caption, "1,000");
    // tdesktop shortens from 10,000 even when that repeats a caption.
    let lines = rulers(Limits::new(12_000.0, 12_092.0), 0.0);
    assert_eq!(lines[0].caption, "12K");
}

#[test]
fn double_linear_rulers_caption_both_scales() {
    let chart = parse_chart(&chart_json(
        &days(3),
        &[
            ("y0", "Views", "#000000", vec![100, 500, 1000]),
            ("y1", "Shares", "#000000", vec![1, 5, 10]),
        ],
        json!({}),
    ))
    .unwrap();
    let lines = chart_rulers(&chart, ChartKind::DoubleLinear, Limits::new(0.0, 1000.0));
    assert_eq!(lines[1].caption, "200");
    assert_eq!(lines[1].right_caption, "2");
    // The other way round, the scaled line reads on the left.
    let flipped = parse_chart(&chart_json(
        &days(3),
        &[
            ("y0", "Shares", "#000000", vec![1, 5, 10]),
            ("y1", "Views", "#000000", vec![100, 500, 1000]),
        ],
        json!({}),
    ))
    .unwrap();
    let lines = chart_rulers(&flipped, ChartKind::DoubleLinear, Limits::new(0.0, 1000.0));
    assert_eq!(lines[1].caption, "2");
    assert_eq!(lines[1].right_caption, "200");
}

#[test]
fn x_labels_thin_out_in_powers_of_two() {
    let chart = parse_chart(&chart_json(
        &days(100),
        &[("y0", "A", "#000000", vec![1; 100])],
        json!({}),
    ))
    .unwrap();
    // 100 points over 300 px: ~16 points per label → 16.
    assert_eq!(label_step(&chart, Limits::new(0.0, 1.0), 300.0), 32);
    // Zoomed to a tenth: ~1.6 points per label → 2.
    assert_eq!(label_step(&chart, Limits::new(0.0, 0.1), 300.0), 2);
    let indices = label_indices(&chart, Limits::new(0.0, 0.1), 300.0, 0.0);
    assert!(indices.iter().all(|i| i % 2 == 0));
    assert!(indices.contains(&0) && indices.contains(&10));
    assert!(!indices.contains(&99));
}

#[test]
fn labels_and_titles_read_like_telegram_desktop() {
    let chart = linear();
    assert_eq!(chart.day_label(0, 0), "1 Oct");
    assert_eq!(chart.range_title(0, 9), "1 Oct 2024 \u{2014} 10 Oct 2024");
    assert_eq!(chart.range_title(4, 4), "5 Oct 2024");
    assert_eq!(chart.point_title(4, 0), "Sat, 5 Oct 2024");
    // A western time zone does not move a day label.
    assert_eq!(chart.day_label(0, -5 * 3600), "1 Oct");
    // Hourly charts.
    let hours: Vec<f64> = (0..24).map(|h| OCT_1 + h as f64 * 3_600_000.0).collect();
    let hourly = parse_chart(&chart_json(
        &hours,
        &[("y0", "A", "#0", vec![1; 24])],
        json!({}),
    ))
    .unwrap();
    assert_eq!(hourly.day_label(14, 0), "14:00");
    assert_eq!(hourly.day_label(14, 7200), "16:00");
    assert_eq!(hourly.point_title(14, 0), "Tue, 1 Oct 14:00");
    // tdesktop names both ends even within one day.
    assert_eq!(hourly.range_title(0, 23), "1 Oct 2024 \u{2014} 1 Oct 2024");
    // Weeks.
    let weekly = parse_chart(&chart_json(
        &days(2),
        &[("y0", "A", "#0", vec![1, 2])],
        json!({"xTooltipFormatter": "'week'"}),
    ))
    .unwrap();
    assert_eq!(weekly.point_title(0, 0), "1 Oct \u{2014} 8 Oct 2024");
    assert_eq!(weekly.range_title(0, 0), "1 Oct 2024 \u{2014} 8 Oct 2024");
    // Hour-of-day series (x is 0..23, step 1) are not dates.
    let index = parse_chart(r#"{"columns":[["x",0,1,2],["y0",1,2,3]]}"#).unwrap();
    assert_eq!(index.day_label(2, 0), "02:00");
    assert_eq!(index.range_title(0, 2), "");
    assert_eq!(index.point_title(1, 0), "01:00");
}

#[test]
fn numbers_read_like_telegram_desktop() {
    assert_eq!(format_short(9_999), "9999");
    assert_eq!(format_short(12_345), "12.3K");
    assert_eq!(format_short(20_000), "20K");
    assert_eq!(format_short(4_000_000), "4M");
    assert_eq!(format_short(1_250_000_000), "1.2B");
    assert_eq!(format_decimal(1_234_567), "1,234,567");
    assert_eq!(format_decimal(-12), "-12");
    assert_eq!(overview_value(12_345.0), "12.3K");
    assert_eq!(percent_label(61.5), "61.5%");
    assert_eq!(percent_label(61.456), "61.46%");
    assert_eq!(
        overview_delta(12_345.0, 11_700.0, 5.5),
        Some(("+645 (5.5%)".to_string(), false))
    );
    assert_eq!(
        overview_delta(80.0, 100.0, -20.0),
        Some(("\u{2212}20 (20%)".to_string(), true))
    );
    assert_eq!(overview_delta(5.0, 5.0, 0.0), None);
    assert_eq!(overview_delta(5.0, 0.0, 0.0), None);
    assert_eq!(
        overview_delta(5_000_000.0, 1_000_000.0, 400.0),
        Some(("+4M (400%)".to_string(), false))
    );
}

#[test]
fn percentages_add_up_to_one_hundred() {
    let parts = percentages(&[1, 1, 1], &[true, true, true]);
    let total: i64 = parts
        .iter()
        .map(|p| p.trim_end_matches('%').parse::<i64>().unwrap())
        .sum();
    assert_eq!(total, 100);
    assert_eq!(percentages(&[3, 1], &[true, false]), vec!["100%", "0%"]);
    assert_eq!(percentages(&[0, 0], &[true, true]), vec!["0%", "0%"]);
}
