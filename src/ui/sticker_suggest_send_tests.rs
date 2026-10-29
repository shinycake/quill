//! Sending an emoji dismisses the sticker and animated-emoji suggestions
//! it raised (tdesktop `updateStickersByEmoji` runs on the field clear
//! after Send). Over the `ready-chats` demo; nothing reaches Telegram.
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
        requires_premium: false,
        premium_animation: None,
    }
}

/// What TDLib's `searchStickers` and `getAnimatedEmoji` answers leave
/// for the typed emoji.
fn hold_suggestions(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            let stickers = &mut session.stickers.stickers;
            stickers.suggest_for = Some(EMOJI.into());
            stickers.suggestions = vec![sticker(1), sticker(2)];
            let emoji = &mut session.stickers.emoji;
            emoji.animated_emoji_for = Some(EMOJI.into());
            emoji.animated_emoji = Some(sticker(3));
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
