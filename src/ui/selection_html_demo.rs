//! Screenshot demo for "Save as HTML" (`parity:menu-save-selection-html`):
//! a chat with a rich message selected, so the selection bar offers the
//! action (injected, no live Telegram).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::composer::ForwardDraft;
use quill::ids::{ChatId, MessageId};
use serde_json::{Value, json};
use std::sync::atomic::Ordering;

register_demos![
    DemoSpec::chats(
        "ready-save-html",
        "screenshot demo — a selected rich message offers Save as HTML (injected)"
    )
    .setup(QuillApp::demo_save_html),
];

pub(super) const CHAT: ChatId = ChatId(41);
pub(super) const RICH: MessageId = MessageId(401);
pub(super) const TEXT: MessageId = MessageId(402);

fn plain(text: &str) -> Value {
    json!({"@type": "richTextPlain", "text": text})
}

/// The rich message: a heading, styled text, a checklist and a table.
pub(super) fn rich_blocks() -> Value {
    json!([
        {"@type": "pageBlockSectionHeading", "size": 1, "text": plain("Weekend trip")},
        {"@type": "pageBlockParagraph", "text": {"@type": "richTexts", "texts": [
            plain("Train leaves at "),
            {"@type": "richTextBold", "text": plain("8:40")},
            plain(" from platform "),
            {"@type": "richTextFixed", "text": plain("3B")},
            plain(". Tickets are on the "),
            {"@type": "richTextUrl", "text": plain("booking page"),
                "url": "https://example.com/booking", "is_cached": false},
            plain("."),
        ]}},
        {"@type": "pageBlockList", "items": [
            {"@type": "pageBlockListItem", "label": "•", "has_checkbox": true, "is_checked": true,
                "value": 0, "type": "",
                "blocks": [{"@type": "pageBlockParagraph", "text": plain("Book the cabin")}]},
            {"@type": "pageBlockListItem", "label": "•", "has_checkbox": true, "is_checked": false,
                "value": 0, "type": "",
                "blocks": [{"@type": "pageBlockParagraph", "text": {"@type": "richTexts", "texts": [
                    plain("Pack the "),
                    {"@type": "richTextBold", "text": plain("tent")},
                ]}}]},
        ]},
        {"@type": "pageBlockTable", "caption": plain(""), "is_bordered": true,
            "is_striped": false, "is_compact": false, "cells": [
            [{"@type": "pageBlockTableCell", "text": plain("Day"), "is_header": true,
                "colspan": 1, "rowspan": 1},
             {"@type": "pageBlockTableCell", "text": plain("Plan"), "is_header": true,
                "colspan": 1, "rowspan": 1}],
            [{"@type": "pageBlockTableCell", "text": plain("Sat"), "is_header": false,
                "colspan": 1, "rowspan": 1},
             {"@type": "pageBlockTableCell", "text": plain("Lake loop"), "is_header": false,
                "colspan": 1, "rowspan": 1}],
        ]},
    ])
}

impl QuillApp {
    fn demo_save_html(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        let seq = &self.demo_ui.seq;
        let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
            self.demo_ui.sink.clone();
        let chat = CHAT.0;
        let updates = [
            json!({"@type": "updateNewChat", "chat": {"id": chat, "title": "Trip planning",
                "type": {"@type": "chatTypePrivate", "user_id": chat}, "unread_count": 0}}),
            json!({"@type": "updateChatPosition", "chat_id": chat, "position": {
                "@type": "chatPosition", "list": {"@type": "chatListMain"}, "order": "70",
                "is_pinned": false}}),
            json!({"@type": "updateNewMessage", "message": {"id": RICH.0, "chat_id": chat,
                "is_outgoing": false, "date": 1_790_631_720,
                "content": {"@type": "messageRichMessage", "message": {"@type": "richMessage",
                    "blocks": rich_blocks(), "is_rtl": false, "is_full": true}}}}),
            json!({"@type": "updateNewMessage", "message": {"id": TEXT.0, "chat_id": chat,
                "is_outgoing": true, "date": 1_790_631_780,
                "content": {"@type": "messageText", "text": {"@type": "formattedText",
                    "text": "Saving this one for later.", "entities": []}}}}),
        ];
        for update in updates {
            if let Some(owned) =
                quill::telegram::client::copy_and_parse(&update.to_string(), seq, &sink)
            {
                session.apply(owned);
            }
        }
        session.open_chat(CHAT);
        self.share.pending_forward = ForwardDraft::from_message(CHAT, RICH, false);
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod tests {
    use super::{CHAT, RICH, TEXT};
    use crate::ui::app::QuillApp;
    use crate::ui::keybindings::bind_keys;
    use crate::ui::screenshot_demo::ScreenshotDemo;
    use crate::ui::shell::QuillShell;
    use gpui_kit::component::Root;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
    use quill::composer::ForwardDraft;

    const BUTTON: &str = "selection-save-html";
    /// The selection bar is showing (its Cancel button).
    const BAR: &str = "selection-cancel";

    fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
        cx.update(gpui_kit::init);
        cx.update(bind_keys);
        quill::file_prefs::set_persistence(false);
        let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
        let slot_in = slot.clone();
        let handle = cx.open_window(size(px(1200.), px(860.)), move |window, cx| {
            let view = cx.new(|cx| {
                QuillApp::new_with_demo(
                    window,
                    cx,
                    None,
                    Some(ScreenshotDemo::named("ready-save-html")),
                )
            });
            *slot_in.borrow_mut() = Some(view.clone());
            let focus = view.focus_handle(cx);
            window.focus(&focus, cx);
            let shell = cx.new(|_| QuillShell::new(view));
            Root::new(shell, window, cx)
        });
        let app: Entity<QuillApp> = slot.borrow().clone().unwrap();
        let mut vcx = VisualTestContext::from_window(handle.into(), cx);
        frames(&mut vcx);
        (app, vcx)
    }

    /// Render until the selection bar's slide has finished (it runs on the
    /// wall clock).
    fn frames(cx: &mut VisualTestContext) {
        cx.update(|window, cx| window.render_frame(cx));
        std::thread::sleep(std::time::Duration::from_millis(400));
        for _ in 0..2 {
            cx.update(|window, cx| window.render_frame(cx));
        }
        cx.run_until_parked();
    }

    fn has(cx: &mut VisualTestContext, id: &'static str) -> bool {
        cx.update(|window, _| window.try_find(id).is_some())
    }

    fn select(app: &Entity<QuillApp>, cx: &mut VisualTestContext, draft: Option<ForwardDraft>) {
        app.update(cx, |app, cx| {
            app.share.pending_forward = draft;
            cx.notify();
        });
        frames(cx);
    }

    #[gpui_kit::test]
    fn the_bar_offers_save_as_html_for_one_selected_rich_message(cx: &mut TestAppContext) {
        let (app, mut cx) = new_app(cx);
        assert!(has(&mut cx, BAR));
        assert!(has(&mut cx, BUTTON));

        // Two messages selected: tdesktop offers it for a single item only.
        let mut both = ForwardDraft::from_message(CHAT, RICH, false).unwrap();
        both.toggle(CHAT, TEXT, false);
        select(&app, &mut cx, Some(both));
        assert!(has(&mut cx, BAR));
        assert!(!has(&mut cx, BUTTON));

        // A plain text message alone is not a rich message.
        select(&app, &mut cx, ForwardDraft::from_message(CHAT, TEXT, false));
        assert!(!has(&mut cx, BUTTON));

        // Back to the rich message; the action ends the selection.
        select(&app, &mut cx, ForwardDraft::from_message(CHAT, RICH, false));
        assert!(has(&mut cx, BUTTON));
        cx.update(|window, cx| window.click(BUTTON, cx));
        frames(&mut cx);
        assert!(app.read_with(&cx, |app, _| app.share.pending_forward.is_none()));
        assert!(!has(&mut cx, BUTTON));
    }

    #[gpui_kit::test]
    fn each_ending_gets_its_toast_and_only_an_ending_redraws(cx: &mut TestAppContext) {
        use quill::rich_html::RichHtmlOutcome;
        let (app, mut cx) = new_app(cx);
        app.update(&mut cx, |app, _| {
            // Nothing ended (the demo has no live session): no redraw asked.
            assert!(!app.poll_rich_html_exports());

            // A picked folder is named as it is, never called "Downloads".
            let html = std::path::PathBuf::from("/Archive/Trip/Trip.html");
            app.toast_rich_html(&RichHtmlOutcome::Saved(html.clone()));
            let note = app.connection.note();
            assert_eq!(note.text(), "Message was saved to your Archive folder");
            assert_eq!(note.reveal(), Some(html.as_path()));

            app.toast_rich_html(&RichHtmlOutcome::Failed);
            let note = app.connection.note();
            assert_eq!(note.text(), "Could not save the message as HTML.");
            assert_eq!(note.reveal(), None);

            // Logout or quit ends it silently.
            app.toast_rich_html(&RichHtmlOutcome::Cancelled);
            assert_eq!(
                app.connection.note().text(),
                "Could not save the message as HTML."
            );
        });
    }

    #[gpui_kit::test]
    fn protected_chats_never_offer_it(cx: &mut TestAppContext) {
        let (app, mut cx) = new_app(cx);
        app.update(&mut cx, |app, cx| {
            if let Some(session) = app.demo_session.as_mut() {
                let seq = std::sync::atomic::AtomicU64::new(session.last_seq + 1);
                let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
                    app.demo_ui.sink.clone();
                let json = format!(
                    r#"{{"@type":"updateChatHasProtectedContent","chat_id":{},"has_protected_content":true}}"#,
                    CHAT.0
                );
                if let Some(owned) = quill::telegram::client::copy_and_parse(&json, &seq, &sink) {
                    session.apply(owned);
                }
            }
            cx.notify();
        });
        frames(&mut cx);
        assert!(has(&mut cx, BAR));
        assert!(!has(&mut cx, BUTTON));
    }
}
