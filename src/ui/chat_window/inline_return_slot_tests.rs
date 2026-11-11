//! A round trip begun by the inline start-bot button inside a chat window
//! ends when another link overtakes the lookup, like the main window's
//! (sweep 10-119): the start is never sent into a chat opened later.

use super::super::tests::{OPEN, OTHER, new_app};
use gpui_kit::TestAppContext;
use quill::ids::ChatId;

const BOT: i64 = 900_001;

fn waiting_in_window(
    h: &super::super::tests::Harness,
    chat: ChatId,
    cx: &mut TestAppContext,
) -> bool {
    h.app
        .update(cx, |app, cx| {
            app.in_window_scope(Some(chat), cx, |app, _| {
                app.composer_ui.inline_return.is_some()
            })
        })
        .unwrap_or(false)
}

#[gpui_kit::test]
fn a_link_ending_elsewhere_ends_a_chat_windows_round_trip(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let _chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, cx| {
        app.in_window_scope(Some(OTHER), cx, |app, _| {
            app.begin_inline_return(BOT, "somebot", "param");
        });
    });
    assert!(
        waiting_in_window(&h, OTHER, cx),
        "the window waits for the bot"
    );
    // The bot's own chat keeps it waiting.
    h.app.update(cx, |app, _| {
        app.cancel_inline_opening_unless_chat(ChatId(BOT));
    });
    assert!(waiting_in_window(&h, OTHER, cx));
    // Another link that ended in a different chat, from the main window.
    h.app.update(cx, |app, _| {
        app.cancel_inline_opening_unless_chat(OPEN);
    });
    assert!(
        !waiting_in_window(&h, OTHER, cx),
        "the overtaken round trip left nothing to send"
    );
}

#[gpui_kit::test]
fn a_failed_lookup_ends_a_chat_windows_round_trip(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let _chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, cx| {
        app.in_window_scope(Some(OTHER), cx, |app, _| {
            app.begin_inline_return(BOT, "somebot", "param");
        });
    });
    h.app.update(cx, |app, _| app.cancel_inline_opening());
    assert!(!waiting_in_window(&h, OTHER, cx));
}
