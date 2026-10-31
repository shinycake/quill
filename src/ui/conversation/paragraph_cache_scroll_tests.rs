//! Laid-out text a history of long right-to-left posts leaves in the
//! paragraph cache (`docs/decisions/paragraph-cache-widths.md`).
use super::*;
use crate::ui::paragraph_cache;

const POSTS: i64 = 40;

/// A long post in a right-to-left script, about 300 characters, with a
/// number and a link: letters picked by code point from `n`.
fn post(n: i64) -> String {
    let mut seed = n as u64 * 7919 + 1;
    let mut words = Vec::new();
    for w in 0..60 {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let len = 2 + (seed >> 33) % 6;
        let word: String = (0..len)
            .filter_map(|i| char::from_u32(0x5D0 + ((seed >> (i * 5)) % 27) as u32))
            .collect();
        words.push(word);
        if w == 30 {
            words.push("2024,".into());
        }
    }
    format!("{} https://example.com/{n}", words.join(" "))
}

fn rtl_group(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> usize {
    paragraph_cache::reset_in_tests();
    open_group(app, cx);
    let updates: Vec<String> = (0..POSTS)
        .map(|n| {
            let id = 1000 + n;
            let date = 1790632000 + n * 60;
            let text = serde_json::to_string(&post(n)).expect("a string");
            format!(
                r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":31,"sender_id":{{"@type":"messageSenderUser","user_id":21}},"is_outgoing":false,"date":{date},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{text},"entities":[]}}}}}}}}"#
            )
        })
        .collect();
    let refs: Vec<&str> = updates.iter().map(String::as_str).collect();
    apply(app, cx, &refs);
    app.read_with(cx, |app, _| app.history.rows.len())
}

/// Brings each row in `rows` into view in turn, a frame each.
fn scroll_through(
    app: &Entity<QuillApp>,
    cx: &mut VisualTestContext,
    rows: impl Iterator<Item = usize>,
) {
    for row in rows {
        app.update(cx, |app, cx| {
            app.history
                .scroller
                .update(cx, |state, cx| state.scroll_to_item(row, cx));
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }
}

/// One more frame that draws everything again, which ends the one before.
fn redraw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

#[gpui_kit::test]
fn a_scrolled_history_keeps_each_paragraph_laid_out_once(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    let rows = rtl_group(&app, vcx);
    scroll_through(&app, vcx, (0..rows).rev());
    redraw(vcx);
    let held = paragraph_cache::held_in_tests(false);
    assert!(
        held.drawn >= POSTS as usize,
        "every post was drawn: {held:?}"
    );
    // Layout measures each post at about four widths; only the one it is
    // drawn at stays laid out, and a row measured just outside the
    // viewport keeps one layout for when it scrolls in.
    assert_eq!(held.laid_out, held.drawn, "{held:?}");
    assert!(held.waiting_laid_out <= held.waiting, "{held:?}");
    assert_eq!(paragraph_cache::relayouts_in_tests(), 0);
}

#[gpui_kit::test]
fn scrolling_back_lays_out_nothing_again(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    let rows = rtl_group(&app, vcx);
    scroll_through(&app, vcx, (0..rows).rev());
    let before = paragraph_cache::layouts_in_tests();
    scroll_through(&app, vcx, 0..rows);
    redraw(vcx);
    assert_eq!(paragraph_cache::layouts_in_tests(), before);
    // Drawing took the layout its measure made, never a second one.
    assert_eq!(paragraph_cache::relayouts_in_tests(), 0);
}

/// Layouts made while the window is resized to each of `widths`, a few
/// frames each, as a window or splitter dragged across them.
fn layouts_resizing(app: &Entity<QuillApp>, cx: &mut VisualTestContext, widths: &[f32]) -> u64 {
    let before = paragraph_cache::layouts_in_tests();
    for width in widths {
        resize(app, cx, *width, 800.);
    }
    paragraph_cache::layouts_in_tests() - before
}

#[gpui_kit::test]
fn resizing_lays_out_each_width_once(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    rtl_group(&app, vcx);
    let laid = layouts_resizing(&app, vcx, &[1000., 900., 800., 700., 640.]);
    redraw(vcx);
    assert!(laid > 0, "the posts rewrapped");
    assert_eq!(paragraph_cache::relayouts_in_tests(), 0);
    let held = paragraph_cache::held_in_tests(false);
    assert!(held.drawn > 0, "{held:?}");
    // Widths only measured let their layouts go once their frame was over,
    // but for the one a paragraph measured and not drawn waits on.
    assert!(held.most_measured_only <= 1, "{held:?}");
}

/// A history narrower than a bubble's widest, as with a panel closed (A),
/// a panel open (B), the window maximized (C), then the panel closed again.
#[gpui_kit::test]
fn going_back_to_an_earlier_width_lays_out_nothing(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    rtl_group(&app, vcx);
    // The window opens at C, where a bubble is as wide as it gets.
    for width in [700., 640.] {
        assert!(
            layouts_resizing(&app, vcx, &[width]) > 0,
            "the posts rewrapped at {width}"
        );
    }
    assert_eq!(
        layouts_resizing(&app, vcx, &[1000., 700., 640., 1000., 700.]),
        0
    );
    assert_eq!(paragraph_cache::relayouts_in_tests(), 0);
}

/// A splitter dragged across a dozen widths and back over them.
#[gpui_kit::test]
fn dragging_back_over_widths_lays_out_nothing(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    rtl_group(&app, vcx);
    let out: Vec<f32> = (0..12).map(|step| 750. - 10. * step as f32).collect();
    assert!(layouts_resizing(&app, vcx, &out) > 0, "the posts rewrapped");
    let back: Vec<f32> = out.iter().rev().copied().collect();
    assert_eq!(layouts_resizing(&app, vcx, &back), 0);
    assert_eq!(layouts_resizing(&app, vcx, &out), 0);
    assert_eq!(paragraph_cache::relayouts_in_tests(), 0);
}
