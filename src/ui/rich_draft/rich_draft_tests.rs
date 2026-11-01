//! A rich draft in the open composer: it shows in place of the field,
//! survives leaving the chat, opens in the rich editor, comes back from
//! it, discards, and asks a non-Premium sender about its formatting. Over
//! the `ready-chats` demo; nothing reaches Telegram.

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::rich::RichDraft;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const OPEN: ChatId = ChatId(11);

const RICH: &str = r#"{"@type":"draftMessage","reply_to":null,"date":1700000000,"content":{"@type":"draftMessageContentRichMessage","message":{"@type":"richMessage","is_rtl":false,"is_full":true,"blocks":[{"@type":"pageBlockSectionHeading","size":1,"text":{"@type":"richTextPlain","text":"Plan"}},{"@type":"pageBlockParagraph","text":{"@type":"richTextBold","text":{"@type":"richTextPlain","text":"ship it"}}}]}},"effect_id":"0","suggested_post_info":null}"#;

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
    let app: Entity<QuillApp> = slot.borrow().clone().expect("the app view");
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

fn shown(vcx: &mut VisualTestContext, id: &'static str) -> bool {
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.update(|window, _| window.try_find(id).is_some())
}

fn click(vcx: &mut VisualTestContext, id: &'static str) {
    vcx.update(|window, cx| {
        window.render_frame(cx);
        window.click(id, cx);
    });
    frames(vcx);
}

/// The rich draft another client saved for the open chat.
fn remote_rich_draft(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, premium: bool) {
    let json = format!(
        r#"{{"@type":"updateChatDraftMessage","chat_id":{},"draft_message":{RICH},"positions":[]}}"#,
        OPEN.0
    );
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.payments.premium_option = Some(premium);
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

fn stored_rich(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<RichDraft> {
    app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|session| session.draft_for(OPEN, Default::default()))
            .and_then(|draft| draft.rich.clone())
    })
}

#[gpui_kit::test]
fn a_rich_draft_shows_in_place_of_the_field_and_survives_leaving(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    remote_rich_draft(&app, vcx, true);
    assert!(shown(vcx, "rich-draft-preview"));
    assert!(shown(vcx, "rich-draft-discard"));
    assert_eq!(composer_text(&app, vcx), "");

    let other = app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|session| {
                session
                    .ordered_chats()
                    .into_iter()
                    .map(|chat| chat.id)
                    .find(|id| *id != OPEN)
            })
            .expect("the demo lists another chat")
    });
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| app.select_listed_chat(other, window, cx));
    });
    frames(vcx);
    assert!(stored_rich(&app, vcx).is_some(), "leaving kept the draft");
    assert!(!shown(vcx, "rich-draft-preview"));
}

#[gpui_kit::test]
fn the_editor_opens_the_draft_and_closing_it_shows_the_draft_again(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    remote_rich_draft(&app, vcx, true);
    click(vcx, "rich-draft-preview");
    assert!(app.read_with(vcx, |app, _| app.composer_ui.rich_editor_open));
    assert_eq!(composer_text(&app, vcx), "# Plan\n\n**ship it**");
    assert!(!shown(vcx, "rich-draft-preview"));

    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.set_composer_markup("# Plan\n\n- [ ] ship it", window, cx);
            app.close_rich_editor(window, cx);
        });
    });
    frames(vcx);
    assert_eq!(
        stored_rich(&app, vcx).map(|rich| rich.markup()).as_deref(),
        Some("# Plan\n\n- [ ] ship it")
    );
    assert_eq!(composer_text(&app, vcx), "");
    assert!(shown(vcx, "rich-draft-preview"));
}

#[gpui_kit::test]
fn discarding_asks_first_then_drops_the_draft(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    remote_rich_draft(&app, vcx, true);
    click(vcx, "rich-draft-discard");
    assert!(shown(vcx, "rich-discard-confirm"));
    assert!(
        stored_rich(&app, vcx).is_some(),
        "nothing goes before the confirm"
    );
    click(vcx, "rich-discard-confirm");
    assert!(stored_rich(&app, vcx).is_none());
    assert!(!shown(vcx, "rich-draft-preview"));
}

#[gpui_kit::test]
fn sending_without_premium_offers_the_text_without_formatting(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    remote_rich_draft(&app, vcx, false);
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| app.submit_from_send_button(window, cx));
    });
    frames(vcx);
    let choice = app.read_with(vcx, |app, _| {
        app.composer_ui.rich_draft.premium_choice.clone()
    });
    assert_eq!(choice.as_deref(), Some("Plan\n**ship it**"));
    assert!(shown(vcx, "rich-choice-plain"));
}

#[gpui_kit::test]
fn the_rich_editor_shows_the_blocks_of_a_long_draft(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let markup = crate::ui::demos::composer::rich_editor_markup(32 * 1024);
    app.update_in(vcx, |app, window, cx| {
        app.composer_ui.rich_editor_open = true;
        app.set_composer_markup(&markup, window, cx);
    });
    frames(vcx);
    assert!(shown(vcx, "rich-editor-close"));
    let text = app.read_with(vcx, |app, cx| app.composer.read(cx).value().to_string());
    assert_eq!(text, markup);
    let blocks = quill::rich::preview_blocks(&text);
    assert!(blocks.len() > 1_000, "{} blocks", blocks.len());
}
