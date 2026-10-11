use super::{
    Face, PRE_BLOCK_CHROME, QUOTE_BLOCK_CHROME, line_widths, longest_with_extra, widest_pre_block,
};
use quill::text::{TextEntity, TextEntityKind, styled_runs};

fn entity(text: &str, needle: &str, kind: TextEntityKind) -> TextEntity {
    let start = text.find(needle).expect("needle");
    TextEntity {
        utf8_start: start,
        utf8_end: start + needle.len(),
        kind,
    }
}

/// Regular glyphs 1 px, bold 2 px, italic 1.5 px, monospace 3 px.
fn advance(face: Face, _: char) -> f32 {
    if face.mono {
        3.
    } else if face.bold {
        2.
    } else if face.italic {
        1.5
    } else {
        1.
    }
}

#[test]
fn bold_runs_are_measured_in_the_bold_face() {
    let text = "ab bold cd";
    let runs = styled_runs(text, &[entity(text, "bold", TextEntityKind::Bold)]);
    let (lines, ends_in_block) = line_widths(&runs, advance, 10.);
    // "ab " + "bold" twice as wide + " cd".
    assert_eq!(lines, vec![3. + 8. + 3.]);
    assert!(!ends_in_block);
    let italic = styled_runs(text, &[entity(text, "bold", TextEntityKind::Italic)]);
    assert_eq!(line_widths(&italic, advance, 10.).0, vec![12.]);
}

#[test]
fn a_custom_emoji_takes_its_placeholder_not_its_text() {
    let text = "hi 👍👍 there";
    let runs = styled_runs(
        text,
        &[entity(
            text,
            "👍👍",
            TextEntityKind::CustomEmoji { custom_emoji_id: 7 },
        )],
    );
    // "hi " (3) + one 20 px emoji + " there" (6).
    assert_eq!(line_widths(&runs, advance, 20.).0, vec![29.]);
}

#[test]
fn newlines_split_lines_and_the_skip_block_joins_the_last() {
    let text = "a long first line\nshort";
    let runs = styled_runs(text, &[]);
    let (lines, ends_in_block) = line_widths(&runs, advance, 10.);
    assert_eq!(lines, vec![17., 5.]);
    // The time's skip block sits on the last line only.
    assert_eq!(longest_with_extra(&lines, ends_in_block, 40), 45);
    assert_eq!(longest_with_extra(&lines, ends_in_block, 0), 17);
    assert_eq!(longest_with_extra(&[], false, 40), 0);
}

#[test]
fn blocks_take_their_chrome_and_the_time_a_line_after_them() {
    let text = "Code:\nfn main() {}";
    let runs = styled_runs(text, &[entity(text, "fn main() {}", TextEntityKind::Pre)]);
    let (lines, ends_in_block) = line_widths(&runs, advance, 10.);
    // "Code:" closes at its newline; the inline paragraph's empty tail and
    // the pre block follow.
    assert_eq!(lines[0], 5.);
    assert_eq!(*lines.last().unwrap(), 36. + PRE_BLOCK_CHROME as f32);
    assert!(ends_in_block);
    // After a closing block the time takes a line of its own: 40 < 76.
    assert_eq!(longest_with_extra(&lines, ends_in_block, 40), 76);
    let quoted = "said: wise words";
    let runs = styled_runs(
        quoted,
        &[entity(quoted, "wise words", TextEntityKind::BlockQuote)],
    );
    let (lines, _) = line_widths(&runs, advance, 10.);
    assert_eq!(lines, vec![6., 10. + QUOTE_BLOCK_CHROME as f32]);
}

#[test]
fn the_widest_pre_block_counts_its_longest_line_and_chrome() {
    let text = "see\nshort\na much longer code line\n";
    let code = "short\na much longer code line\n";
    let runs = styled_runs(
        text,
        &[entity(
            text,
            code,
            TextEntityKind::PreCode {
                language: "rust".into(),
            },
        )],
    );
    // 23 monospace glyphs of 3 px.
    assert_eq!(widest_pre_block(&runs, advance, 0.8), 69 + PRE_BLOCK_CHROME);
    // No block: nothing to widen.
    assert_eq!(
        widest_pre_block(&styled_runs("plain", &[]), advance, 0.8),
        0
    );
    // A long language label wins over short code.
    let tiny = "x";
    let runs = styled_runs(
        tiny,
        &[entity(
            tiny,
            "x",
            TextEntityKind::PreCode {
                language: "typescript-react".into(),
            },
        )],
    );
    // 16 glyphs at 3 px, drawn at 0.8: 38.4, rounded up with the chrome.
    assert_eq!(widest_pre_block(&runs, advance, 0.8), 79);
}
