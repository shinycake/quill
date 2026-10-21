//! The word-boundary rule through the parser, the clearing code and the
//! markup writer.

use crate::composer::{FormatKind, clear_format_markup, parse_format_markup};
use crate::composer_doc::{ComposerDoc, ComposerTag, DocSpan};

fn kinds(markup: &str) -> (String, Vec<(FormatKind, i32, i32)>) {
    let (text, entities) = parse_format_markup(markup);
    let spans = entities
        .iter()
        .map(|e| (e.kind, e.offset, e.length))
        .collect();
    (text, spans)
}

#[test]
fn markers_inside_a_word_stay_plain_text() {
    for plain in [
        "my_cmd_x",
        "a_b_c",
        "**x**y",
        "a**x**",
        "snake_case_name",
        "x~~y~~ z",
        "a||b||c",
        "a`b`c",
        "_a_b_c",
        "2*3*4",
    ] {
        assert_eq!(kinds(plain), (plain.to_string(), Vec::new()), "{plain}");
    }
}

#[test]
fn markers_at_word_edges_still_format() {
    assert_eq!(kinds("_a_"), ("a".into(), vec![(FormatKind::Italic, 0, 1)]));
    assert_eq!(
        kinds("go _a_ now"),
        ("go a now".into(), vec![(FormatKind::Italic, 3, 1)])
    );
    assert_eq!(
        kinds("(**x**), ok"),
        ("(x), ok".into(), vec![(FormatKind::Bold, 1, 1)])
    );
    assert_eq!(
        kinds("**x**\n`y`"),
        (
            "x\ny".into(),
            vec![(FormatKind::Bold, 0, 1), (FormatKind::Code, 2, 1)]
        )
    );
}

#[test]
fn a_closing_strike_may_touch_a_word() {
    assert_eq!(
        kinds("~~a~~b"),
        ("ab".into(), vec![(FormatKind::Strikethrough, 0, 1)])
    );
    assert_eq!(kinds("a~~b~~"), ("a~~b~~".into(), Vec::new()));
}

#[test]
fn a_closer_inside_a_word_is_skipped_for_the_next_one() {
    assert_eq!(
        kinds("**x**y**"),
        ("x**y".into(), vec![(FormatKind::Bold, 0, 4)])
    );
}

#[test]
fn nested_and_adjacent_spans_read_back() {
    let (text, mut spans) = kinds("**_x_**");
    spans.sort_by_key(|span| format!("{:?}", span.0));
    assert_eq!(text, "x");
    assert_eq!(
        spans,
        vec![(FormatKind::Bold, 0, 1), (FormatKind::Italic, 0, 1)]
    );
    assert_eq!(
        kinds("**a**_b_"),
        (
            "ab".into(),
            vec![(FormatKind::Bold, 0, 1), (FormatKind::Italic, 1, 1)]
        )
    );
    assert_eq!(
        kinds("[**x**](https://t.me)"),
        (
            "x".into(),
            vec![(FormatKind::Bold, 0, 1), (FormatKind::TextUrl, 0, 1)]
        )
    );
}

#[test]
fn clearing_formatting_leaves_words_alone() {
    assert_eq!(
        clear_format_markup("my_cmd_x and _a_", 0..0),
        "my_cmd_x and a"
    );
    assert_eq!(clear_format_markup("**x**y", 0..0), "**x**y");
}

#[test]
fn a_span_inside_a_word_survives_the_round_trip() {
    let mut doc = ComposerDoc::plain("hello world");
    doc.spans.push(DocSpan::new(1..4, ComposerTag::Bold));
    doc.spans.push(DocSpan::new(7..9, ComposerTag::Italic));
    let markup = doc.to_markup();
    assert_eq!(markup, "h\u{10}**ell**\u{10}o w\u{10}*or*\u{10}ld");
    let back = ComposerDoc::from_markup(&markup);
    assert_eq!(back.text, "hello world");
    assert_eq!(back.spans, doc.spans);
}

#[test]
fn a_span_on_whole_words_needs_no_barrier() {
    let mut doc = ComposerDoc::plain("say hello, world");
    doc.spans.push(DocSpan::new(4..9, ComposerTag::Bold));
    assert_eq!(doc.to_markup(), "say **hello**, world");
}

#[test]
fn plain_text_with_marker_characters_is_not_formatted_on_send() {
    let doc = ComposerDoc::plain("run my_cmd_x now");
    let (text, entities) = parse_format_markup(&doc.to_markup());
    assert_eq!(text, "run my_cmd_x now");
    assert!(entities.is_empty());
}

#[test]
fn a_barrier_never_reaches_the_parsed_text() {
    let (text, entities) = parse_format_markup("`a\u{10}b` [la\u{10}bel](https://x.org)");
    assert_eq!(text, "ab label");
    assert_eq!(entities.len(), 2);
}

#[test]
fn clearing_formatting_drops_barriers() {
    assert_eq!(clear_format_markup("h\u{10}**ell**\u{10}o", 0..0), "hello");
}
