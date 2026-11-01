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
