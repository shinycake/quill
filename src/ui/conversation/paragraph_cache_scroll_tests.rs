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

#[gpui_kit::test]
fn a_scrolled_history_keeps_each_paragraph_laid_out_once(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    let rows = rtl_group(&app, vcx);
    scroll_through(&app, vcx, (0..rows).rev());
    let held = paragraph_cache::held_in_tests();
    assert!(
        held.drawn >= POSTS as usize,
        "every post was drawn: {held:?}"
    );
    // Layout measures each post at about four widths; only the one it is
    // drawn at stays laid out.
    assert_eq!(held.laid_out, held.drawn, "{held:?}");
}

#[gpui_kit::test]
fn scrolling_back_lays_out_nothing_again(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    let rows = rtl_group(&app, vcx);
    scroll_through(&app, vcx, (0..rows).rev());
    // Each width a post was measured at was laid out once, and drawing it
    // took the measured layout instead of laying it out again.
    let held = paragraph_cache::held_in_tests();
    assert_eq!(paragraph_cache::layouts_in_tests(), held.widths, "{held:?}");
    let before = paragraph_cache::layouts_in_tests();
    scroll_through(&app, vcx, 0..rows);
    assert_eq!(paragraph_cache::layouts_in_tests(), before);
}

#[gpui_kit::test]
fn resizing_keeps_two_widths_laid_out(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    rtl_group(&app, vcx);
    for width in [1100., 1080., 1060., 1040., 1020., 1000.] {
        resize(&app, vcx, width, 800.);
    }
    let held = paragraph_cache::held_in_tests();
    assert!(held.drawn > 0, "{held:?}");
    assert!(held.laid_out <= 2 * held.drawn, "{held:?}");
}
