//! A very long composer line through a real kit `Textarea`: an edit costs what
//! it costs on a short line, and rows, the caret, hit tests and right-to-left
//! text work as on a short line, though only the rows near the viewport are
//! shaped. The kit's own tests in `row_wrap.rs` cover the wrap itself; they do
//! not run in CI, these do.

use crate::ui::composer_field_tests::{caret, load, park, with_field};
use crate::ui::composer_rtl::tests::take_shaped;
use gpui_base::input::DisplayPoint;
use gpui_kit::component::input::TextareaState;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{App, AppContext as _, Entity, Pixels, Window, px};
use quill::composer_doc::{ComposerDoc, ComposerTag, DocSpan};

/// A line of this many bytes wraps without shaping and is laid out by viewport.
const LONG: usize = 32 * 1024;

fn repeated(sentence: &str, len: usize) -> String {
    sentence.repeat(len / sentence.len() + 1)
}

fn plain_line(len: usize) -> String {
    repeated("the quick brown fox jumps over a lazy dog ", len)
}

fn mixed_line(len: usize) -> String {
    repeated("the quick brown مرحبا بالعالم jumps over العربية 123 ", len)
}

fn arabic_first_line(len: usize) -> String {
    repeated("مرحبا بالعالم the quick brown جميل ", len)
}

fn prepare(input: &Entity<TextareaState>, window: &mut Window, cx: &mut App) {
    input.update(cx, |state, cx| state.set_auto_grow(1, 8, cx));
    window.render_frame(cx);
    window.click("composer", cx);
}

fn load_text(input: &Entity<TextareaState>, text: &str, window: &mut Window, cx: &mut App) {
    load(input, &ComposerDoc::plain(text), window, cx);
}

/// The buffer column every display row starts at.
fn row_starts(input: &Entity<TextareaState>, cx: &App) -> Vec<usize> {
    let map = input.read(cx).display_map();
    (0..map.display_row_count())
        .map(|row| map.display_pos_to_buffer_pos(DisplayPoint::new(row, 0)).col)
        .collect()
}

fn char_at_or_after(text: &str, at: usize) -> usize {
    (at..text.len())
        .find(|&ix| text.is_char_boundary(ix))
        .unwrap_or(text.len())
}

/// Scroll so that the row holding `offset` is on screen, caret left where it is.
fn scroll_to_row_of(
    input: &Entity<TextareaState>,
    offset: usize,
    window: &mut Window,
    cx: &mut App,
) {
    let row = {
        let map = input.read(cx).display_map();
        (0..map.display_row_count())
            .rev()
            .find(|&row| map.display_pos_to_buffer_pos(DisplayPoint::new(row, 0)).col <= offset)
            .unwrap_or(0)
    };
    let height = input.read(cx).line_height().unwrap_or(px(20.));
    input.update(cx, |state, cx| {
        let x = state.scroll_offset().x;
        state.set_scroll_offset(gpui_kit::point(x, -(height * row as f32)), cx);
    });
    window.render_frame(cx);
    window.render_frame(cx);
}

fn shaped_bytes_of_a_keystroke(
    input: &Entity<TextareaState>,
    len: usize,
    window: &mut Window,
    cx: &mut App,
) -> usize {
    let text = plain_line(len);
    load_text(input, &text, window, cx);
    park(input, text.len() / 2, window, cx);
    window.render_frame(cx);
    take_shaped();
    window.input("x", cx);
    window.render_frame(cx);
    take_shaped().iter().map(String::len).sum()
}

#[test]
fn a_keystroke_shapes_the_rows_on_screen_on_any_length_of_line() {
    with_field(|cx, handle, input| {
        cx.update_window(handle, |_, window, cx| {
            prepare(&input, window, cx);
            let near = shaped_bytes_of_a_keystroke(&input, LONG, window, cx);
            let far = shaped_bytes_of_a_keystroke(&input, 16 * LONG, window, cx);
            assert!(near < LONG / 4, "{near} bytes shaped");
            assert!(far <= near + near / 4, "{near} then {far} bytes shaped");
        })
        .unwrap();
    });
}

/// Types, deletes and replaces all over the line, and compares its rows with a
/// fresh field holding the same text.
fn edits_wrap_like_a_fresh_field(text: String) {
    with_field(|cx, handle, input| {
        cx.update_window(handle, |_, window, cx| {
            prepare(&input, window, cx);
            load_text(&input, &text, window, cx);
            let mut seed = 0x9e37_79b9_u64;
            let mut next = |below: usize| {
                seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                ((seed >> 33) as usize) % below
            };
            for _ in 0..60 {
                let value = input.read(cx).value().to_string();
                let start = char_at_or_after(&value, 1 + next(value.len() - 1));
                let end = char_at_or_after(&value, start + next(6));
                park(&input, start, window, cx);
                input.update(cx, |state, cx| state.set_selected_range(start..end, cx));
                window.input(["x", " ", "نص ", "word "][next(4)], cx);
            }
            window.render_frame(cx);
            let incremental = row_starts(&input, cx);
            let value = input.read(cx).value().to_string();
            load_text(&input, &value, window, cx);
            assert_eq!(incremental, row_starts(&input, cx));
        })
        .unwrap();
    });
}

#[test]
fn edits_on_a_measured_line_wrap_like_a_fresh_field() {
    edits_wrap_like_a_fresh_field(mixed_line(4 * 1024));
}

#[test]
fn edits_on_a_long_line_wrap_like_a_fresh_field() {
    edits_wrap_like_a_fresh_field(mixed_line(LONG));
}

/// A text of this many bytes of short lines wraps without shaping.
const LONG_TEXT: usize = 256 * 1024;

fn log_lines(len: usize) -> String {
    repeated("12:00:01 [INFO] [worker-3] a request came in from the proxy\n", len)
}

#[test]
fn a_paste_of_many_short_lines_shapes_the_rows_on_screen() {
    with_field(|cx, handle, input| {
        cx.update_window(handle, |_, window, cx| {
            prepare(&input, window, cx);
            let text = log_lines(4 * LONG_TEXT);
            take_shaped();
            load_text(&input, &text, window, cx);
            window.render_frame(cx);
            let shaped: usize = take_shaped().iter().map(String::len).sum();
            assert!(shaped < LONG, "{shaped} bytes shaped");
        })
        .unwrap();
    });
}

#[test]
fn crossing_the_long_text_mark_wraps_like_a_fresh_field() {
    with_field(|cx, handle, input| {
        cx.update_window(handle, |_, window, cx| {
            prepare(&input, window, cx);
            let wide = format!("{}\n", repeated("many words on a line of text ", 2_000));
            let text = format!("{wide}{}", log_lines(LONG_TEXT - wide.len() - 2));
            load_text(&input, &text, window, cx);
            let end = text.len();
            for (selected, typed) in [(end..end, "word "), (0..wide.len() + 16, "")] {
                input.update(cx, |state, cx| state.set_selected_range(selected, cx));
                window.input(typed, cx);
                window.render_frame(cx);
                let incremental = row_starts(&input, cx);
                let value = input.read(cx).value().to_string();
                load_text(&input, &value, window, cx);
                assert_eq!(incremental, row_starts(&input, cx));
                load_text(&input, &format!("{text}word "), window, cx);
            }
        })
        .unwrap();
    });
}

#[test]
fn a_jump_to_the_far_end_shows_it_in_the_same_frame() {
    with_field(|cx, handle, input| {
        cx.update_window(handle, |_, window, cx| {
            prepare(&input, window, cx);
            let text = plain_line(LONG);
            load_text(&input, &text, window, cx);
            input.update(cx, |state, cx| {
                state.set_selected_range(text.len()..text.len(), cx)
            });
            window.render_frame(cx);
            let starts = row_starts(&input, cx);
            let last_row = text.len() - starts.last().copied().unwrap_or(0);
            let end = caret(&input, text.len(), cx);
            let row_start = caret(&input, text.len() - last_row, cx);
            assert!(
                end.left() - row_start.left() > px(last_row as f32),
                "the caret is shaped on the last row: {end:?} {row_start:?}"
            );
            assert_eq!(end.top(), row_start.top());
        })
        .unwrap();
    });
}

#[test]
fn rows_far_down_a_long_line_answer_like_the_first_rows() {
    with_field(|cx, handle, input| {
        cx.update_window(handle, |_, window, cx| {
            prepare(&input, window, cx);
            let text = plain_line(LONG);
            load_text(&input, &text, window, cx);
            let middle = text.len() / 2;
            scroll_to_row_of(&input, middle, window, cx);
            let starts = row_starts(&input, cx);
            let row = starts.partition_point(|&start| start <= middle);
            let [_, start, next, after] = starts[row - 2..row + 2] else {
                panic!("rows around the middle");
            };

            // A range over two rows gives two rectangles, one row apart.
            let rects = input.read(cx).range_to_rects(&(start + 4..after - 1));
            assert_eq!(rects.len(), 2, "{rects:?}");
            let row_height = rects[1].top() - rects[0].top();
            assert!(row_height > px(0.));

            // A caret stands where the range starts, and a click there lands on it.
            park(&input, start + 4, window, cx);
            let at = caret(&input, start + 4, cx);
            assert!((rects[0].left() - at.left()).abs() < px(0.5));
            let composer = window.find("composer").bounds();
            window.click_at("composer", at.center() - composer.origin, cx);
            assert_eq!(input.read(cx).cursor(), start + 4);

            // Up and down keep the column, from row to row.
            window.press("down", cx);
            window.render_frame(cx);
            assert_eq!(input.read(cx).cursor(), next + 4);
            assert!((caret(&input, next + 4, cx).left() - at.left()).abs() < px(1.));
            window.press("up", cx);
            window.render_frame(cx);
            assert_eq!(input.read(cx).cursor(), start + 4);
        })
        .unwrap();
    });
}

#[test]
fn a_mixed_line_keeps_its_directions_far_from_where_it_starts() {
    with_field(|cx, handle, input| {
        cx.update_window(handle, |_, window, cx| {
            prepare(&input, window, cx);
            let text = mixed_line(LONG);
            load_text(&input, &text, window, cx);
            let word =
                text.get(text.len() / 2..).unwrap().find("بالعالم").unwrap() + text.len() / 2;
            scroll_to_row_of(&input, word, window, cx);
            // Right to left inside the word, left to right after it.
            let letter = "ب".len();
            let x = |at: usize, cx: &App| caret(&input, at, cx).left();
            assert!(x(word + letter, cx) < x(word, cx));
            let latin = text.get(word..).unwrap().find("jumps").unwrap() + word;
            assert!(x(latin + 2, cx) > x(latin, cx));
            let right_edge = |input: &Entity<TextareaState>, cx: &App| -> Pixels {
                let bounds = input.read(cx).range_to_rects(&(word..word + letter));
                bounds[0].right()
            };
            let left_to_right = right_edge(&input, cx);

            // The base direction comes from the start of the line, here right to left.
            let text = arabic_first_line(LONG);
            load_text(&input, &text, window, cx);
            let middle = char_at_or_after(&text, text.len() / 2);
            scroll_to_row_of(&input, middle, window, cx);
            let word = text.get(middle..).unwrap().find("the quick").unwrap() + middle;
            let starts = row_starts(&input, cx);
            let row = starts.partition_point(|&start| start <= word);
            let (start, end) = (starts[row - 1], starts[row]);
            let rects = input.read(cx).range_to_rects(&(start..end));
            let right = rects.iter().map(|r| r.right()).fold(px(0.), Pixels::max);
            assert!(
                right >= left_to_right,
                "a right-to-left paragraph hugs the right edge: {rects:?}"
            );
        })
        .unwrap();
    });
}

#[test]
fn arrows_move_a_caret_scrolled_away_from_its_row() {
    with_field(|cx, handle, input| {
        cx.update_window(handle, |_, window, cx| {
            prepare(&input, window, cx);
            let text = mixed_line(LONG);
            load_text(&input, &text, window, cx);
            let middle = char_at_or_after(&text, text.len() / 2);
            scroll_to_row_of(&input, middle, window, cx);
            assert_eq!(input.read(cx).cursor(), 0);
            window.press("right", cx);
            assert_eq!(input.read(cx).cursor(), 1);
            window.render_frame(cx);
            window.render_frame(cx);
            assert!(input.read(cx).range_to_bounds(&(1..1)).is_some());
        })
        .unwrap();
    });
}

#[test]
fn formatting_and_emoji_stay_in_place_far_down_a_long_line() {
    with_field(|cx, handle, input| {
        cx.update_window(handle, |_, window, cx| {
            prepare(&input, window, cx);
            let text = plain_line(LONG);
            let bold = text.len() / 2 - text.len() / 2 % 42;
            let mut doc = ComposerDoc::plain(&text);
            doc.spans = vec![DocSpan::new(bold + 4..bold + 19, ComposerTag::Bold)];
            load(&input, &doc, window, cx);
            scroll_to_row_of(&input, bold, window, cx);
            let plain_x = caret(&input, bold + 4, cx).left() - caret(&input, bold, cx).left();
            assert!(plain_x > px(0.));
            let edited = bold + 8;
            park(&input, edited, window, cx);
            window.input("x", cx);
            window.render_frame(cx);
            let doc = crate::ui::composer_field::field_doc(input.read(cx));
            assert_eq!(doc.spans[0].range, bold + 4..bold + 20);
        })
        .unwrap();
    });
}
