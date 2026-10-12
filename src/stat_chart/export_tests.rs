//! Pie zoom math and the "Export to File" workbook.
use super::*;
use crate::telegram::envelope::{
    ChannelStatistics, ChatStatistics, StatisticalGraph, StatisticalValue,
};
use std::io::Read;

const OCT_1: f64 = 1_727_740_800_000.0;

fn chart(points: usize, step: f64, lines: &[(&str, Vec<i64>)]) -> StatChart {
    let mut columns = vec![{
        let mut c = vec![serde_json::json!("x")];
        c.extend((0..points).map(|i| serde_json::json!(OCT_1 + i as f64 * step)));
        c
    }];
    let mut names = serde_json::Map::new();
    for (name, y) in lines {
        let id = format!("y{}", columns.len() - 1);
        let mut c = vec![serde_json::json!(id)];
        c.extend(y.iter().map(|v| serde_json::json!(v)));
        columns.push(c);
        names.insert(id, serde_json::json!(name));
    }
    parse_chart(&serde_json::json!({"columns": columns, "names": names}).to_string()).unwrap()
}

#[test]
fn pie_parts_round_to_one_hundred_and_pick_slices_by_angle() {
    let parts = pie_parts(&[1.0, 1.0, 1.0], true);
    let texts: Vec<&str> = parts.iter().map(|p| p.text.as_str()).collect();
    assert_eq!(texts.iter().filter(|t| **t == "33%").count(), 2);
    assert_eq!(texts.iter().filter(|t| **t == "34%").count(), 1);
    let total: f64 = parts.iter().map(|p| p.share).sum();
    assert!((total - 1.0).abs() < 1e-9);
    // A quarter and three quarters: the first slice runs from the top to
    // three o'clock.
    let parts = pie_parts(&[1.0, 3.0], true);
    assert_eq!(pie_angles(&parts), vec![(0.0, 90.0), (90.0, 360.0)]);
    assert_eq!(pie_part_at(&parts, 1.0, -1.0), Some(0));
    assert_eq!(pie_part_at(&parts, -1.0, 1.0), Some(1));
    assert_eq!(pie_part_at(&parts, 0.0, 1.0), Some(1));
    assert_eq!(pie_part_at(&parts, 0.0, 0.0), None);
}

#[test]
fn pie_sums_follow_the_range_and_switched_off_lines() {
    let c = chart(
        4,
        ONE_DAY_MS,
        &[("A", vec![1, 2, 3, 4]), ("B", vec![10, 0, 0, 10])],
    );
    assert_eq!(pie_sums(&c, &[1.0, 1.0], (1, 2)), vec![5.0, 0.0]);
    assert_eq!(pie_sums(&c, &[1.0, 0.0], (0, 3)), vec![10.0, 0.0]);
}

#[test]
fn local_zoom_covers_eight_days_around_the_point() {
    let c = chart(30, ONE_DAY_MS, &[("A", vec![1; 30])]);
    let zoom = local_zoom(&c, 15);
    assert_eq!(zoom.limit, (11, 20));
    // The selected day sits a quarter to three quarters into its step.
    let indices = local_zoom_indices(&c, zoom.range, zoom.limit);
    assert_eq!(indices, (15, 15));
    // Near the start and the end the window stays inside the chart.
    assert_eq!(local_zoom(&c, 1).limit, (0, 9));
    let end = local_zoom(&c, 29);
    assert_eq!(end.limit, (21, 29));
    assert_eq!(local_zoom_indices(&c, end.range, end.limit), (29, 29));
    // Selecting the whole window names all its points but the last
    // (unless it is the chart's last).
    assert_eq!(
        local_zoom_indices(&c, Limits::new(0.0, 1.0), (11, 20)),
        (11, 19)
    );
    // A tiny chart.
    let small = chart(3, ONE_DAY_MS, &[("A", vec![1, 2, 3])]);
    assert_eq!(local_zoom(&small, 1).limit, (0, 2));
}

fn unzip(bytes: &[u8]) -> Vec<(String, String)> {
    // Walk the local headers (no data descriptors in this writer).
    let mut entries = Vec::new();
    let mut at = 0;
    while at + 30 <= bytes.len()
        && u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) == 0x0403_4b50
    {
        let field = |o: usize| u16::from_le_bytes(bytes[at + o..at + o + 2].try_into().unwrap());
        let crc = u32::from_le_bytes(bytes[at + 14..at + 18].try_into().unwrap());
        let csize = u32::from_le_bytes(bytes[at + 18..at + 22].try_into().unwrap()) as usize;
        let name_len = field(26) as usize;
        let extra = field(28) as usize;
        let name = String::from_utf8(bytes[at + 30..at + 30 + name_len].to_vec()).unwrap();
        let start = at + 30 + name_len + extra;
        let mut text = String::new();
        flate2::read::DeflateDecoder::new(&bytes[start..start + csize])
            .read_to_string(&mut text)
            .unwrap();
        let mut check = flate2::Crc::new();
        check.update(text.as_bytes());
        assert_eq!(check.sum(), crc, "{name}");
        entries.push((name, text));
        at = start + csize;
    }
    entries
}

#[test]
fn workbook_holds_every_part_with_valid_entries() {
    let c = chart(2, ONE_DAY_MS, &[("Views & <shares>", vec![10, 20])]);
    let sheets = vec![
        overview_sheet(&[("Views", 12.0, Some(10.0)), ("Hidden", -1.0, None)]),
        chart_sheet("Growth", &c),
        chart_sheet("Growth", &c),
    ];
    let bytes = serialize_workbook(&sheets);
    let entries = unzip(&bytes);
    let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "[Content_Types].xml",
            "_rels/.rels",
            "xl/workbook.xml",
            "xl/_rels/workbook.xml.rels",
            "xl/styles.xml",
            "xl/worksheets/sheet1.xml",
            "xl/worksheets/sheet2.xml",
            "xl/worksheets/sheet3.xml",
        ]
    );
    let workbook = &entries[2].1;
    assert!(workbook.contains(r#"<sheet name="Growth" sheetId="2" r:id="rId2"/>"#));
    assert!(workbook.contains(r#"<sheet name="Growth (2)" sheetId="3" r:id="rId3"/>"#));
    let overview = &entries[5].1;
    assert!(overview.contains(r#"<c r="A2" t="inlineStr"><is><t xml:space="preserve">Views</t></is></c><c r="B2"><v>12</v></c><c r="C2"><v>10</v></c>"#));
    assert!(
        !overview.contains("<c r=\"B3\""),
        "negative counters stay blank"
    );
    let growth = &entries[6].1;
    // Header escaped and right-aligned over numbers, dates as serials.
    assert!(growth.contains("Views &amp; &lt;shares&gt;"));
    assert!(growth.contains(r#"<c r="B1" s="4" t="inlineStr">"#));
    assert!(growth.contains(r#"<c r="A2" s="1"><v>45566</v></c>"#));
    assert!(growth.contains(r#"<pane ySplit="1""#));
    assert!(serialize_workbook(&[]).is_empty());
}

#[test]
fn sheet_names_follow_excel_rules() {
    let sheet = |name: &str| Sheet {
        name: name.into(),
        rows: Vec::new(),
    };
    let names = unique_sheet_names(&[
        sheet("Views: by [source]?"),
        sheet(""),
        sheet("A very long chart title that goes on and on"),
        sheet("A very long chart title that goes on and on"),
    ]);
    assert_eq!(names[0], "Views by source");
    assert_eq!(names[1], "Sheet2");
    assert_eq!(names[2].chars().count(), 31);
    assert!(names[3].ends_with(" (2)") && names[3].chars().count() <= 31);
}

#[test]
fn hourly_charts_export_hours_and_times() {
    let hours =
        parse_chart(r#"{"columns":[["x",0,1],["y0",3,4]],"names":{"y0":"Views"}}"#).unwrap();
    let sheet = chart_sheet("By hour", &hours);
    assert_eq!(sheet.rows[0][0], Cell::Header("Hour".into()));
    assert_eq!(sheet.rows[1][0], Cell::Number(0.0));
    let day = chart(2, 3_600_000.0, &[("A", vec![1, 2])]);
    assert!(matches!(
        chart_sheet("Day", &day).rows[1][0],
        Cell::DateTime(_)
    ));
}

#[test]
fn chat_workbook_has_the_overview_and_loaded_charts() {
    let value = |v: f64| StatisticalValue {
        value: v,
        previous_value: v - 1.0,
        growth_rate_percentage: 1.0,
    };
    let json =
        r#"{"columns":[["x",1727740800000,1727827200000],["y0",1,2]],"names":{"y0":"Followers"}}"#;
    let data = StatisticalGraph::Data {
        json_data: json.into(),
        zoom_token: String::new(),
    };
    let error = StatisticalGraph::Error {
        error_message: "x".into(),
    };
    let stats = ChatStatistics::Channel(Box::new(ChannelStatistics {
        period_start: 0,
        period_end: 0,
        member_count: value(10.0),
        mean_message_view_count: value(5.0),
        mean_message_share_count: value(1.0),
        mean_message_reaction_count: value(2.0),
        mean_story_view_count: value(3.0),
        mean_story_share_count: value(1.0),
        mean_story_reaction_count: value(1.0),
        enabled_notifications_percentage: 61.5,
        member_count_graph: data.clone(),
        join_graph: error.clone(),
        mute_graph: error.clone(),
        view_count_by_hour_graph: error.clone(),
        view_count_by_source_graph: error.clone(),
        join_by_source_graph: error.clone(),
        language_graph: data,
        message_interaction_graph: error.clone(),
        message_reaction_graph: error.clone(),
        story_interaction_graph: error.clone(),
        story_reaction_graph: error.clone(),
        instant_view_interaction_graph: error,
        recent_interactions: Vec::new(),
    }));
    let sheets = chat_statistics_sheets(&stats, |graph| match graph {
        StatisticalGraph::Data { json_data, .. } => parse_chart(json_data).ok(),
        _ => None,
    });
    let names: Vec<&str> = sheets.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["Overview", "Growth", "Languages"]);
    assert_eq!(sheets[0].rows.len(), 9);
    assert_eq!(
        sheets[0].rows[8].len(),
        2,
        "notifications have no previous value"
    );
    let post = post_statistics_sheets(
        PostCounters {
            views: Some(5),
            ..Default::default()
        },
        &[
            ("Interactions", None),
            (
                "Poll votes",
                Some(chart(2, ONE_DAY_MS, &[("Votes", vec![1, 2])])),
            ),
        ],
    );
    assert_eq!(post.len(), 2);
    assert_eq!(post[1].name, "Poll votes");
    assert_eq!(
        post[0].rows[2],
        vec![Cell::Text("Reactions".into()), Cell::Empty]
    );
    assert_eq!(
        export_file_name("My: Channel", Some(42 << 20), None, "2024-10-01"),
        "My_ Channel_42_2024-10-01.xlsx"
    );
    assert_eq!(
        export_file_name("", None, Some(7), "2024-10-01"),
        "Statistics_story_7_2024-10-01.xlsx"
    );
}
