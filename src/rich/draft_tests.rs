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
