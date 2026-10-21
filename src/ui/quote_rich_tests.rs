//! A reply quoting formatted text of a rich message, on the
//! `ready-quote-rich` demo: the strip shows the quote's formatting, and
//! quoting the rich message again sends `rich_texts`. Needs the
//! `demo-capture` feature (gpui-kit's test support).
#![cfg(test)]
#![cfg(feature = "demo-capture")]

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use quill::ids::{ChatId, MessageId};
use quill::text::TextEntityKind;

const CHAT: ChatId = ChatId(11);
const RICH: MessageId = MessageId(801);
const REPLY: MessageId = MessageId(802);

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-quote-rich");
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("the window opened the app");
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

#[gpui_kit::test]
fn the_reply_strip_keeps_the_formatting_of_a_rich_quote(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let header = app.read_with(vcx, |app, _| {
        let session = app.session()?;
        let reply = session.histories.get(&CHAT.0)?.messages.get(&REPLY.0)?;
        session.reply_header(reply)
    });
    let header = header.expect("the reply has a strip");
    assert!(header.is_quote);
    assert_eq!(
        header.text,
        "formatted quote feature before Friday.\nOld plan is dropped"
    );
    let kinds: Vec<_> = header
        .entities
        .iter()
        .filter_map(|entity| {
            header
                .text
                .get(entity.utf8_start..entity.utf8_end)
                .map(|text| (text, entity.kind.clone()))
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            ("formatted quote", TextEntityKind::Bold),
            ("Friday", TextEntityKind::Italic),
            ("Old plan", TextEntityKind::Strikethrough),
        ]
    );
}

#[gpui_kit::test]
fn quoting_the_rich_message_keeps_its_rich_texts(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        app.begin_quote_reply(CHAT, RICH, "formatted quote", window, cx);
    });
    frames(vcx);
    let quote = app
        .read_with(vcx, |app, _| {
            app.composer_ui
                .reply()
                .and_then(|reply| reply.quote.clone())
        })
        .expect("the reply carries the quote");
    assert_eq!(quote.text, "formatted quote");
    // "Launch checklist\n" is 17 units, then "Ship the " is 9 more.
    assert_eq!(quote.position, 26);
    assert_eq!(quote.rich_texts.len(), 1);
    let target = app
        .read_with(vcx, |app, _| {
            app.composer_ui
                .reply()
                .and_then(|reply| reply.send_target(CHAT))
        })
        .expect("a same-chat reply");
    assert_eq!(target.quote_rich, quote.rich_texts);
}
