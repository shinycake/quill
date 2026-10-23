//! A draft saved on another device reaches the open composer (tdesktop
//! `HistoryWidget::applyCloudDraft`), and leaving the chat keeps it. Over
//! the `ready-chats` demo; nothing reaches Telegram.
#![cfg(feature = "demo-capture")]

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const OPEN: ChatId = ChatId(11);

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    (app, vcx)
}

fn frames(vcx: &mut VisualTestContext) {
    vcx.update(|window, cx| {
        for _ in 0..2 {
            window.render_frame(cx);
        }
    });
    vcx.run_until_parked();
}

/// `updateChatDraftMessage` for `chat`, as TDLib sends it when another
/// client saves (`None`: the draft was cleared there).
fn remote_draft(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    chat: ChatId,
    text: Option<&str>,
) {
    let draft = text.map_or("null".to_string(), |text| {
        format!(
            r#"{{"@type":"draftMessage","reply_to":null,"date":1700000000,"content":{{"@type":"draftMessageContentText","text":{{"@type":"formattedText","text":"{text}","entities":[]}},"link_preview_options":null}},"effect_id":"0","suggested_post_info":null}}"#
        )
    });
    let json = format!(
        r#"{{"@type":"updateChatDraftMessage","chat_id":{},"draft_message":{draft},"positions":[]}}"#,
        chat.0
    );
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            // After the demo's own updates, or the reducer drops it.
            let seq = AtomicU64::new(session.last_seq);
            let owned = copy_and_parse(&json, &seq, &sink).expect("update parses");
            session.apply(owned);
        }
        cx.notify();
    });
    frames(vcx);
}

fn composer_text(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> String {
    app.read_with(vcx, |app, cx| app.composer.read(cx).value().to_string())
}

fn stored_draft(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    chat: ChatId,
) -> Option<String> {
    app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|session| session.draft_for(chat, Default::default()))
            .map(|draft| draft.text.clone())
    })
}

fn other_chat(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> ChatId {
    app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|session| {
                session
                    .ordered_chats()
                    .into_iter()
                    .map(|chat| chat.id)
                    .find(|id| *id != OPEN)
            })
            .expect("the demo lists another chat")
    })
}

#[gpui_kit::test]
fn a_draft_typed_elsewhere_fills_the_open_composer_and_survives_leaving(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    remote_draft(&app, vcx, OPEN, Some("typed on the phone"));
    assert_eq!(composer_text(&app, vcx), "typed on the phone");

    // Leaving the chat saves what the composer shows, not an empty field.
    let other = other_chat(&app, vcx);
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| app.select_listed_chat(other, window, cx));
    });
    frames(vcx);
    assert_eq!(
        stored_draft(&app, vcx, OPEN).as_deref(),
        Some("typed on the phone")
    );

    // Cleared on the phone while the chat is open again: the field empties.
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| app.select_listed_chat(OPEN, window, cx));
    });
    frames(vcx);
    assert_eq!(composer_text(&app, vcx), "typed on the phone");
    remote_draft(&app, vcx, OPEN, None);
    assert_eq!(composer_text(&app, vcx), "");
}

#[gpui_kit::test]
fn unsaved_typing_wins_over_a_draft_from_elsewhere(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.composer.update(cx, |input, cx| input.focus(window, cx));
        });
    });
    vcx.simulate_input("mine");
    // The demo stores each keystroke at once; mark the field as unsaved, as
    // a live composer is during the save's quiet second.
    app.update(vcx, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            session.mark_draft_dirty(OPEN, Default::default());
        }
    });
    remote_draft(&app, vcx, OPEN, Some("from the phone"));
    assert_eq!(composer_text(&app, vcx), "mine");
}

#[gpui_kit::test]
fn a_draft_for_another_chat_leaves_the_composer_alone(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let other = other_chat(&app, vcx);
    remote_draft(&app, vcx, other, Some("for the other chat"));
    assert_eq!(composer_text(&app, vcx), "");
    assert_eq!(
        stored_draft(&app, vcx, other).as_deref(),
        Some("for the other chat")
    );
}
