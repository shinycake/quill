//! The viewer over a chat with two looping stickers: they play through the
//! viewer's fade, hold still once its black backdrop hides them, and play
//! again from the moment it closes. Over the demo; nothing reaches Telegram.

use super::super::navigation::wheel_ui_tests::{add_photo, open as open_viewer};
use crate::ui::anim_layer::layer_tick_tests::open_stickers;
use crate::ui::app::QuillApp;
use crate::ui::frame_clock::frames_over;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use std::collections::BTreeMap;

const PHOTO: i64 = 900;

fn count(drawn: &BTreeMap<&'static str, u32>, kind: &str) -> u32 {
    drawn.get(kind).copied().unwrap_or(0)
}

/// GPUI's fade runs by the wall clock, which hardly moves here, so it keeps
/// asking for frames.
fn frames_not_from_the_fade(drawn: &BTreeMap<&'static str, u32>) -> u32 {
    count(drawn, "(window frame)").saturating_sub(count(drawn, "(next frame)"))
}

fn clock_runs(app: &Entity<QuillApp>, vcx: &VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| app.frame.frame_clock_running.get())
}

#[gpui_kit::test]
fn the_chat_behind_the_viewer_holds_still_until_it_closes(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_stickers(cx);
    let vcx = &mut vcx;
    add_photo(&app, vcx, PHOTO);
    let _ = frames_over(vcx, 1);
    let alone = frames_over(vcx, 2);
    assert!(count(&alone, "animation layer") > 10, "it plays: {alone:?}");

    open_viewer(&app, vcx, PHOTO);
    let fade = frames_over(vcx, 1);
    assert!(
        count(&fade, "animation layer") > 0,
        "the chat shows through the fade: {fade:?}"
    );
    let hidden = frames_over(vcx, 2);
    assert_eq!(frames_not_from_the_fade(&hidden), 0, "{hidden:?}");
    assert!(!clock_runs(&app, vcx), "nothing asks for ticks");

    app.update(vcx, |app, cx| app.close_media_viewer(cx));
    let back = frames_over(vcx, 2);
    assert!(count(&back, "animation layer") > 10, "it plays: {back:?}");
}

#[gpui_kit::test]
fn a_chat_left_behind_another_opening_shows_through_its_fade(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_stickers(cx);
    let vcx = &mut vcx;
    add_photo(&app, vcx, PHOTO);
    open_viewer(&app, vcx, PHOTO);
    let _ = frames_over(vcx, 2);
    app.update(vcx, |app, cx| app.close_media_viewer(cx));
    open_viewer(&app, vcx, PHOTO);
    let fade = frames_over(vcx, 1);
    assert!(
        count(&fade, "animation layer") > 0,
        "a new fade shows the chat again: {fade:?}"
    );
    let hidden = frames_over(vcx, 1);
    assert_eq!(frames_not_from_the_fade(&hidden), 0, "{hidden:?}");
}
