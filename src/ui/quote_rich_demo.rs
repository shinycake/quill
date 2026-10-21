//! Screenshot demo for quotes chosen from rich messages: a rich message and
//! a reply whose quote carries `textQuote.rich_texts` (bold, italic and
//! strikethrough over two quoted texts).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::ids::ChatId;
use quill::state::Session;
use std::sync::atomic::{AtomicU64, Ordering};

register_demos![
    DemoSpec::chats(
        "ready-quote-rich",
        "screenshot demo — a reply quoting formatted text of a rich message"
    )
    .setup(QuillApp::demo_ready_quote_rich),
];

const CHAT: ChatId = ChatId(11);

const RICH_MESSAGE: &str = r#"{"@type":"updateNewMessage","message":{"id":801,"chat_id":11,"is_outgoing":false,"date":1790631600,"content":{"@type":"messageRichMessage","message":{"@type":"richMessage","is_rtl":false,"is_full":true,"blocks":[
{"@type":"pageBlockTitle","title":{"@type":"richTextPlain","text":"Launch checklist"}},
{"@type":"pageBlockParagraph","text":{"@type":"richTexts","texts":[
 {"@type":"richTextPlain","text":"Ship the "},
 {"@type":"richTextBold","text":{"@type":"richTextPlain","text":"formatted quote"}},
 {"@type":"richTextPlain","text":" feature before "},
 {"@type":"richTextItalic","text":{"@type":"richTextPlain","text":"Friday"}},
 {"@type":"richTextPlain","text":"."}]}},
{"@type":"pageBlockParagraph","text":{"@type":"richTexts","texts":[
 {"@type":"richTextStrikethrough","text":{"@type":"richTextPlain","text":"Old plan"}},
 {"@type":"richTextPlain","text":" is dropped."}]}}
]}}}}"#;

const REPLY: &str = r#"{"@type":"updateNewMessage","message":{"id":802,"chat_id":11,"is_outgoing":true,"date":1790631660,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"On it, starting now.","entities":[]}},"reply_to":{"@type":"messageReplyToMessage","chat_id":11,"message_id":801,"quote":{"@type":"textQuote","text":{"@type":"formattedText","text":"","entities":[]},"rich_texts":[
{"@type":"richTexts","texts":[
 {"@type":"richTextBold","text":{"@type":"richTextPlain","text":"formatted quote"}},
 {"@type":"richTextPlain","text":" feature before "},
 {"@type":"richTextItalic","text":{"@type":"richTextPlain","text":"Friday"}},
 {"@type":"richTextPlain","text":"."}]},
{"@type":"richTexts","texts":[
 {"@type":"richTextStrikethrough","text":{"@type":"richTextPlain","text":"Old plan"}},
 {"@type":"richTextPlain","text":" is dropped"}]}
],"position":28,"is_manual":true},"checklist_task_id":0,"poll_option_id":""}}}"#;

pub(super) fn apply_ready_quote_rich(
    session: &mut Session,
    seq: &AtomicU64,
    sink: &std::sync::Arc<quill::diagnostics::MemorySink>,
) {
    let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    for json in [RICH_MESSAGE, REPLY] {
        if let Some(owned) = quill::telegram::client::copy_and_parse(json, seq, &sink) {
            session.apply(owned);
        }
    }
    session.open_chat(CHAT);
}

impl QuillApp {
    fn demo_ready_quote_rich(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_quote_rich(session, &self.demo_ui.seq, &self.demo_ui.sink);
        }
        cx.notify();
    }
}
