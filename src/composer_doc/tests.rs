use super::*;

fn doc(text: &str, spans: &[(&str, ComposerTag)]) -> ComposerDoc {
    let mut doc = ComposerDoc::plain(text);
    for (part, tag) in spans {
        let start = text.find(part).expect("part in text");
        doc.spans
            .push(DocSpan::new(start..start + part.len(), tag.clone()));
    }
    doc.normalize();
    doc
}

/// The document survives markup both ways, and the markup reads as
/// expected.
fn round_trip(doc: &ComposerDoc, markup: &str) {
    assert_eq!(doc.to_markup(), markup, "markup of {doc:?}");
    assert_eq!(
        &ComposerDoc::from_markup(markup),
        doc,
        "document of {markup}"
    );
}

#[test]
fn every_tag_round_trips_through_markup() {
    use ComposerTag as T;
    let cases: Vec<(ComposerDoc, &str)> = vec![
        (doc("a bold b", &[("bold", T::Bold)]), "a **bold** b"),
        (doc("a it b", &[("it", T::Italic)]), "a *it* b"),
        (doc("a un b", &[("un", T::Underline)]), "a __un__ b"),
        (doc("a st b", &[("st", T::Strikethrough)]), "a ~~st~~ b"),
        (doc("a sp b", &[("sp", T::Spoiler)]), "a ||sp|| b"),
        (doc("a co b", &[("co", T::Code)]), "a `co` b"),
        (
            doc("see docs", &[("docs", T::Link("https://t.me".into()))]),
            "see [docs](https://t.me)",
        ),
        (
            doc("hi Ann!", &[("Ann", T::Mention(42))]),
            "hi [Ann](tg://user?id=42)!",
        ),
        (
            doc("x\nlet a;\ny", &[("let a;", T::Pre("rust".into()))]),
            "x\n```rust\nlet a;\n```\ny",
        ),
        (doc("quoted", &[("quoted", T::Quote)]), "> quoted"),
    ];
    for (doc, markup) in cases {
        round_trip(&doc, markup);
    }
}

#[test]
fn nested_and_overlapping_spans_round_trip() {
    use ComposerTag as T;
    // Italic inside bold uses `_`, so `**` is never read into it.
    round_trip(
        &doc(
            "a very bold b",
            &[("very bold", T::Bold), ("bold", T::Italic)],
        ),
        "a **very _bold_** b",
    );
    // Bold inside a link label.
    round_trip(
        &doc(
            "go home now",
            &[
                ("home now", T::Link("https://x.y".into())),
                ("home", T::Bold),
            ],
        ),
        "go [**home** now](https://x.y)",
    );
    // Crossing spans: the inner one is closed and reopened, the text
    // and formatting per character are kept.
    let crossing = doc("abcdef", &[("abcd", T::Bold), ("cdef", T::Strikethrough)]);
    let markup = crossing.to_markup();
    assert_eq!(markup, "**ab~~cd~~**~~ef~~");
    assert_eq!(ComposerDoc::from_markup(&markup), crossing);
}

#[test]
fn a_quote_over_several_lines_is_one_span() {
    use ComposerTag as T;
    round_trip(
        &doc("one\ntwo\nafter", &[("one\ntwo", T::Quote)]),
        "> one\n> two\nafter",
    );
    // With bold inside a quoted line.
    round_trip(
        &doc("a b", &[("a b", T::Quote), ("b", T::Bold)]),
        "> a **b**",
    );
}

#[test]
fn spans_across_lines_split_at_the_line_break() {
    let doc = doc("ab\ncd", &[("ab\ncd", ComposerTag::Bold)]);
    let markup = doc.to_markup();
    assert_eq!(markup, "**ab**\n**cd**");
    let back = ComposerDoc::from_markup(&markup);
    assert_eq!(back.text, "ab\ncd");
    assert_eq!(
        back.spans,
        [
            DocSpan::new(0..2, ComposerTag::Bold),
            DocSpan::new(3..5, ComposerTag::Bold)
        ]
    );
}

#[test]
fn custom_emoji_round_trip_inside_formatting() {
    let mut d = doc("a 🔠 b", &[("a 🔠 b", ComposerTag::Bold)]);
    d.emoji.push(DocEmoji {
        range: 2..6,
        custom_emoji_id: 7,
    });
    round_trip(&d, "**a ![🔠](tg://emoji?id=7) b**");
}

#[test]
fn formatting_inside_code_is_dropped() {
    let d = doc(
        "x code y",
        &[("code", ComposerTag::Code), ("od", ComposerTag::Bold)],
    );
    assert_eq!(d.to_markup(), "x `code` y");
    // Formatting around code keeps it.
    let d = doc(
        "x code y",
        &[("x code y", ComposerTag::Bold), ("code", ComposerTag::Code)],
    );
    round_trip(&d, "**x `code` y**");
}

#[test]
fn markup_and_entities_round_trip_through_the_document() {
    // markup -> document -> TDLib entities -> document -> markup.
    let markup = "hi **bold _it_** [Ann](tg://user?id=5) ![😀](tg://emoji?id=9)\n> q";
    let doc = ComposerDoc::from_markup(markup);
    let entities: Vec<TextEntity> = doc
        .spans
        .iter()
        .map(|span| TextEntity {
            utf8_start: span.range.start,
            utf8_end: span.range.end,
            kind: match &span.tag {
                ComposerTag::Bold => TextEntityKind::Bold,
                ComposerTag::Italic => TextEntityKind::Italic,
                ComposerTag::Mention(user_id) => TextEntityKind::MentionName { user_id: *user_id },
                ComposerTag::Quote => TextEntityKind::BlockQuote,
                other => panic!("unexpected {other:?}"),
            },
        })
        .chain(doc.emoji.iter().map(|e| TextEntity {
            utf8_start: e.range.start,
            utf8_end: e.range.end,
            kind: TextEntityKind::CustomEmoji {
                custom_emoji_id: e.custom_emoji_id,
            },
        }))
        .collect();
    let back = ComposerDoc::from_entities(&doc.text, &entities);
    assert_eq!(back, doc);
    assert_eq!(back.to_markup(), markup);
    // And the send path sees a mention-name entity.
    let (_, parsed) = parse_format_markup(markup);
    assert!(parsed.iter().any(|e| e.kind == FormatKind::MentionName));
}

#[test]
fn toggling_adds_then_removes_and_respects_conflicts() {
    let text = "one two three";
    let spans = toggle_tag(text, &[], 4..7, ComposerTag::Bold);
    assert_eq!(spans, [DocSpan::new(4..7, ComposerTag::Bold)]);
    // Over a wider selection that is only partly bold: add.
    let spans = toggle_tag(text, &spans, 0..7, ComposerTag::Bold);
    assert_eq!(spans, [DocSpan::new(0..7, ComposerTag::Bold)]);
    // Fully bold: remove from the selection only.
    let spans = toggle_tag(text, &spans, 0..3, ComposerTag::Bold);
    assert_eq!(spans, [DocSpan::new(3..7, ComposerTag::Bold)]);
    // Code clears other inline formatting under it.
    let spans = toggle_tag(text, &spans, 4..13, ComposerTag::Code);
    assert_eq!(
        spans,
        [
            DocSpan::new(3..4, ComposerTag::Bold),
            DocSpan::new(4..13, ComposerTag::Code)
        ]
    );
    // A quote takes whole lines.
    let text = "ab\ncd\nef";
    let spans = toggle_tag(text, &[], 4..4, ComposerTag::Quote);
    assert_eq!(spans, [DocSpan::new(3..5, ComposerTag::Quote)]);
    // Clearing removes everything in the range.
    let spans = vec![
        DocSpan::new(0..8, ComposerTag::Italic),
        DocSpan::new(3..5, ComposerTag::Link("u".into())),
    ];
    assert_eq!(
        clear_tags(&spans, &(2..6)),
        [
            DocSpan::new(0..2, ComposerTag::Italic),
            DocSpan::new(6..8, ComposerTag::Italic)
        ]
    );
}

#[test]
fn typed_markdown_turns_into_formatting_when_closed() {
    let check = |text: &str| markdown_replacement(text, text.len(), &[]);
    let r = check("say **hi**").unwrap();
    assert_eq!((r.outer, r.inner, r.tag), (4..10, 6..8, ComposerTag::Bold));
    let r = check("an *it*").unwrap();
    assert_eq!(r.tag, ComposerTag::Italic);
    assert_eq!(check("x ~~gone~~").unwrap().tag, ComposerTag::Strikethrough);
    assert_eq!(check("x ||hide||").unwrap().tag, ComposerTag::Spoiler);
    assert_eq!(check("x `code`").unwrap().tag, ComposerTag::Code);
    assert_eq!(check("x __u__").unwrap().tag, ComposerTag::Underline);
    assert_eq!(
        check("```let a```").unwrap().tag,
        ComposerTag::Pre(String::new())
    );
    // Not yet closed, or not formatting.
    assert_eq!(check("say **hi*"), None);
    assert_eq!(check("say ** hi**"), None);
    assert_eq!(check("say **hi **"), None);
    assert_eq!(check("2*3*"), None);
    assert_eq!(check("snake_case_"), None);
    assert_eq!(check("https://x.org/__a__"), None);
    assert_eq!(check("a `b *c*"), None);
    assert_eq!(check("**\nx**"), None);
    // Not at the end of the text: the caret decides.
    let text = "a *b* c";
    assert!(markdown_replacement(text, 5, &[]).is_some());
    assert_eq!(markdown_replacement(text, 7, &[]), None);
    // Nothing inside text already formatted as code.
    assert_eq!(
        markdown_replacement("x *y*", 5, std::slice::from_ref(&(0..5))),
        None
    );
}

#[test]
fn tags_and_has_tag_follow_the_spans() {
    let spans = vec![DocSpan::new(0..3, ComposerTag::Bold)];
    assert_eq!(tags_at(&spans, 3), [ComposerTag::Bold]);
    assert!(tags_at(&spans, 0).is_empty());
    assert!(has_tag("abc d", &spans, &(0..4), &ComposerTag::Bold));
    assert!(!has_tag("abc d", &spans, &(0..5), &ComposerTag::Bold));
    assert!(!has_tag("   ", &[], &(0..3), &ComposerTag::Bold));
}

#[test]
fn tags_survive_their_string_form() {
    for tag in [
        ComposerTag::Bold,
        ComposerTag::Pre("rust".into()),
        ComposerTag::Pre(String::new()),
        ComposerTag::Link("https://a.b/c?d=e:f".into()),
        ComposerTag::Mention(77),
    ] {
        assert_eq!(ComposerTag::from_tag(&tag.to_tag()), Some(tag));
    }
    assert_eq!(ComposerTag::from_tag("mention:x"), None);
    assert_eq!(ComposerTag::from_tag("nope"), None);
}
