//! The composer's emoji suggestions end to end: typed text, keyword
//! answers landing, keys. Over the `ready-chats` demo with the keyword
//! answers put in the session by hand; nothing reaches Telegram.

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use quill::ids::RequestId;
use quill::suggest::SuggestKind;
use quill::telegram::envelope_emoji::EmojiKeyword;
use std::time::Duration;

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
    vcx.update(|window, cx| {
        for _ in 0..2 {
            window.render_frame(cx);
        }
    });
    vcx.run_until_parked();
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.composer.update(cx, |input, cx| input.focus(window, cx));
        });
    });
    (app, vcx)
}

/// TDLib answering `searchEmojis` for `query`.
fn answer(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, query: &str, hits: &[(&str, &str)]) {
    app.update(vcx, |app, _| {
        let lookups = &mut app
            .demo_session
            .as_mut()
            .expect("demo session")
            .stickers
            .emoji
            .keyword_lookups;
        let id = RequestId(900_000 + lookups.generation);
        lookups.sent(id, query.to_string());
        lookups.answered(
            id,
            hits.iter()
                .map(|(emoji, keyword)| EmojiKeyword {
                    emoji: emoji.to_string(),
                    keyword: keyword.to_string(),
                })
                .collect(),
        );
    });
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.run_until_parked();
}

fn shown(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
) -> Option<(SuggestKind, Vec<String>, Option<usize>)> {
    app.read_with(vcx, |app, _| {
        let ui = &app.composer_ui.suggest;
        ui.active().map(|a| {
            (
                a.query.kind,
                a.items.iter().map(|i| i.glyph.clone()).collect(),
                a.selected,
            )
        })
    })
}

fn text(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> String {
    app.read_with(vcx, |app, cx| app.composer.read(cx).value().to_string())
}

#[gpui_kit::test]
fn colon_query_shows_keyword_emoji_once_they_arrive(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.simulate_input("this is an example :happy");
    // `happy` is no emoji name, so nothing shows until the keywords land.
    assert_eq!(shown(&app, vcx), None);
    answer(&app, vcx, "happy", &[("😂", "happy"), ("😊", "happy")]);
    let (kind, glyphs, selected) = shown(&app, vcx).expect("the strip shows");
    assert_eq!(kind, SuggestKind::Emoji);
    assert_eq!(glyphs[..2], ["😂", "😊"]);
    assert_eq!(selected, Some(0), "a `:` query highlights the first emoji");
    // Right moves the highlight; Enter inserts it in place of `:happy`.
    vcx.simulate_keystrokes("right");
    assert_eq!(shown(&app, vcx).and_then(|s| s.2), Some(1));
    vcx.update(|window, cx| window.press("enter", cx));
    assert_eq!(text(&app, vcx), "this is an example 😊");
    assert_eq!(shown(&app, vcx), None);
}

#[gpui_kit::test]
fn a_lone_word_waits_and_starts_with_nothing_highlighted(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    answer(&app, vcx, "funny", &[("😂", "funny"), ("🤡", "funnyman")]);
    vcx.simulate_input("funny");
    assert_eq!(
        shown(&app, vcx),
        None,
        "tdesktop waits before showing a word"
    );
    vcx.executor().advance_clock(Duration::from_millis(300));
    vcx.run_until_parked();
    let (kind, glyphs, selected) = shown(&app, vcx).expect("the strip shows");
    assert_eq!(
        (kind, glyphs, selected),
        (SuggestKind::EmojiWord, vec!["😂".to_string()], None)
    );
    // Tab takes the first.
    vcx.simulate_keystrokes("tab");
    assert_eq!(text(&app, vcx), "😂");
}

/// Left with nothing highlighted is the caret's, and a caret off the end
/// of the word closes the strip (tdesktop `handleCursorPositionChange`).
#[gpui_kit::test]
fn moving_the_caret_off_a_lone_word_closes_its_strip(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    answer(&app, vcx, "funny", &[("😂", "funny")]);
    vcx.simulate_input("funny");
    vcx.executor().advance_clock(Duration::from_millis(300));
    vcx.run_until_parked();
    assert!(shown(&app, vcx).is_some());
    vcx.simulate_keystrokes("left");
    vcx.update(|window, cx| window.render_frame(cx));
    assert_eq!(shown(&app, vcx), None);
    assert_eq!(text(&app, vcx), "funny");
}

#[gpui_kit::test]
fn enter_still_sends_under_a_lone_word_strip(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    answer(&app, vcx, "funny", &[("😂", "funny")]);
    vcx.simulate_input("funny");
    vcx.executor().advance_clock(Duration::from_millis(300));
    vcx.run_until_parked();
    assert!(shown(&app, vcx).is_some());
    vcx.update(|window, cx| window.press("enter", cx));
    assert_eq!(text(&app, vcx), "", "the word was sent, not replaced");
}

#[gpui_kit::test]
fn a_new_list_keeps_the_highlighted_emoji(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.simulate_input("hot :fire");
    // The catalog's names first; Right highlights the second.
    let before = shown(&app, vcx).expect("catalog rows");
    vcx.simulate_keystrokes("right");
    let picked = before.1[1].clone();
    // Keywords land and put another emoji in front.
    answer(
        &app,
        vcx,
        "fire",
        &[("🧯", "fire"), (picked.as_str(), "fire")],
    );
    let after = shown(&app, vcx).expect("keyword rows");
    let at = after.2.expect("still highlighted");
    assert_eq!(after.1[at], picked);
}

#[gpui_kit::test]
fn escape_keeps_the_strip_closed_for_that_query(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    answer(&app, vcx, "fire", &[("🔥", "fire")]);
    vcx.simulate_input("hot :fire");
    assert!(shown(&app, vcx).is_some());
    vcx.simulate_keystrokes("escape");
    assert_eq!(shown(&app, vcx), None);
    // A new keyword answer does not bring it back.
    answer(&app, vcx, "fire", &[("🔥", "fire"), ("🧯", "fire")]);
    assert_eq!(shown(&app, vcx), None);
}

/// The bug from 2026-11-11: "Yes" sent before the lone-word delay ran out
/// left the delay running, and the strip opened over the empty field.
#[gpui_kit::test]
fn a_word_sent_before_its_delay_never_opens_the_strip(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    answer(&app, vcx, "yes", &[("👍", "yes"), ("👌", "yes")]);
    vcx.simulate_input("yes");
    vcx.update(|window, cx| window.press("enter", cx));
    assert_eq!(text(&app, vcx), "");
    vcx.executor().advance_clock(Duration::from_millis(300));
    vcx.run_until_parked();
    vcx.update(|window, cx| window.render_frame(cx));
    assert_eq!(shown(&app, vcx), None);
}

/// Sending under an open strip closes it.
#[gpui_kit::test]
fn sending_closes_an_open_word_strip(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    answer(&app, vcx, "yes", &[("👍", "yes")]);
    vcx.simulate_input("yes");
    vcx.executor().advance_clock(Duration::from_millis(300));
    vcx.run_until_parked();
    assert!(shown(&app, vcx).is_some());
    vcx.update(|window, cx| window.press("enter", cx));
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.run_until_parked();
    assert_eq!(text(&app, vcx), "");
    assert_eq!(shown(&app, vcx), None);
    // Nor does a later keyword answer bring it back.
    answer(&app, vcx, "yes", &[("👍", "yes"), ("💪", "yes")]);
    assert_eq!(shown(&app, vcx), None);
}

/// A field cleared or replaced without a key press (a draft or chat
/// switch, the clear after a send) closes the strip at the next frame.
#[gpui_kit::test]
fn a_field_changed_in_code_closes_the_strip(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    answer(&app, vcx, "fire", &[("🔥", "fire")]);
    vcx.simulate_input("hot :fire");
    assert!(shown(&app, vcx).is_some());
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.composer
                .update(cx, |input, cx| input.set_value("another draft", window, cx));
        });
        window.render_frame(cx);
    });
    assert_eq!(shown(&app, vcx), None);
}
