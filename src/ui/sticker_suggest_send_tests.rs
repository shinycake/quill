//! Sending an emoji dismisses the sticker and animated-emoji suggestions
//! it raised (tdesktop `updateStickersByEmoji` runs on the field clear
//! after Send), and a suggested sticker takes its emoji's place (tdesktop
//! `sendExistingDocument` clears the field while `stickersShown()`). Over
//! the `ready-chats` demo; nothing reaches Telegram.
#![cfg(feature = "demo-capture")]
#![allow(clippy::unwrap_used, reason = "test code")]

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use quill::ids::FileId;
use quill::telegram::envelope::{StickerFormat, StickerItem};

const EMOJI: &str = "\u{1fae0}";

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

fn sticker(id: i64) -> StickerItem {
    StickerItem {
        custom_emoji_id: None,
        id,
        set_id: 1,
        emoji: EMOJI.into(),
        width: 512,
        height: 512,
        format: StickerFormat::Webp,
        file_id: FileId(0),
        thumb_file_id: None,
        thumb_width: 0,
        thumb_height: 0,
        premium: None,
    }
}

/// What TDLib's `searchStickers` and `getAnimatedEmoji` answers leave
/// for the typed emoji.
fn hold_suggestions(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            let stickers = &mut session.stickers.stickers;
            stickers.suggest_for = quill::state::SuggestFor::Asked {
                emoji: EMOJI.into(),
                request: quill::ids::RequestId(0),
            };
            stickers.suggestions = vec![sticker(1), sticker(2)];
            let emoji = &mut session.stickers.emoji;
            emoji.animated = quill::emoji::AnimatedSuggestion::Ready {
                emoji: EMOJI.into(),
                sticker: Some(sticker(3)),
            };
        }
        cx.notify();
    });
    frames(vcx);
}

/// Which suggestion rows the composer shows: (stickers, animated emoji).
fn shown(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> (bool, bool) {
    app.update(vcx, |app, cx| {
        (
            app.sticker_suggestions_row(cx).is_some(),
            app.animated_emoji_suggestion(cx).is_some(),
        )
    })
}

#[gpui_kit::test]
fn sending_an_emoji_dismisses_its_suggestions(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.composer.update(cx, |input, cx| input.focus(window, cx));
        });
    });
    vcx.simulate_input(EMOJI);
    hold_suggestions(&app, vcx);
    assert_eq!(shown(&app, vcx), (true, true));

    vcx.simulate_keystrokes("enter");
    frames(vcx);
    assert_eq!(
        app.read_with(vcx, |app, cx| app.composer.read(cx).value().to_string()),
        ""
    );
    assert_eq!(shown(&app, vcx), (false, false));
}

#[gpui_kit::test]
fn escape_closes_the_suggestions_until_the_text_changes(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.composer.update(cx, |input, cx| input.focus(window, cx));
        });
    });
    vcx.simulate_input(EMOJI);
    hold_suggestions(&app, vcx);
    assert_eq!(shown(&app, vcx), (true, true));

    vcx.simulate_keystrokes("escape");
    frames(vcx);
    assert_eq!(shown(&app, vcx), (false, false));

    // TDLib's answers arriving again do not bring it back for this text.
    hold_suggestions(&app, vcx);
    assert_eq!(shown(&app, vcx), (false, false));

    // A new emoji is new text.
    vcx.simulate_input(" ");
    vcx.simulate_input(EMOJI);
    frames(vcx);
    assert_eq!(shown(&app, vcx), (true, true));
}

fn composer_text(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> String {
    app.read_with(vcx, |app, cx| app.composer.read(cx).value().to_string())
}

/// Click the suggested sticker the way its cell does.
fn send_suggested(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| app.send_sticker_cell(&sticker(1), window, cx));
    });
    frames(vcx);
}

fn type_in_composer(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, text: &str) {
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.composer.update(cx, |input, cx| input.focus(window, cx));
        });
    });
    vcx.simulate_input(text);
}

#[gpui_kit::test]
fn sending_a_suggested_sticker_clears_its_emoji(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    type_in_composer(&app, vcx, EMOJI);
    hold_suggestions(&app, vcx);
    assert_eq!(shown(&app, vcx), (true, true));

    send_suggested(&app, vcx);
    assert_eq!(composer_text(&app, vcx), "");
    assert_eq!(shown(&app, vcx), (false, false));
}

#[gpui_kit::test]
fn sending_the_animated_emoji_clears_its_emoji(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    type_in_composer(&app, vcx, EMOJI);
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            let emoji = &mut session.stickers.emoji;
            emoji.animated = quill::emoji::AnimatedSuggestion::Ready {
                emoji: EMOJI.into(),
                sticker: Some(sticker(3)),
            };
        }
        cx.notify();
    });
    frames(vcx);
    assert_eq!(shown(&app, vcx), (false, true));

    vcx.update(|window, cx| {
        app.update(cx, |app, cx| app.send_sticker_cell(&sticker(3), window, cx));
    });
    frames(vcx);
    assert_eq!(composer_text(&app, vcx), "");
}

#[gpui_kit::test]
fn a_suggested_sticker_leaves_the_text_before_its_emoji(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    type_in_composer(&app, vcx, &format!("so tired {EMOJI}"));
    hold_suggestions(&app, vcx);

    send_suggested(&app, vcx);
    assert_eq!(composer_text(&app, vcx), "so tired ");
}

#[gpui_kit::test]
fn a_sticker_sent_without_suggestions_keeps_the_text(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    type_in_composer(&app, vcx, EMOJI);
    hold_suggestions(&app, vcx);
    vcx.simulate_keystrokes("escape");
    frames(vcx);
    assert_eq!(shown(&app, vcx), (false, false));

    send_suggested(&app, vcx);
    assert_eq!(composer_text(&app, vcx), EMOJI);
}
