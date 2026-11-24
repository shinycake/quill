use super::*;
use serde_json::json;

fn server_draft(blocks: Value) -> RichDraft {
    let (blocks, _) = parse_rich_message(&json!({"@type": "richMessage", "blocks": blocks}));
    RichDraft::from_message(&blocks).expect("a visible block")
}

fn paragraph(text: Value) -> Value {
    json!({"@type": "pageBlockParagraph", "text": text})
}

#[test]
fn a_draft_from_tdlib_keeps_its_inline_formatting_through_the_editor() {
    let draft = server_draft(json!([
        {"@type": "pageBlockSectionHeading", "size": 1, "text": {"@type": "richTextPlain", "text": "Plan"}},
        paragraph(json!({"@type": "richTexts", "texts": [
            {"@type": "richTextPlain", "text": "ship "},
            {"@type": "richTextBold", "text": {"@type": "richTextPlain", "text": "today"}},
        ]})),
    ]));
    let markup = draft.markup();
    assert!(markup.starts_with("# Plan"), "{markup}");
    let preview = draft.preview_blocks();
    let Some(RichBlock::Paragraph { text, entities, .. }) = preview.get(1) else {
        panic!("a paragraph: {preview:?}");
    };
    assert_eq!(text, "ship today");
    assert!(
        entities
            .iter()
            .any(|entity| entity.kind == TextEntityKind::Bold),
        "{entities:?}"
    );
    // Saving it back sends the same structure.
    let input = draft.input().expect("every block has an input form");
    let blocks = input["source"]["blocks"].as_array().expect("blocks");
    assert_eq!(blocks[0]["@type"], "inputPageBlockSectionHeading");
    assert_eq!(blocks[1]["text"]["texts"][1]["@type"], "richTextBold");
}

fn list_item(blocks: Value) -> Value {
    json!({"@type": "pageBlockListItem", "label": "", "blocks": blocks,
        "has_checkbox": false, "is_checked": false})
}

fn bold(text: &str) -> Value {
    json!({"@type": "richTextBold", "text": {"@type": "richTextPlain", "text": text}})
}

#[test]
fn a_received_list_item_keeps_its_formatting_across_its_blocks() {
    let RichBlock::List { ordered, items } = parse_page_block(&json!({
        "@type": "pageBlockList",
        "items": [{
            "@type": "pageBlockListItem", "label": "3.", "value": 3, "type": "1", "has_checkbox": false, "is_checked": false,
            "blocks": [
                paragraph(json!({"@type": "richTexts", "texts": [
                    {"@type": "richTextPlain", "text": "日本 "}, bold("first"),
                ]})),
                paragraph(json!({"@type": "richTextFixed",
                    "text": {"@type": "richTextPlain", "text": "second"}})),
            ],
        }],
    })) else {
        panic!("a list");
    };
    assert!(ordered);
    let item = items.first().expect("an item");
    assert_eq!(item.label, "3.");
    assert_eq!(item.text, "日本 first\nsecond");
    let styled: Vec<(&str, &TextEntityKind)> = item
        .entities
        .iter()
        .map(|entity| {
            let text = item.text.get(entity.utf8_start..entity.utf8_end);
            (text.expect("a range of the text"), &entity.kind)
        })
        .collect();
    assert_eq!(
        styled,
        [
            ("first", &TextEntityKind::Bold),
            ("second", &TextEntityKind::Code)
        ]
    );
}

#[test]
fn a_nested_list_keeps_its_formatting_in_the_item_that_holds_it() {
    let RichBlock::List { items, .. } = parse_page_block(&json!({
        "@type": "pageBlockList",
        "items": [list_item(json!([
            paragraph(json!({"@type": "richTextPlain", "text": "outer"})),
            {"@type": "pageBlockList", "items": [
                list_item(json!([paragraph(json!({"@type": "richTextPlain", "text": "a"}))])),
                list_item(json!([paragraph(bold("b"))])),
            ]},
        ]))],
    })) else {
        panic!("a list");
    };
    let item = items.first().expect("an item");
    assert_eq!(item.text, "outer\na\nb");
    let [entity] = item.entities.as_slice() else {
        panic!("one span: {:?}", item.entities);
    };
    assert_eq!(item.text.get(entity.utf8_start..entity.utf8_end), Some("b"));
}

#[test]
fn a_typed_ordered_list_sends_its_numbers() {
    let input = input_rich_message(&markup_to_blocks("1. a\n2. b")).expect("a message");
    let items = &input["source"]["blocks"][0]["items"];
    assert_eq!(items[1]["value"], 2);
    assert_eq!(items[1]["type"], "1");
    let input = input_rich_message(&markup_to_blocks("- a")).expect("a message");
    let item = &input["source"]["blocks"][0]["items"][0];
    assert_eq!(item["value"], 0);
    assert_eq!(item["type"], "");
}

#[test]
fn a_typed_list_item_sends_its_formatting() {
    let blocks = markup_to_blocks("- **a** b");
    let input = input_rich_message(&blocks).expect("a message");
    let item = &input["source"]["blocks"][0]["items"][0]["blocks"][0]["text"];
    assert_eq!(item["texts"][0]["@type"], "richTextBold");
    assert_eq!(item["texts"][0]["text"]["text"], "a");
}

#[test]
fn a_list_from_tdlib_keeps_its_formatting_through_the_editor() {
    let draft = server_draft(json!([{"@type": "pageBlockList", "items": [
        list_item(json!([paragraph(json!({"@type": "richTexts", "texts": [
            bold("buy"),
            {"@type": "richTextPlain", "text": " 2*3*4 milk"},
        ]}))])),
    ]}]));
    let input = draft.input().expect("every block has an input form");
    let texts = &input["source"]["blocks"][0]["items"][0]["blocks"][0]["text"]["texts"];
    assert_eq!(texts[0]["@type"], "richTextBold");
    assert_eq!(texts[0]["text"]["text"], "buy");
    assert_eq!(texts[1]["text"], " 2*3*4 milk");

    // The editor's markup is what reopens and sends the draft.
    let reopened = RichDraft::from_markup(&draft.markup()).expect("a list");
    assert_eq!(reopened.input(), draft.input());
    let Some(RichBlock::List { items, .. }) = reopened.preview_blocks().into_iter().next() else {
        panic!("a list");
    };
    let item = items.first().expect("an item");
    assert_eq!(item.text, "buy 2*3*4 milk");
    assert_eq!(item.entities.len(), 1);
}

fn styled(parts: Vec<Value>) -> Value {
    json!({ "@type": "richTexts", "texts": parts })
}

fn plain(text: &str) -> Value {
    json!({ "@type": "richTextPlain", "text": text })
}

fn bold_part(text: &str, entities: &[TextEntity]) -> Vec<String> {
    entities
        .iter()
        .filter(|entity| matches!(entity.kind, TextEntityKind::Bold))
        .filter_map(|entity| text.get(entity.utf8_start..entity.utf8_end))
        .map(str::to_string)
        .collect()
}

#[test]
fn a_block_quote_keeps_its_formatting_behind_the_quote_mark() {
    let block = parse_page_block(&json!({
        "@type": "pageBlockBlockQuote",
        "blocks": [
            paragraph(styled(vec![plain("one "), bold("two")])),
            paragraph(styled(vec![bold("three"), plain(" four")])),
        ],
    }));
    let RichBlock::Paragraph { text, entities, .. } = block else {
        panic!("a quote is a paragraph");
    };
    assert_eq!(text, "❝ one two\n❝ three four");
    assert_eq!(bold_part(&text, &entities), ["two", "three"]);
}

#[test]
fn an_expandable_quote_keeps_header_and_body_formatting() {
    let block = parse_page_block(&json!({
        "@type": "pageBlockExpandableBlockQuote",
        "blocks": [
            {"@type": "pageBlockParagraph", "text": styled(vec![bold("head")])},
            {"@type": "pageBlockParagraph", "text": styled(vec![plain("body "), bold("end")])},
        ],
    }));
    let RichBlock::Collapsible {
        header,
        header_entities,
        body,
        body_entities,
        ..
    } = block
    else {
        panic!("an expandable quote is collapsible");
    };
    assert_eq!((header.as_str(), body.as_str()), ("head", "body end"));
    assert_eq!(bold_part(&header, &header_entities), ["head"]);
    assert_eq!(bold_part(&body, &body_entities), ["end"]);
}

#[test]
fn details_keep_their_formatting_through_the_editor() {
    let received = parse_page_block(&json!({
        "@type": "pageBlockDetails",
        "header": styled(vec![plain("see "), bold("this")]),
        "blocks": [paragraph(styled(vec![plain("the "), bold("body")]))],
        "is_open": true,
    }));
    let RichBlock::Collapsible {
        header,
        header_entities,
        body,
        body_entities,
        ..
    } = &received
    else {
        panic!("details are collapsible");
    };
    assert_eq!(bold_part(header, header_entities), ["this"]);
    assert_eq!(bold_part(body, body_entities), ["body"]);

    let Some(draft) = RichDraft::from_message(std::slice::from_ref(&received)) else {
        panic!("the block shows");
    };
    let shown = draft.preview_blocks();
    assert_eq!(shown.first(), Some(&received), "edits as it was received");
    let input = draft.input().expect("details have an input form");
    let block = &input["source"]["blocks"][0];
    assert_eq!(block["header"]["texts"][1]["@type"], "richTextBold");
    assert_eq!(
        block["blocks"][0]["text"]["texts"][1]["@type"],
        "richTextBold"
    );
}

#[test]
fn a_task_list_keeps_its_checkboxes_through_the_editor() {
    let draft = server_draft(json!([{"@type": "pageBlockList", "items": [
        {"@type": "pageBlockListItem", "label": "", "blocks": [
            paragraph(json!({"@type": "richTextPlain", "text": "buy milk"}))
        ], "has_checkbox": true, "is_checked": true}
    ]}]));
    assert_eq!(draft.markup(), "- [x] buy milk");
    assert_eq!(RichDraft::from_markup(&draft.markup()), Some(draft));
}

#[test]
fn editor_markup_saves_rich_only_with_structure() {
    assert_eq!(RichDraft::from_markup("just **text**\n\nmore"), None);
    assert_eq!(RichDraft::from_markup("   "), None);
    let draft = RichDraft::from_markup("# Title\n\nbody").expect("a heading is rich");
    assert_eq!(draft.markup(), "# Title\n\nbody");
    assert!(draft.input().is_some());
}

#[test]
fn media_from_another_app_is_never_dropped_by_a_resave() {
    let draft = server_draft(json!([
        paragraph(json!({"@type": "richTextPlain", "text": "look"})),
        {"@type": "pageBlockPhoto", "photo": null, "caption": {"@type": "pageBlockCaption", "text": {"@type": "richTextPlain", "text": "sunset"}, "credit": {"@type": "richTextPlain", "text": ""}}, "url": ""},
    ]));
    assert_eq!(draft.input(), None);
    assert_eq!(draft.plain_markup(), None);
    assert_eq!(draft.summary(), "look\nsunset");
}

#[test]
fn premium_formatting_is_anything_past_plain_paragraphs() {
    let plain = server_draft(json!([paragraph(
        json!({"@type": "richTextPlain", "text": "hi"})
    )]));
    assert!(!plain.uses_premium_formatting());
    let bold = server_draft(json!([paragraph(
        json!({"@type": "richTextBold", "text": {"@type": "richTextPlain", "text": "hi"}})
    )]));
    assert!(bold.uses_premium_formatting());
    let list = RichDraft::from_markup("- [ ] task").expect("a task list");
    assert!(list.uses_premium_formatting());
}

#[test]
fn flattening_keeps_inline_formatting_and_writes_structure_as_lines() {
    let draft = RichDraft::from_markup("# Title\n\nbody **bold**\n\n- [x] done\n- [ ] next\n\n---")
        .expect("rich");
    assert_eq!(
        draft.plain_markup().as_deref(),
        Some("Title\nbody **bold**\n[x] done\n[ ] next")
    );
    let table = server_draft(
        json!([{"@type": "pageBlockTable", "caption": {"@type": "richTextPlain", "text": ""}, "cells": [[
            {"@type": "pageBlockTableCell", "text": {"@type": "richTextPlain", "text": "a"}}
        ]]}]),
    );
    assert_eq!(table.plain_markup(), None);
    assert_eq!(table.summary(), "a");
}

#[test]
fn pasted_markdown_structure_is_headings_dividers_and_tasks() {
    assert!(text_has_markdown_structure("# Notes\nsome text"));
    assert!(text_has_markdown_structure("one\n---\ntwo"));
    assert!(text_has_markdown_structure("- [ ] buy milk"));
    assert!(!text_has_markdown_structure("- milk\n- eggs"));
    assert!(!text_has_markdown_structure("> quoted\n**bold** text"));
    assert!(!text_has_markdown_structure(
        &"# x\n".repeat(RICH_TEXT_MAX_CHARS)
    ));
}

#[test]
fn a_code_block_with_a_comment_line_survives_the_editor_markup() {
    use crate::text::{TextEntity, TextEntityKind};
    let code = RichBlock::Paragraph {
        text: "# setup\nx = 1".into(),
        entities: vec![TextEntity {
            utf8_start: 0,
            utf8_end: 13,
            kind: TextEntityKind::PreCode {
                language: "python".into(),
            },
        }],
        buttons: Vec::new(),
    };
    let draft = RichDraft::from_message(&[code, RichBlock::Divider]).expect("visible");
    let again = RichDraft::from_markup(&draft.markup()).expect("structured");
    assert_eq!(again.blocks(), draft.blocks());
}

#[test]
fn a_blank_line_stays_inside_a_code_block() {
    let text = "```\na\n\n- b\n```\n---";
    let blocks = markup_to_blocks(text);
    assert_eq!(blocks.len(), 2, "{blocks:?}");
    assert!(matches!(&blocks[0], RichBlock::Paragraph { text, .. } if text.contains("\n\n- b")));
    let mut parsed = MarkupBlocks::default();
    let shared: Vec<RichBlock> = parsed.parse(text).iter().map(|b| (**b).clone()).collect();
    assert_eq!(shared, blocks);
}

#[test]
fn an_unclosed_fence_is_plain_text() {
    let blocks = markup_to_blocks("```\n# heading");
    assert!(matches!(blocks.last(), Some(RichBlock::Heading { .. })));
}

#[test]
fn opening_a_fence_far_above_its_close_matches_a_full_parse() {
    let body = "one\n\ntwo\n\nthree\n\n```\n\nfour";
    let mut parsed = MarkupBlocks::default();
    let check = |parsed: &mut MarkupBlocks, text: &str| {
        let incremental: Vec<RichBlock> =
            parsed.parse(text).iter().map(|b| (**b).clone()).collect();
        assert_eq!(incremental, markup_to_blocks(text), "{text:?}");
    };
    check(&mut parsed, body);
    let opened = format!("```\n{body}");
    check(&mut parsed, &opened);
    check(&mut parsed, &opened.replacen("one", "ones", 1));
    check(&mut parsed, body);
}

#[test]
fn a_list_item_keeps_its_lines_through_the_editor_markup() {
    let list = RichBlock::List {
        ordered: false,
        items: vec![RichListItem {
            label: String::new(),
            text: "one\ntwo".into(),
            entities: Vec::new(),
            checked: None,
        }],
    };
    let markup = blocks_to_markup(std::slice::from_ref(&list));
    assert_eq!(markup_to_blocks(&markup), vec![list]);
}

#[test]
fn text_after_a_list_stays_after_it() {
    let blocks = markup_to_blocks("- a\nafter");
    assert!(matches!(blocks.first(), Some(RichBlock::List { .. })));
    assert!(matches!(blocks.last(), Some(RichBlock::Paragraph { .. })));
}
