//! What a platform text system's `layout_line` must do with right-to-left and
//! mixed-direction text, written once and checked against every backend Quill
//! ships: CoreText on macOS, DirectWrite on Windows and cosmic-text on Linux
//! (`tests/backends.rs`), plus GPUI's stand-in text system that the input and
//! composer tests run on.
//!
//! The contract everything above the backends relies on (the input engine's
//! bidi placement in gpui-base, message text, selection, hit testing):
//!
//! * a line takes its base direction from its first strong character, looking
//!   past isolated runs (UAX #9 P2/P3), so a name wrapped in FSI…PDI does not
//!   decide which way a sentence reads;
//! * glyphs come back in visual order, left to right, each carrying the byte
//!   index of the character it starts in the original text;
//! * characters sit where the Unicode Bidirectional Algorithm puts them on
//!   screen (rules L1/L2), trailing whitespace included;
//! * every glyph lies inside the line's reported width.
//!
//! Windows once broke the second rule and every Hebrew word read backwards,
//! with no test anywhere to notice; see docs/decisions/rtl-sweep.md.

use std::collections::BTreeMap;
use std::ops::Range;

use std::sync::Arc;

use gpui::{
    DecorationPlan, DecorationRun, Font, FontId, FontRun, Hsla, LineLayout, PlatformTextSystem,
    SharedString, TextRun, TextSystem, UnderlineStyle, WindowTextSystem, px,
};
use unicode_bidi::{BidiClass, BidiInfo, Level, bidi_class};

/// Lines that every backend must lay out the way UAX #9 orders them.
pub const CASES: &[&str] = &[
    "שלום",
    "مرحبا بالعالم",
    "abc שלום def",
    "שלום world",
    "שלום ",
    "שלום  ",
    "  שלום",
    "אבג דהו ",
    "123 שלום ",
    "שלום 123 עולם",
    "١٢٣ مرحبا",
    "مرحبا ١٢٣ عالم",
    "\u{2068}שלום\u{2069} joined the group",
    "Dana \u{2068}דנה\u{2069}: 5 new messages",
    "\u{2068}Dana\u{2069} הצטרפה לקבוצה",
    "שלום 😀 עולם",
    "שלום (עולם) [1]",
    "שָׁלוֹם עוֹלָם",
    "لا إله",
    "Version 2.0 של האפליקציה",
    "@username שלום",
    "א\tב",
    "א\t\tב",
    "abc\tשלום",
    "אב\t\tגד",
    "שלום\u{2028}abc",
    "abc שלום\u{2028}עולם",
    "\u{200F}1.",
    "\u{200F}12)",
];

/// Two x positions closer than this are the same place.
const EPS: f32 = 0.01;

/// Every way `layout` breaks the contract for `text`, empty when it holds.
pub fn violations(text: &str, layout: &LineLayout) -> Vec<String> {
    let mut found = Vec::new();
    let glyphs: Vec<_> = layout.runs.iter().flat_map(|run| &run.glyphs).collect();
    let width = f32::from(layout.width);

    // Combining marks sit over or under their base, so their x may run backwards.
    let spacing: Vec<_> = glyphs
        .iter()
        .filter(|glyph| compared(text, glyph.index))
        .collect();
    for pair in spacing.windows(2) {
        if let [a, b] = pair
            && f32::from(b.position.x) + EPS < f32::from(a.position.x)
        {
            found.push(format!(
                "{text:?}: glyphs out of visual order (byte {} at x {:.2}, then byte {} at x {:.2})",
                a.index,
                f32::from(a.position.x),
                b.index,
                f32::from(b.position.x)
            ));
            break;
        }
    }
    for glyph in &glyphs {
        let x = f32::from(glyph.position.x);
        if !text.is_char_boundary(glyph.index) || glyph.index > text.len() {
            found.push(format!(
                "{text:?}: glyph index {} is not a character start",
                glyph.index
            ));
        }
        if x < -EPS || x > width + EPS {
            found.push(format!(
                "{text:?}: glyph for byte {} at x {x:.2} lies outside the line (width {width:.2})",
                glyph.index
            ));
        }
    }

    // Where each character starts on screen: the leftmost glyph that names it.
    let mut x_of = BTreeMap::<usize, f32>::new();
    for glyph in &glyphs {
        let x = f32::from(glyph.position.x);
        x_of.entry(glyph.index)
            .and_modify(|seen| *seen = seen.min(x))
            .or_insert(x);
    }
    let order: Vec<usize> = visual_order(text)
        .into_iter()
        .filter(|index| compared(text, *index) && x_of.contains_key(index))
        .collect();
    for pair in order.windows(2) {
        if let [left, right] = pair
            && let (Some(&left_x), Some(&right_x)) = (x_of.get(left), x_of.get(right))
            && right_x + EPS < left_x
        {
            found.push(format!(
                "{text:?}: byte {left} should sit left of byte {right} (x {left_x:.2} vs {right_x:.2})",
            ));
            break;
        }
    }
    found
}

/// The byte index of every character of `text` in the order UAX #9 shows them,
/// left to right, the base direction taken from the first strong character.
pub fn visual_order(text: &str) -> Vec<usize> {
    let info = BidiInfo::new(text, None);
    let mut order = Vec::with_capacity(text.len());
    for paragraph in &info.paragraphs {
        let (levels, runs) = info.visual_runs(paragraph, paragraph.range.clone());
        for run in runs {
            let starts = char_starts(text, run.clone());
            let rtl = levels.get(run.start).is_some_and(|level| level.is_rtl());
            if rtl {
                order.extend(starts.into_iter().rev());
            } else {
                order.extend(starts);
            }
        }
    }
    order
}

fn char_starts(text: &str, range: Range<usize>) -> Vec<usize> {
    text.get(range.clone())
        .map(|part| part.char_indices().map(|(i, _)| range.start + i).collect())
        .unwrap_or_default()
}

/// Whether the character at `index` takes a place of its own on screen: bidi
/// controls and combining marks ride on their neighbours, so where a backend
/// puts their (empty) glyph says nothing about order.
fn compared(text: &str, index: usize) -> bool {
    let Some(c) = text.get(index..).and_then(|rest| rest.chars().next()) else {
        return false;
    };
    !matches!(
        bidi_class(c),
        BidiClass::NSM
            | BidiClass::BN
            | BidiClass::LRE
            | BidiClass::RLE
            | BidiClass::LRO
            | BidiClass::RLO
            | BidiClass::PDF
            | BidiClass::LRI
            | BidiClass::RLI
            | BidiClass::FSI
            | BidiClass::PDI
    ) && !('\u{FE00}'..='\u{FE0F}').contains(&c)
        && c != '\u{200D}'
}

/// Single-direction right-to-left lines whose caret stops must run right to left.
pub const RTL_CARET_CASES: &[&str] = &["שלום", "مرحبا", "אבג דהו", "שלום עולם"];

/// Where a caret goes on a right-to-left line: `x_for_index` moves left as the text goes
/// on, ends at the line's left edge, and `closest_index_for_x` finds each stop again.
pub fn caret_violations(text: &str, layout: &LineLayout) -> Vec<String> {
    let mut found = Vec::new();
    let stops: Vec<usize> = text
        .char_indices()
        .map(|(i, _)| i)
        .chain([text.len()])
        .collect();
    let xs: Vec<f32> = stops
        .iter()
        .map(|&i| f32::from(layout.x_for_index(i)))
        .collect();
    for (pair, at) in xs.windows(2).zip(&stops) {
        if let [here, next] = pair
            && next + EPS >= *here
        {
            found.push(format!(
                "{text:?}: the caret after byte {at} (x {next:.2}) is not left of the one before it (x {here:.2})"
            ));
            break;
        }
    }
    if let (Some(first), Some(last)) = (xs.first(), xs.last()) {
        let width = f32::from(layout.width);
        if (first - width).abs() > 0.5 || last.abs() > 0.5 {
            found.push(format!(
                "{text:?}: carets should span the line, start at {width:.2} and end at 0 (got {first:.2} and {last:.2})"
            ));
        }
    }
    for (&stop, &x) in stops.iter().zip(&xs) {
        let hit = layout.closest_index_for_x(px(x));
        if hit != stop {
            found.push(format!(
                "{text:?}: a click at the caret of byte {stop} lands on byte {hit}"
            ));
        }
    }
    found
}

/// A link in the middle of a Hebrew sentence keeps its own color and underline: each
/// glyph takes the decoration whose bytes hold it, and the underline covers just the
/// link's glyphs.
pub fn decoration_violations(system: &dyn PlatformTextSystem, font_id: FontId) -> Vec<String> {
    let text = "שלום קישור עולם";
    let link = 9..19;
    let layout = system.layout_line(
        text,
        px(16.0),
        &[FontRun {
            len: text.len(),
            font_id,
        }],
    );
    let plain = DecorationRun {
        len: link.start as u32,
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let blue = gpui::blue();
    let runs = [
        plain.clone(),
        DecorationRun {
            len: link.len() as u32,
            color: blue,
            underline: Some(UnderlineStyle {
                thickness: px(1.),
                color: None,
                wavy: false,
            }),
            ..plain.clone()
        },
        DecorationRun {
            len: (text.len() - link.end) as u32,
            ..plain
        },
    ];
    let plan = DecorationPlan::new(&layout, &runs);
    let mut found = Vec::new();
    let mut link_xs = Vec::new();
    for planned in &plan.glyphs {
        let Some(glyph) = layout
            .runs
            .get(planned.run)
            .and_then(|run| run.glyphs.get(planned.glyph))
        else {
            continue;
        };
        let expected = if link.contains(&glyph.index) {
            link_xs.push(f32::from(glyph.position.x));
            1
        } else if glyph.index < link.start {
            0
        } else {
            2
        };
        if planned.decoration != Some(expected) {
            found.push(format!(
                "{text:?}: byte {} took decoration {:?}, not {expected}",
                glyph.index, planned.decoration
            ));
        }
    }
    let left = link_xs.iter().copied().fold(f32::INFINITY, f32::min);
    match plan.underlines.as_slice() {
        [underline] => {
            if (f32::from(underline.start) - left).abs() > 0.5 {
                found.push(format!(
                    "{text:?}: the link's underline starts at {:.2}, its glyphs at {left:.2}",
                    f32::from(underline.start)
                ));
            }
            if underline.style.color != Some(blue) {
                found.push(format!("{text:?}: the underline is not the link's color"));
            }
        }
        other => found.push(format!(
            "{text:?}: {} underlines instead of one under the link",
            other.len()
        )),
    }
    found
}

/// Lays every case out on `system` in `family` and returns all violations.
pub fn check(system: &dyn PlatformTextSystem, family: &str) -> Vec<String> {
    let font = Font {
        family: family.to_string().into(),
        ..gpui::font(family)
    };
    let font_id = match system.font_id(&font) {
        Ok(id) => id,
        Err(error) => return vec![format!("no font {family:?}: {error}")],
    };
    let layout = |text: &str| {
        system.layout_line(
            text,
            px(16.0),
            &[FontRun {
                len: text.len(),
                font_id,
            }],
        )
    };
    let mut found: Vec<String> = CASES
        .iter()
        .flat_map(|text| violations(text, &layout(text)))
        .collect();
    for text in RTL_CARET_CASES {
        found.extend(caret_violations(text, &layout(text)));
    }
    found.extend(decoration_violations(system, font_id));
    found
}

/// Paragraphs long enough to wrap at [`WRAP_WIDTH`], each with what its first row must
/// hold: right-to-left ones start at the right of their first row, with their first
/// word, as Telegram Desktop wraps them.
pub const WRAP_CASES: &[&str] = &[
    "שלום עולם זה משפט ארוך מאוד שצריך לעבור כמה שורות כדי לבדוק את הסדר",
    "مرحبا بالعالم هذه جملة طويلة جدا يجب أن تلتف على عدة أسطر للتحقق من الترتيب",
    "שלום world זה משפט עם English words באמצע שצריך לעבור שורה",
    "Hello עולם this is a long English sentence with some עברית inside it",
    "הודעה ארוכה שמסתיימת בקישור https://example.com/a/long/path/here",
];

/// The width paragraphs wrap at, about twelve characters of 16 px text.
pub const WRAP_WIDTH: f32 = 130.;

/// Wrapping a paragraph that holds right-to-left text: rows follow the text (the first
/// row holds its first characters), each row lies in visual order for the paragraph's
/// direction, and a caret placed at a row's offsets lands on that row.
pub fn wrap_violations(system: Arc<dyn PlatformTextSystem>, family: &str) -> Vec<String> {
    let text_system = WindowTextSystem::new(Arc::new(TextSystem::new(system)));
    let font = gpui::font(family);
    let mut found = Vec::new();
    for text in WRAP_CASES {
        let runs = [TextRun {
            len: text.len(),
            font: font.clone(),
            color: gpui::black(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }];
        let lines = match text_system.shape_text(
            SharedString::from(*text),
            px(16.),
            &runs,
            Some(px(WRAP_WIDTH)),
            None,
        ) {
            Ok(lines) => lines,
            Err(error) => {
                found.push(format!("{text:?}: {error}"));
                continue;
            }
        };
        let Some(line) = lines.first() else {
            found.push(format!("{text:?}: no line"));
            continue;
        };
        let rows = gpui::wrapped_rows(&line.unwrapped_layout, &line.wrap_boundaries);
        if rows.len() < 2 {
            found.push(format!("{text:?}: did not wrap ({} row)", rows.len()));
            continue;
        }
        let rtl = unicode_bidi::get_base_direction_full(*text) == unicode_bidi::Direction::Rtl;
        let mut start = 0;
        for (ix, row) in rows.iter().enumerate() {
            let end = row.len;
            if end <= start || end > text.len() {
                found.push(format!("{text:?}: row {ix} covers {start}..{end}"));
                break;
            }
            let indices: Vec<usize> = row
                .runs
                .iter()
                .flat_map(|run| &run.glyphs)
                .map(|glyph| glyph.index)
                .collect();
            if let Some(outside) = indices.iter().find(|i| **i < start || **i >= end) {
                found.push(format!(
                    "{text:?}: row {ix} ({start}..{end}) holds byte {outside}"
                ));
            }
            let row_text = text.get(start..end).unwrap_or_default();
            let expected: Vec<usize> = visual_order_with(row_text, rtl)
                .into_iter()
                .map(|i| i + start)
                .filter(|i| compared(text, *i))
                .collect();
            let mut seen: Vec<usize> = Vec::new();
            for index in indices.into_iter().filter(|i| compared(text, *i)) {
                if seen.last() != Some(&index) {
                    seen.push(index);
                }
            }
            if seen != expected {
                found.push(format!(
                    "{text:?}: row {ix} glyphs {seen:?}, expected {expected:?}"
                ));
            }
            start = end;
        }
        if start != text.len() {
            found.push(format!("{text:?}: rows end at {start} of {}", text.len()));
        }
        // The caret at the paragraph's start sits on the first row, at its start edge.
        match line.position_for_index(0, px(20.)) {
            Some(at) if at.y == px(0.) => {
                let width = rows.first().map_or(px(0.), |row| row.width);
                let start_edge = if rtl { width } else { px(0.) };
                if (at.x - start_edge).abs() > px(1.) {
                    found.push(format!(
                        "{text:?}: caret 0 at x {:?}, expected {start_edge:?}",
                        at.x
                    ));
                }
            }
            other => found.push(format!("{text:?}: caret 0 at {other:?}")),
        }
    }
    found
}

/// [`visual_order`] with the paragraph direction given rather than found.
fn visual_order_with(text: &str, rtl: bool) -> Vec<usize> {
    let level = if rtl { Level::rtl() } else { Level::ltr() };
    let info = BidiInfo::new(text, Some(level));
    let mut order = Vec::with_capacity(text.len());
    for paragraph in &info.paragraphs {
        let (levels, runs) = info.visual_runs(paragraph, paragraph.range.clone());
        for run in runs {
            let starts = char_starts(text, run.clone());
            if levels.get(run.start).is_some_and(|level| level.is_rtl()) {
                order.extend(starts.into_iter().rev());
            } else {
                order.extend(starts);
            }
        }
    }
    order
}
