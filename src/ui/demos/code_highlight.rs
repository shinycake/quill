//! Screenshot demos: code blocks highlighted by their language. One chat with
//! a Rust block, a Python block, a diff and a block whose language the
//! highlighter does not know, which stays plain; and the Instant View reader
//! with a Rust block, a diff and a block without a language (injected, no live
//! Telegram).
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::ids::ChatId;
use quill::telegram::client::copy_and_parse;
use std::sync::atomic::Ordering;

const CHAT: i64 = 31;

pub(crate) const RUST: &str = "// Add two numbers\nfn add(a: i32, b: i32) -> i32 {\n    let sum = a + b; // 42 max\n    println!(\"sum = {sum}\");\n    sum\n}";
pub(crate) const PYTHON: &str = "# Greet someone\ndef greet(name: str) -> str:\n    if not name:\n        return \"Hello, world!\"\n    return f\"Hello, {name}!\"";
pub(crate) const DIFF: &str = "@@ -1,5 +1,5 @@\n fn greet(name: &str) {\n     println!(\"start\");\n-    println!(\"Hi\");\n+    println!(\"Hello, {name}!\");\n }";
// The reader's blocks differ from the chat's, so tests running side by side do
// not share cache entries.
pub(crate) const IV_RUST: &str =
    "// Add two numbers\nfn add(a: i32, b: i32) -> i32 {\n    a + b // 42 max\n}";
pub(crate) const IV_DIFF: &str =
    "@@ -1,3 +1,3 @@\n fn add(a: i32, b: i32) -> i32 {\n-    a - b\n+    a + b\n }";
const IV_PLAIN: &str = "WIBBLE y = 7; // not a language";
const UNKNOWN: &str = "WIBBLE x = 42; // not a language\nfrob(\"quux\") -> 7";

register_demos![
    DemoSpec::chats(
        "ready-code-highlight",
        "screenshot demo — code blocks highlighted by language (injected)"
    )
    .setup(QuillApp::demo_code_highlight),
    DemoSpec::chats(
        "ready-iv-code-highlight",
        "screenshot demo — Instant View code blocks highlighted by language (injected)"
    )
    .setup(QuillApp::demo_iv_code_highlight),
];

fn message(id: i64, intro: &str, code: &str, language: &str) -> String {
    let text = format!("{intro}\n{code}");
    let offset = intro.encode_utf16().count() + 1;
    let length = code.encode_utf16().count();
    let text_json = serde_json::to_string(&text).unwrap_or_default();
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{CHAT},"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{text_json},"entities":[{{"@type":"textEntity","offset":{offset},"length":{length},"type":{{"@type":"textEntityTypePreCode","language":"{language}"}}}}]}}}}}}}}"#
    )
}

impl QuillApp {
    fn demo_code_highlight(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        let seq = &self.demo_ui.seq;
        let dyn_sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
            self.demo_ui.sink.clone();
        let chat = format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{CHAT},"title":"Demo code","type":{{"@type":"chatTypePrivate","user_id":{CHAT}}},"unread_count":0}}}}"#
        );
        let position = format!(
            r#"{{"@type":"updateChatPosition","chat_id":{CHAT},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"60","is_pinned":false}}}}"#
        );
        for json in [
            chat,
            position,
            message(201, "Rust:", RUST, "rust"),
            message(202, "Python:", PYTHON, "python"),
            message(203, "Diff:", DIFF, "diff"),
            message(204, "Unknown language:", UNKNOWN, "wibble"),
        ] {
            if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
                session.apply(owned);
            }
        }
        session.open_chat(ChatId(CHAT));
    }
}

/// An Instant View page: a heading, then `pageBlockPreformatted` blocks as
/// TDLib sends them.
fn iv_page() -> quill::state::InstantViewPage {
    let plain = |text: &str| serde_json::json!({"@type": "richTextPlain", "text": text});
    let blocks = [
        serde_json::json!({"@type": "pageBlockHeader", "text": plain("Adding numbers")}),
        serde_json::json!({"@type": "pageBlockPreformatted", "text": plain(IV_RUST), "language": "rust"}),
        serde_json::json!({"@type": "pageBlockPreformatted", "text": plain(IV_DIFF), "language": "diff"}),
        serde_json::json!({"@type": "pageBlockPreformatted", "text": plain(IV_PLAIN), "language": ""}),
    ];
    quill::state::InstantViewPage {
        url: "https://example.com/adding-numbers".to_owned(),
        rich: quill::telegram::envelope::RichMessageContent {
            blocks: blocks.iter().map(quill::rich::parse_page_block).collect(),
            is_full: true,
            quote_texts: Default::default(),
        },
    }
}

impl QuillApp {
    fn demo_iv_code_highlight(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            session.messages.instant_view = Some(iv_page());
        }
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod tests {
    use crate::ui::app::QuillApp;
    use crate::ui::code_highlight::{DRAWN_RUNS, Token, ready};
    use crate::ui::keybindings::bind_keys;
    use crate::ui::screenshot_demo::ScreenshotDemo;
    use crate::ui::shell::QuillShell;
    use gpui_kit::component::Root;
    use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};

    fn new_app(cx: &mut TestAppContext, demo: &str) -> (Entity<QuillApp>, VisualTestContext) {
        cx.update(gpui_kit::init);
        cx.update(bind_keys);
        let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
        let slot_in = slot.clone();
        let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
            let view = cx.new(|cx| {
                QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named(demo)))
            });
            *slot_in.borrow_mut() = Some(view.clone());
            let shell = cx.new(|_| QuillShell::new(view));
            Root::new(shell, window, cx)
        });
        let app = slot.borrow().clone();
        let vcx = VisualTestContext::from_window(handle.into(), cx);
        (app.expect("the window builder fills the slot"), vcx)
    }

    #[gpui_kit::test]
    fn code_blocks_render_highlighted(cx: &mut TestAppContext) {
        let (_app, mut vcx) = new_app(cx, "ready-code-highlight");
        vcx.run_until_parked();
        // The tokens arrived after the first draw; the next one colors the text.
        vcx.update(|window, _| window.refresh());
        vcx.run_until_parked();
        assert!(
            DRAWN_RUNS.with(std::cell::Cell::get) > 0,
            "the cached spans reached the drawn text"
        );
        let spans = ready("rust", super::RUST).expect("the rust block was tokenized");
        assert!(spans.iter().any(|(range, token)| *token == Token::Keyword
            && super::RUST.get(range.clone()) == Some("fn")));
        assert!(ready("python", super::PYTHON).is_some_and(|spans| !spans.is_empty()));
        assert!(
            ready("wibble", super::UNKNOWN).is_some_and(|spans| spans.is_empty()),
            "an unknown language is cached as plain"
        );
        let diff = ready("diff", super::DIFF).expect("the diff block was tokenized");
        assert!(diff.iter().any(|(range, token)| *token == Token::Inserted
            && super::DIFF.get(range.clone()) == Some("+    println!(\"Hello, {name}!\");")));
    }

    #[gpui_kit::test]
    fn instant_view_code_blocks_render_highlighted(cx: &mut TestAppContext) {
        let (_app, mut vcx) = new_app(cx, "ready-iv-code-highlight");
        vcx.run_until_parked();
        vcx.update(|window, _| window.refresh());
        vcx.run_until_parked();
        let spans = ready("rust", super::IV_RUST).expect("the page's rust block was tokenized");
        assert!(spans.iter().any(|(range, token)| *token == Token::Keyword
            && super::IV_RUST.get(range.clone()) == Some("fn")));
        assert!(ready("diff", super::IV_DIFF).is_some_and(|spans| !spans.is_empty()));
        assert!(
            DRAWN_RUNS.with(std::cell::Cell::get) > 0,
            "the cached spans reached the reader's drawn text"
        );
    }
}
