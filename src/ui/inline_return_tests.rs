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
    use super::{Baseline, InlineReturn, InlineStage};
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
            stage: InlineStage::Started {
                baseline: Baseline::At(0),
            },
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

fn apply_json(app: &mut QuillApp, json: &str) {
    let last = app
        .demo_session
        .as_ref()
        .map_or(0, |session| session.last_seq);
    app.demo_ui
        .seq
        .fetch_max(last, std::sync::atomic::Ordering::Relaxed);
    let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> = app.demo_ui.sink.clone();
    let owned = quill::telegram::client::copy_and_parse(json, &app.demo_ui.seq, &sink)
        .expect("a valid update");
    app.demo_session
        .as_mut()
        .expect("a demo session")
        .apply(owned);
}

fn bot_message(chat: i64, id: i64, outgoing: bool, switch: bool) -> String {
    let markup = if switch {
        r#","reply_markup":{"@type":"replyMarkupInlineKeyboard","rows":[[{"@type":"inlineKeyboardButton","text":"Search","icon_custom_emoji_id":0,"style":{"@type":"buttonStyleDefault"},"type":{"@type":"inlineKeyboardButtonTypeSwitchInline","query":"pic","target_chat":{"@type":"targetChatCurrent"}}}]],"force_reply":false}"#
    } else {
        ""
    };
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{chat},"is_outgoing":{outgoing},"date":1790631720{markup},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hi","entities":[]}}}}}}}}"#
    )
}

/// The bot's chat history was not loaded when the start went out: an old
/// message of the bot with a switch-inline button, loaded later, is not
/// its answer (sweep 10-139). Only what follows the user's own start is.
#[gpui_kit::test]
fn an_old_bot_message_loaded_after_the_start_does_not_bring_the_user_back(cx: &mut TestAppContext) {
    use quill::telegram::envelope::ChatKind;
    let (app, mut vcx) = open(cx);
    let (origin, bot_chat, bot_id) = app.read_with(&vcx, |app, _| {
        let session = app.session().expect("demo session");
        let origin = session.open_chat.expect("an open chat");
        let (chat, user) = session
            .chats
            .values()
            .find_map(|chat| match &chat.kind {
                ChatKind::Private { user_id } if chat.id != origin => Some((chat.id.0, user_id.0)),
                _ => None,
            })
            .expect("a private chat to stand for the bot");
        (origin, chat, user)
    });
    app.update(&mut vcx, |app, cx| {
        // The user is in the bot's chat when the start goes: the trip ends
        // when they leave it, and a frame between the steps checks that.
        app.demo_session
            .as_mut()
            .expect("a demo session")
            .histories
            .remove(&bot_chat);
        app.begin_inline_return(bot_id, "somebot", "param");
        // The user lands in the bot's chat with the start: a frame between
        // the steps must not find the trip outside the bot's chat.
        app.demo_session.as_mut().expect("a demo session").open_chat = Some(ChatId(bot_chat));
        assert!(
            app.take_inline_start(ChatId(bot_chat), cx),
            "the start goes"
        );
        // The user lands in the bot's chat with the start: a frame between
        // the two would see the origin chat and end the trip.
        app.demo_session.as_mut().expect("a demo session").open_chat = Some(ChatId(bot_chat));
    });
    // Its history loads: an old answer of the bot, then the user's own
    // start.
    app.update_in(&mut vcx, |app, window, cx| {
        apply_json(app, &bot_message(bot_chat, 5, false, true));
        apply_json(app, &bot_message(bot_chat, 10, true, false));
        app.flush_inline_return(window, cx);
    });
    assert!(waiting(&app, &vcx), "the old message did not end the trip");
    // The bot's real answer comes after the start.
    app.update_in(&mut vcx, |app, window, cx| {
        apply_json(app, &bot_message(bot_chat, 11, false, true));
        app.flush_inline_return(window, cx);
    });
    assert!(!waiting(&app, &vcx), "the answer took the user back");
    assert_eq!(
        app.read_with(&vcx, |app, _| app.open_chat_id()),
        Some(origin),
        "to the chat the query came from"
    );
}
