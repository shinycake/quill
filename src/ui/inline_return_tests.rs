#![cfg(test)]
#![cfg(feature = "demo-capture")]
//! The inline results button's round trip ends when the bot's chat cannot
//! be opened, so its start never goes to a chat opened later (sweep 10-119).

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;

fn open(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    app.update(&mut vcx, |app, _| {
        let session = app.demo_session.as_mut().expect("a demo session");
        let first = session.chats.keys().min().copied().expect("a chat");
        session.open_chat = Some(ChatId(first));
    });
    (app, vcx)
}

fn waiting(app: &Entity<QuillApp>, vcx: &VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| app.composer_ui.inline_return.is_some())
}

#[gpui_kit::test]
fn a_failed_bot_lookup_ends_the_round_trip(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    app.update(&mut vcx, |app, _| {
        app.begin_inline_return(900_001, "somebot", "param");
    });
    assert!(waiting(&app, &vcx), "waits for the bot's chat");
    app.update(&mut vcx, |app, _| app.cancel_inline_opening());
    assert!(!waiting(&app, &vcx), "the failed lookup leaves nothing");
}

#[gpui_kit::test]
fn another_link_ending_in_a_different_chat_ends_the_round_trip(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    app.update(&mut vcx, |app, _| {
        app.begin_inline_return(900_001, "somebot", "param");
        // The bot's own chat is the one this round trip waits for.
        app.cancel_inline_opening_unless_chat(ChatId(900_001));
    });
    assert!(waiting(&app, &vcx), "the bot's chat keeps it waiting");
    app.update(&mut vcx, |app, _| {
        app.cancel_inline_opening_unless_chat(ChatId(5));
    });
    assert!(
        !waiting(&app, &vcx),
        "a link that ended in another chat leaves nothing waiting"
    );
}

/// Sweep 10-140: once the start went, the trip lives while the user stays in
/// the bot's chat; leaving it ends the trip.
#[gpui_kit::test]
fn leaving_the_bots_chat_after_the_start_ends_the_round_trip(cx: &mut TestAppContext) {
    use super::{InlineReturn, InlineStage};
    let (app, mut vcx) = open(cx);
    // Chat 11 is the private chat with user 11, standing in for the bot's.
    const BOT_CHAT: ChatId = ChatId(11);
    app.update_in(&mut vcx, |app, window, cx| {
        app.select_listed_chat(BOT_CHAT, window, cx);
        let account = app.session().map(|s| s.account.clone()).expect("account");
        app.composer_ui.inline_return = Some(InlineReturn {
            account,
            origin: ChatId(12),
            bot_id: 11,
            username: "somebot".into(),
            stage: InlineStage::Started { baseline: 0 },
        });
        app.flush_inline_return(window, cx);
    });
    assert!(
        waiting(&app, &vcx),
        "still in the bot's chat: the trip waits"
    );
    app.update_in(&mut vcx, |app, window, cx| {
        app.select_listed_chat(ChatId(13), window, cx);
        app.flush_inline_return(window, cx);
    });
    assert!(
        !waiting(&app, &vcx),
        "another chat ended it: a later message of the bot goes nowhere"
    );
}
