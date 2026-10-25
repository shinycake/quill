//! `ready-rich-draft`: the open chat's rich draft shown in place of the
//! field. `QUILL_DEMO_RICH_DRAFT` picks what else shows: `preview` (the
//! default, a Premium account), `discard` (the discard box), `editor` (the
//! draft opened in the rich editor), `choice` (Remove Formatting, without
//! Premium), `toast` (the Premium toast for a draft with a table),
//! `paste` (the paste notice with Open) or `paste-locked` (the notice
//! without Premium).

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::diagnostics::DiagnosticSink;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::Ordering;

register_demos![
    DemoSpec::ready(
        "ready-rich-draft",
        crate::ui::demo::seed_ready_chats_session,
        "screenshot demo — a rich draft shown in place of the message field"
    )
    .setup(QuillApp::demo_rich_draft),
];

const BLOCKS: &str = r#"[
    {"@type":"pageBlockSectionHeading","size":1,"text":{"@type":"richTextPlain","text":"Launch checklist"}},
    {"@type":"pageBlockParagraph","text":{"@type":"richTexts","texts":[
        {"@type":"richTextPlain","text":"Ship the "},
        {"@type":"richTextBold","text":{"@type":"richTextPlain","text":"desktop build"}},
        {"@type":"richTextPlain","text":" on Thursday, then post the notes."}
    ]}},
    {"@type":"pageBlockList","items":[
        {"@type":"pageBlockListItem","label":"","blocks":[{"@type":"pageBlockParagraph","text":{"@type":"richTextPlain","text":"Freeze the strings"}}],"has_checkbox":true,"is_checked":true},
        {"@type":"pageBlockListItem","label":"","blocks":[{"@type":"pageBlockParagraph","text":{"@type":"richTextPlain","text":"Record the demo"}}],"has_checkbox":true,"is_checked":false},
        {"@type":"pageBlockListItem","label":"","blocks":[{"@type":"pageBlockParagraph","text":{"@type":"richTextPlain","text":"Update the changelog"}}],"has_checkbox":true,"is_checked":false}
    ]},
    {"@type":"pageBlockDivider"},
    {"@type":"pageBlockParagraph","text":{"@type":"richTextPlain","text":"Questions go in the release thread."}}
]"#;

const TABLE: &str = r#"[
    {"@type":"pageBlockSectionHeading","size":2,"text":{"@type":"richTextPlain","text":"Build sizes"}},
    {"@type":"pageBlockTable","caption":{"@type":"richTextPlain","text":""},"cells":[
        [{"@type":"pageBlockTableCell","text":{"@type":"richTextPlain","text":"macOS"}},{"@type":"pageBlockTableCell","text":{"@type":"richTextPlain","text":"38 MB"}}],
        [{"@type":"pageBlockTableCell","text":{"@type":"richTextPlain","text":"Linux"}},{"@type":"pageBlockTableCell","text":{"@type":"richTextPlain","text":"41 MB"}}]
    ]}
]"#;

const PASTED: &str = "# Release notes\n- [x] Rich drafts\n- [ ] Paste notice";

impl QuillApp {
    fn demo_rich_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let variant = std::env::var("QUILL_DEMO_RICH_DRAFT").unwrap_or_default();
        let premium = !matches!(variant.as_str(), "choice" | "toast" | "paste-locked");
        let paste = variant.starts_with("paste");
        let blocks = if variant == "toast" { TABLE } else { BLOCKS };
        let sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        if let Some(session) = self.demo_session.as_mut() {
            session.payments.premium_option = Some(premium);
            if !paste {
                self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
                let json = format!(
                    r#"{{"@type":"updateChatDraftMessage","chat_id":11,"draft_message":{{"@type":"draftMessage","reply_to":null,"date":1700000000,"content":{{"@type":"draftMessageContentRichMessage","message":{{"@type":"richMessage","is_rtl":false,"is_full":true,"blocks":{blocks}}}}},"effect_id":"0","suggested_post_info":null}},"positions":[]}}"#
                );
                if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &sink) {
                    session.apply(owned);
                }
            }
        }
        self.restore_open_draft(window, cx);
        match variant.as_str() {
            "discard" => self.composer_ui.rich_draft.discard_confirm = true,
            "editor" => self.open_rich_draft(window, cx),
            "choice" | "toast" => {
                let _ = self.submit_rich_draft(window, cx);
            }
            "paste" | "paste-locked" => {
                self.set_composer_markup(PASTED, window, cx);
                // window-scope: a demo has one window; its notification
                // layer exists once the window is open.
                cx.defer_in(window, move |this, window, cx| {
                    this.show_rich_paste_notice("", !premium, window, cx);
                });
            }
            _ => {}
        }
        cx.notify();
    }
}
