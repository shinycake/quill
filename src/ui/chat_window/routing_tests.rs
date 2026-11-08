//! Every chat open goes to the window that shows the chat, with what it
//! lands on (`routing`).

use super::super::tests::{Harness, OPEN, OTHER, OTHER_LAST, new_app, new_demo_app};
use super::ChatOpen;
use gpui_kit::{AnyWindowHandle, AppContext, TestAppContext};
use quill::composer::ComposerReplyTo;
use quill::ids::{ChatId, MessageId};
use quill::state::ChatSearchJump;

/// The newest message of the main window's chat.
fn open_chat_message(h: &Harness, cx: &mut TestAppContext) -> MessageId {
    h.settle(cx);
    h.app
        .read_with(cx, |app, _| {
            let history = app.session()?.histories.get(&OPEN.0)?;
            history.ordered().last().map(|message| message.id)
        })
        .expect("the chat has a message")
}

/// The chat `chat`'s window shows, and the message its view jumped to.
fn window_view(
    h: &Harness,
    chat: Option<ChatId>,
    cx: &mut TestAppContext,
) -> (Option<ChatId>, Option<MessageId>) {
    h.app
        .update(cx, |app, cx| {
            app.in_window_scope(chat, cx, |app, _| {
                let session = app.session()?;
                let jump = match session.search.chat_search.jump {
                    ChatSearchJump::Ready { message_id }
                    | ChatSearchJump::Loading { message_id }
                    | ChatSearchJump::Missing { message_id } => Some(message_id),
                    _ => None,
                };
                Some((session.open_chat, jump))
            })
        })
        .flatten()
        .unwrap_or((None, None))
}

fn in_window(
    h: &Harness,
    handle: AnyWindowHandle,
    chat: Option<ChatId>,
    cx: &mut TestAppContext,
    f: impl FnOnce(
        &mut super::super::super::app::QuillApp,
        &mut gpui_kit::Window,
        &mut gpui_kit::Context<super::super::super::app::QuillApp>,
    ),
) {
    let app = h.app.clone();
    cx.update_window(handle, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.in_window_scope(chat, cx, |app, cx| f(app, window, cx));
        });
    })
    .unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn go_to_message_from_a_chat_window_opens_the_main_window_at_it(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let target = open_chat_message(&h, cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    in_window(&h, chat_window, Some(OTHER), cx, |app, window, cx| {
        app.go_to_message_in(OPEN, target, window, cx);
    });
    assert_eq!(
        window_view(&h, None, cx),
        (Some(OPEN), Some(target)),
        "the main window opened the chat at the message"
    );
    assert_eq!(window_view(&h, Some(OTHER), cx).0, Some(OTHER));
}

#[gpui_kit::test]
fn a_header_click_in_a_chat_window_keeps_the_window_on_its_chat(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let target = open_chat_message(&h, cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    // "Forwarded from" and an external reply header open the message.
    in_window(&h, chat_window, Some(OTHER), cx, |app, window, cx| {
        app.select_search_message(OPEN, target, window, cx);
    });
    assert_eq!(
        window_view(&h, Some(OTHER), cx).0,
        Some(OTHER),
        "the window still shows its own chat"
    );
    assert_eq!(window_view(&h, None, cx), (Some(OPEN), Some(target)));
}

#[gpui_kit::test]
fn a_search_hit_for_a_chat_with_its_own_window_leaves_the_main_window(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    in_window(&h, h.main, None, cx, |app, window, cx| {
        app.select_search_chat(OTHER, window, cx);
    });
    assert_eq!(h.open_chat(cx), Some(OPEN), "the main window kept its chat");
    in_window(&h, h.main, None, cx, |app, window, cx| {
        app.select_search_message(OTHER, OTHER_LAST, window, cx);
    });
    assert_eq!(h.open_chat(cx), Some(OPEN));
    assert_eq!(
        window_view(&h, Some(OTHER), cx),
        (Some(OTHER), Some(OTHER_LAST)),
        "the chat's own window went to the message"
    );
    let _ = chat_window;
}

#[gpui_kit::test]
fn reply_in_another_chat_from_a_chat_window_reaches_the_destination(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    in_window(&h, chat_window, Some(OTHER), cx, |app, window, cx| {
        app.chat_ui
            .set_reply(Some(ComposerReplyTo::new(OTHER, OTHER_LAST)));
        app.choose_reply_chat(OPEN, window, cx);
    });
    h.settle(cx);
    let (main_reply, window_reply) = h.app.read_with(cx, |app, _| {
        let slot = app.windows.slot_for(OTHER).expect("slot");
        let window_reply = slot.borrow().chat_ui.reply().cloned();
        (app.chat_ui.reply().cloned(), window_reply)
    });
    let main_reply = main_reply.expect("the destination's composer has the reply");
    assert_eq!(
        (main_reply.chat_id, main_reply.message_id),
        (OTHER, OTHER_LAST)
    );
    assert_eq!(main_reply.target_chat, Some(OPEN));
    assert!(window_reply.is_none(), "the reply left the source window");
}

#[gpui_kit::test]
fn a_topic_chip_for_a_forum_with_its_own_window_opens_the_topic_there(cx: &mut TestAppContext) {
    use crate::ui::demos::composer::{FORUM_CHAT, RELEASES_TOPIC};
    let h = new_demo_app(cx, "ready-composer-bars");
    let _chat_window = h.open_in_new_window(FORUM_CHAT, cx);
    let main_before = h.open_chat(cx);
    in_window(&h, h.main, None, cx, |app, window, cx| {
        app.select_chat_request(
            ChatOpen::new(FORUM_CHAT).in_topic(RELEASES_TOPIC),
            window,
            cx,
        );
    });
    let topics = h.app.update(cx, |app, cx| {
        let window = app.in_window_scope(Some(FORUM_CHAT), cx, |app, _| {
            app.session().and_then(|session| session.open_topic)
        });
        let main = app.in_window_scope(None, cx, |app, _| {
            app.session().and_then(|session| session.open_topic)
        });
        (window.flatten(), main.flatten())
    });
    assert_eq!(topics, (Some(RELEASES_TOPIC), None));
    assert_eq!(h.open_chat(cx), main_before, "the main window stayed put");
}
