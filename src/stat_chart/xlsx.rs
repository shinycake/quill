//! "Export to File": statistics as an Excel workbook, one sheet for the
//! overview and one per chart (tdesktop `statistics_sheets.cpp`,
//! `statistics_xlsx.cpp`). The workbook is a zip of SpreadsheetML parts
//! with inline strings, written in memory.

use super::{ONE_DAY_MS, StatChart};
use std::io::Write;

/// One spreadsheet cell.
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    Empty,
    Text(String),
    /// Bold; right-aligned over a numeric column.
    Header(String),
    Number(f64),
    /// A day (milliseconds since the epoch).
    Date(f64),
    /// A moment (milliseconds since the epoch).
    DateTime(f64),
}

/// One worksheet.
#[derive(Debug, Clone, PartialEq)]
pub struct Sheet {
    pub name: String,
    pub rows: Vec<Vec<Cell>>,
}

const EPOCH_SERIAL: f64 = 25569.0;
const STYLE_DATE: u8 = 1;
const STYLE_DATE_TIME: u8 = 2;
const STYLE_HEADER: u8 = 3;
const STYLE_HEADER_RIGHT: u8 = 4;
const MAX_SHEET_NAME: usize = 31;

fn serial(milliseconds: f64) -> f64 {
    milliseconds / ONE_DAY_MS + EPOCH_SERIAL
}

/// A chart as rows: a date (or hour) column, then one per line
/// (`SheetFromChart`).
pub fn chart_sheet(title: &str, chart: &StatChart) -> Sheet {
    let hours = chart.x.first().is_none_or(|x| *x < ONE_DAY_MS);
    let whole_days = chart.time_step >= ONE_DAY_MS;
    let mut header = vec![Cell::Header(if hours { "Hour" } else { "Date" }.into())];
    header.extend(chart.lines.iter().map(|l| Cell::Header(l.name.clone())));
    let mut rows = vec![header];
    for (i, x) in chart.x.iter().enumerate() {
        let mut row = vec![if hours {
            Cell::Number(*x)
        } else if whole_days {
            Cell::Date(*x)
        } else {
            Cell::DateTime(*x)
        }];
        row.extend(chart.lines.iter().map(|line| {
            line.y
                .get(i)
                .map_or(Cell::Empty, |v| Cell::Number(*v as f64))
        }));
        rows.push(row);
    }
    Sheet {
        name: title.to_string(),
        rows,
    }
}

/// The overview sheet: a value and its previous value per row, or a
/// single counter (a negative counter is left blank).
pub fn overview_sheet(values: &[(&str, f64, Option<f64>)]) -> Sheet {
    let mut rows = vec![vec![
        Cell::Empty,
        Cell::Header("Value".into()),
        Cell::Header("Previous".into()),
    ]];
    for (title, value, previous) in values {
        let mut row = vec![
            Cell::Text(title.to_string()),
            if *value < 0.0 {
                Cell::Empty
            } else {
                Cell::Number(*value)
            },
        ];
        if let Some(previous) = previous {
            row.push(Cell::Number(*previous));
        }
        rows.push(row);
    }
    Sheet {
        name: "Overview".into(),
        rows,
    }
}

fn column_name(index: usize) -> String {
    let mut name = Vec::new();
    let mut i = index + 1;
    while i > 0 {
        name.push(b'A' + ((i - 1) % 26) as u8);
        i = (i - 1) / 26;
    }
    name.reverse();
    String::from_utf8(name).unwrap_or_default()
}

fn escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if c >= ' ' || matches!(c, '\t' | '\n' | '\r') => out.push(c),
            _ => {}
        }
    }
    out
}

fn number_text(value: f64, digits: usize) -> String {
    if value == value.round() && value.abs() < 1e15 {
        format!("{}", value.round() as i64)
    } else {
        format!("{value:.digits$}")
    }
}

fn sheet_name(name: &str, index: usize, limit: usize) -> String {
    let cleaned: String = name.chars().filter(|c| !":\\/?*[]".contains(*c)).collect();
    let trimmed: String = cleaned.trim().chars().take(limit).collect();
    let trimmed = trimmed.trim().to_string();
    if trimmed.is_empty() {
        format!("Sheet{}", index + 1)
    } else {
        trimmed
    }
}

/// Sheet names as Excel accepts them: no `:\/?*[]`, at most 31
/// characters, unique (` (2)`, ` (3)` …).
pub fn unique_sheet_names(sheets: &[Sheet]) -> Vec<String> {
    let mut names: Vec<String> = Vec::with_capacity(sheets.len());
    for (i, sheet) in sheets.iter().enumerate() {
        let mut name = sheet_name(&sheet.name, i, MAX_SHEET_NAME);
        let mut attempt = 2;
        while names.contains(&name) {
            let suffix = format!(" ({attempt})");
            name = sheet_name(&sheet.name, i, MAX_SHEET_NAME - suffix.len()) + &suffix;
            attempt += 1;
        }
        names.push(name);
    }
    names
}

fn numeric_columns(sheet: &Sheet) -> Vec<bool> {
    let mut numeric: Vec<bool> = Vec::new();
    let mut filled: Vec<bool> = Vec::new();
    for row in sheet.rows.iter().skip(1) {
        if numeric.len() < row.len() {
            numeric.resize(row.len(), true);
            filled.resize(row.len(), false);
        }
        for (j, cell) in row.iter().enumerate() {
            match cell {
                Cell::Empty => {}
                Cell::Number(_) | Cell::Date(_) | Cell::DateTime(_) => {
                    if let Some(f) = filled.get_mut(j) {
                        *f = true;
                    }
                }
                _ => {
                    if let Some(n) = numeric.get_mut(j) {
                        *n = false;
                    }
                }
            }
        }
    }
    numeric.iter().zip(filled).map(|(n, f)| *n && f).collect()
}

fn cell_width(cell: &Cell) -> usize {
    match cell {
        Cell::Empty => 0,
        Cell::Text(text) => text.chars().count() + 2,
        Cell::Header(text) => text.chars().count() + 3,
        Cell::Number(value) => number_text(*value, 6).len() + 2,
        Cell::Date(_) => 12,
        Cell::DateTime(_) => 18,
    }
}

fn sheet_xml(sheet: &Sheet) -> String {
    let numeric = numeric_columns(sheet);
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">",
    );
    let starts_with_header = sheet
        .rows
        .first()
        .is_some_and(|row| row.iter().any(|c| matches!(c, Cell::Header(_))));
    if starts_with_header {
        out.push_str(
            "<sheetViews><sheetView workbookViewId=\"0\">\
             <pane ySplit=\"1\" topLeftCell=\"A2\" activePane=\"bottomLeft\" state=\"frozen\"/>\
             </sheetView></sheetViews>",
        );
    }
    let mut widths: Vec<usize> = Vec::new();
    for row in &sheet.rows {
        if widths.len() < row.len() {
            widths.resize(row.len(), 0);
        }
        for (i, cell) in row.iter().enumerate() {
            if let Some(w) = widths.get_mut(i) {
                *w = (*w).max(cell_width(cell));
            }
        }
    }
    if !widths.is_empty() {
        out.push_str("<cols>");
        for (i, width) in widths.iter().enumerate() {
            out.push_str(&format!(
                "<col min=\"{0}\" max=\"{0}\" width=\"{1}\" customWidth=\"1\"/>",
                i + 1,
                width.clamp(&8, &48)
            ));
        }
        out.push_str("</cols>");
    }
    out.push_str("<sheetData>");
    for (i, row) in sheet.rows.iter().enumerate() {
        out.push_str(&format!("<row r=\"{}\">", i + 1));
        for (j, cell) in row.iter().enumerate() {
            let reference = format!("{}{}", column_name(j), i + 1);
            let open = |style: u8, kind: &str| {
                let mut tag = format!("<c r=\"{reference}\"");
                if style != 0 {
                    tag.push_str(&format!(" s=\"{style}\""));
                }
                if !kind.is_empty() {
                    tag.push_str(&format!(" t=\"{kind}\""));
                }
                tag.push('>');
                tag
            };
            let text = |style: u8, text: &str| {
                format!(
                    "{}<is><t xml:space=\"preserve\">{}</t></is></c>",
                    open(style, "inlineStr"),
                    escaped(text)
                )
            };
            let number = |style: u8, value: f64, digits: usize| {
                format!(
                    "{}<v>{}</v></c>",
                    open(style, ""),
                    number_text(value, digits)
                )
            };
            out.push_str(&match cell {
                Cell::Empty => continue,
                Cell::Text(value) => text(0, value),
                Cell::Header(value) => {
                    let right = numeric.get(j).copied().unwrap_or(false);
                    text(
                        if right {
                            STYLE_HEADER_RIGHT
                        } else {
                            STYLE_HEADER
                        },
                        value,
                    )
                }
                Cell::Number(value) => number(0, *value, 6),
                Cell::Date(ms) => number(STYLE_DATE, serial(*ms), 10),
                Cell::DateTime(ms) => number(STYLE_DATE_TIME, serial(*ms), 10),
            });
        }
        out.push_str("</row>");
    }
    out.push_str("</sheetData></worksheet>");
    out
}

const STYLES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">\
<fonts count=\"2\"><font><sz val=\"11\"/><name val=\"Calibri\"/></font>\
<font><b/><sz val=\"11\"/><name val=\"Calibri\"/></font></fonts>\
<fills count=\"2\"><fill><patternFill patternType=\"none\"/></fill>\
<fill><patternFill patternType=\"gray125\"/></fill></fills>\
<borders count=\"1\"><border/></borders>\
<cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs>\
<cellXfs count=\"5\">\
<xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/>\
<xf numFmtId=\"14\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyNumberFormat=\"1\"/>\
<xf numFmtId=\"22\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyNumberFormat=\"1\"/>\
<xf numFmtId=\"0\" fontId=\"1\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyFont=\"1\"/>\
<xf numFmtId=\"0\" fontId=\"1\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyFont=\"1\" applyAlignment=\"1\">\
<alignment horizontal=\"right\"/></xf></cellXfs></styleSheet>";

fn workbook_xml(names: &[String]) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" \
         xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets>",
    );
    for (i, name) in names.iter().enumerate() {
        out.push_str(&format!(
            "<sheet name=\"{}\" sheetId=\"{1}\" r:id=\"rId{1}\"/>",
            escaped(name),
            i + 1
        ));
    }
    out.push_str("</sheets></workbook>");
    out
}

fn relationship(id: usize, kind: &str, target: &str) -> String {
    format!(
        "<Relationship Id=\"rId{id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}\" Target=\"{target}\"/>"
    )
}

fn workbook_rels(count: usize) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
    );
    for i in 0..count {
        out.push_str(&relationship(
            i + 1,
            "worksheet",
            &format!("worksheets/sheet{}.xml", i + 1),
        ));
    }
    out.push_str(&relationship(count + 1, "styles", "styles.xml"));
    out.push_str("</Relationships>");
    out
}

const ROOT_RELS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/>\
</Relationships>";

fn content_types(count: usize) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
         <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
         <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
         <Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>\
         <Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>",
    );
    for i in 0..count {
        out.push_str(&format!(
            "<Override PartName=\"/xl/worksheets/sheet{}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>",
            i + 1
        ));
    }
    out.push_str("</Types>");
    out
}

/// The workbook file's bytes; empty without sheets.
pub fn serialize_workbook(sheets: &[Sheet]) -> Vec<u8> {
    if sheets.is_empty() {
        return Vec::new();
    }
    let names = unique_sheet_names(sheets);
    let mut parts: Vec<(String, Vec<u8>)> = vec![
        (
            "[Content_Types].xml".into(),
            content_types(sheets.len()).into_bytes(),
        ),
        ("_rels/.rels".into(), ROOT_RELS.as_bytes().to_vec()),
        ("xl/workbook.xml".into(), workbook_xml(&names).into_bytes()),
        (
            "xl/_rels/workbook.xml.rels".into(),
            workbook_rels(sheets.len()).into_bytes(),
        ),
        ("xl/styles.xml".into(), STYLES.as_bytes().to_vec()),
    ];
    for (i, sheet) in sheets.iter().enumerate() {
        parts.push((
            format!("xl/worksheets/sheet{}.xml", i + 1),
            sheet_xml(sheet).into_bytes(),
        ));
    }
    zip(&parts)
}

/// A zip of in-memory parts, deflated, with a central directory.
pub(crate) fn zip(parts: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    let (time, date) = crate::folder_archive::dos_time(std::time::SystemTime::now());
    for (name, data) in parts {
        let mut crc = flate2::Crc::new();
        crc.update(data);
        let crc = crc.sum();
        let mut encoder =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        let _ = encoder.write_all(data);
        let compressed = encoder.finish().unwrap_or_default();
        let offset = out.len() as u32;
        let header = |out: &mut Vec<u8>, signature: u32, central: bool| {
            out.extend_from_slice(&signature.to_le_bytes());
            if central {
                out.extend_from_slice(&20u16.to_le_bytes()); // made by
            }
            out.extend_from_slice(&20u16.to_le_bytes()); // needed
            out.extend_from_slice(&(1u16 << 11).to_le_bytes()); // UTF-8 names
            out.extend_from_slice(&8u16.to_le_bytes()); // deflate
            out.extend_from_slice(&time.to_le_bytes());
            out.extend_from_slice(&date.to_le_bytes());
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // extra
        };
        header(&mut out, 0x0403_4b50, false);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&compressed);
        header(&mut central, 0x0201_4b50, true);
        central.extend_from_slice(&0u16.to_le_bytes()); // comment
        central.extend_from_slice(&0u16.to_le_bytes()); // disk
        central.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
        central.extend_from_slice(&0u32.to_le_bytes()); // external attributes
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let directory_start = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(parts.len() as u16).to_le_bytes());
    out.extend_from_slice(&(parts.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&directory_start.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

/// The suggested file name: the chat's title (48 characters at most,
/// without characters file systems reject), the post or story, today's
/// date (`SuggestedName`).
pub fn export_file_name(
    title: &str,
    message_id: Option<i64>,
    story_id: Option<i32>,
    today: &str,
) -> String {
    let cleaned: String = title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .map(|c| {
            if "<>:\"/\\|?*".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .take(48)
        .collect();
    let mut name = cleaned.trim().to_string();
    if name.is_empty() {
        name = "Statistics".into();
    }
    if let Some(id) = message_id {
        name.push_str(&format!("_{}", id >> 20));
    } else if let Some(id) = story_id {
        name.push_str(&format!("_story_{id}"));
    }
    format!("{name}_{today}.xlsx")
}
