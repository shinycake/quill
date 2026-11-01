//! A chat window with its own slices: what changes in it redraws only the
//! part that shows it, as in the main window, and the two windows' parts
//! stay apart. Over the demo; nothing reaches Telegram.

use super::*;
use crate::ui::frame_clock::take_slice_renders;

/// A chat window on the open chat, its stickers playing, and a context to
/// type into it.
fn opened(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext, VisualTestContext) {
    let (app, mut vcx) = open(cx);
    let chat = open_chat_window(&app, &mut vcx, OPEN, beside());
    for _ in 0..4 {
        app.read_with(&vcx, |app, _| app.playback.stickers.settle());
        app.update(&mut vcx, |_, cx| cx.notify());
        let _ = frames_over(&mut vcx, 1);
    }
    (app, vcx, VisualTestContext::from_window(chat, cx))
}

/// Typing redraws the composer, and the history above it stays.
#[gpui_kit::test]
fn a_keystroke_in_a_chat_window_builds_no_history(cx: &mut TestAppContext) {
    let (_app, _vcx, mut chat) = opened(cx);
    chat.simulate_keystrokes("a");
    chat.run_until_parked();
    let _ = take_slice_renders();
    chat.simulate_keystrokes("b");
    chat.run_until_parked();
    let drawn = take_slice_renders();
    assert_eq!(drawn.get("history"), None, "the history stayed: {drawn:?}");
    assert!(drawn.contains_key("chat window composer"), "the composer: {drawn:?}");
}

/// The caret's blink notifies the field alone.
#[gpui_kit::test]
fn a_caret_blink_in_a_chat_window_builds_no_history(cx: &mut TestAppContext) {
    let (app, _vcx, mut chat) = opened(cx);
    app.update(&mut chat, |app, _| app.set_window_scope(Some(OPEN)));
    let field = app.read_with(&chat, |app, _| app.composer.clone());
    chat.run_until_parked();
    let _ = take_slice_renders();
    field.update(&mut chat, |_, cx| cx.notify());
    chat.run_until_parked();
    let drawn = take_slice_renders();
    assert_eq!(drawn.get("history"), None, "the history stayed: {drawn:?}");
    assert!(drawn.contains_key("chat window composer"), "the composer: {drawn:?}");
}

/// A tick of its stickers redraws the window's layer, and neither of its
/// slices.
#[gpui_kit::test]
fn a_chat_windows_stickers_tick_in_its_layer_alone(cx: &mut TestAppContext) {
    let (_app, mut vcx, _chat) = opened(cx);
    let drawn = frames_over(&mut vcx, 2);
    assert!(
        drawn.get("chat window layer").is_some_and(|n| *n > 5),
        "they play: {drawn:?}"
    );
    for part in ["history", "conversation", "composer", "app"] {
        assert_eq!(drawn.get(part), None, "{part} stayed: {drawn:?}");
    }
}

/// The main window opening another chat leaves the stickers of the chat
/// window's chat playing: they are that window's, however the main window
/// lets go of its own.
#[gpui_kit::test]
fn a_main_window_chat_switch_leaves_a_chat_windows_stickers_playing(cx: &mut TestAppContext) {
    let (app, mut vcx, _chat) = opened(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.select_listed_chat(OTHER, window, cx);
    });
    vcx.run_until_parked();
    let drawn = frames_over(&mut vcx, 2);
    assert!(
        drawn.get("chat window layer").is_some_and(|n| *n > 5),
        "they play on: {drawn:?}"
    );
}

/// A window whose session view is gone draws nothing of whatever is in
/// place instead, and closes.
#[gpui_kit::test]
fn a_chat_window_that_lost_its_session_view_builds_no_history(cx: &mut TestAppContext) {
    let (app, mut vcx, mut chat) = opened(cx);
    let window = chat.update(|window, _| window.window_handle());
    app.update(&mut vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.close_window_chat(OPEN);
        }
        cx.notify();
    });
    let _ = take_slice_renders();
    chat.run_until_parked();
    let drawn = take_slice_renders();
    assert_eq!(drawn.get("history"), None, "nothing shows: {drawn:?}");
    assert!(
        !vcx.windows().contains(&window),
        "the window closed, as it must not show another window's chat"
    );
}

/// A chat window with stickers behind the active main window.
fn held(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext, AnyWindowHandle) {
    let (app, mut vcx) = open(cx);
    let under = open_chat_window(&app, &mut vcx, OPEN, inside());
    for _ in 0..4 {
        app.read_with(&vcx, |app, _| app.playback.stickers.settle());
        app.update(&mut vcx, |_, cx| cx.notify());
        let _ = frames_over(&mut vcx, 1);
    }
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let _ = frames_of(under, &mut vcx);
    let _ = take_slice_renders();
    (app, vcx, under)
}

/// A held chat window draws nothing whatever the app does. Once it shows
/// again it draws what changed, and its stickers play on.
#[gpui_kit::test]
fn a_held_chat_window_catches_up_when_it_shows_again(cx: &mut TestAppContext) {
    let (app, mut vcx, under) = held(cx);
    let vcx = &mut vcx;
    app.update(vcx, |_, cx| cx.notify());
    let drawn = frames_over(vcx, 1);
    assert_eq!(frames_of(under, vcx), 0, "held: nothing drawn");
    for part in ["chat window conversation", "chat window composer", "chat window layer"] {
        assert_eq!(drawn.get(part), None, "{part} held: {drawn:?}");
    }

    let _ = take_slice_renders();
    vcx.simulate_window_move(under, beside().origin);
    vcx.run_until_parked();
    let drawn = take_slice_renders();
    assert!(drawn.contains_key("chat window conversation"), "it caught up: {drawn:?}");
    let drawn = frames_over(vcx, 1);
    assert!(
        drawn.get("chat window layer").is_some_and(|n| *n > 5),
        "its stickers play again: {drawn:?}"
    );
}

/// Activating a held window, as Cmd-` or the Window menu do, shows it.
#[gpui_kit::test]
fn activating_a_held_chat_window_shows_it(cx: &mut TestAppContext) {
    let (_app, mut vcx, under) = held(cx);
    let vcx = &mut vcx;
    let _ = take_slice_renders();
    vcx.update_window(under, |_, window, _| window.activate_window())
        .expect("the window");
    vcx.run_until_parked();
    let drawn = take_slice_renders();
    assert!(drawn.contains_key("chat window conversation"), "it shows: {drawn:?}");
}
