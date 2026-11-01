//! Characters dropped before shaping leave every style and click range on
//! the text it belongs to.
use super::paragraph_text;
use quill::text::{LinkTarget, RunStyle, TextRun};
use std::collections::{HashMap, HashSet};

fn run(text: &str, style: RunStyle, link: Option<LinkTarget>) -> TextRun {
    TextRun {
        text: text.into(),
        href: None,
        link,
        style,
        custom_emoji_id: None,
    }
}

#[test]
fn zalgo_marks_and_stray_tags_go_and_ranges_stay_on_their_text() {
    let bold = RunStyle {
        bold: true,
        ..RunStyle::default()
    };
    let link = LinkTarget::Url {
        url: "https://example.com".into(),
        label: None,
    };
    let runs = [
        run(
            &format!("He{}y \u{1F4DA}\u{E0069}\u{E007F}", "\u{0300}".repeat(40)),
            RunStyle::default(),
            None,
        ),
        run("bo\u{0301}\u{0302}\u{0303}ld", bold, None),
        run(" go\u{200D}\u{2028}", RunStyle::default(), Some(link)),
    ];
    let shaped = paragraph_text(
        &runs,
        0,
        (1, 1),
        false,
        &HashSet::new(),
        &HashMap::new(),
        &HashMap::new(),
    );
    assert_eq!(
        shaped.text,
        "He\u{0300}\u{0300}y \u{1F4DA}bo\u{0301}\u{0302}ld go\u{200D}"
    );
    let bold_range = shaped.highlights.first().map(|(r, _)| r.clone());
    let bold_text = bold_range.and_then(|r| shaped.text.get(r));
    assert_eq!(bold_text, Some("bo\u{0301}\u{0302}ld"));
    let link_text = shaped
        .click_ranges
        .first()
        .and_then(|r| shaped.text.get(r.clone()));
    assert_eq!(link_text, Some(" go\u{200D}"));
}
