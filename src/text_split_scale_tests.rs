//! A pasted text can be a megabyte, and the composer parses and splits it on
//! the UI thread, so that work has to grow in step with the text.

use super::split_markup_text;
use crate::composer::{parse_format_markup, scan_work};

const SMALL: usize = 10_000;
const LARGE: usize = 1_000_000;
/// How much more work per byte the large text may take. A cut's window sits
/// at a different place in the text, so the ratio is never exactly 1.
const SLACK: u64 = 2;

/// `line(i)` repeated up to `size` bytes, one line each.
fn lines(size: usize, line: impl Fn(usize) -> String) -> String {
    let mut text = String::with_capacity(size + 256);
    let mut i = 0;
    while text.len() < size {
        text.push_str(&line(i));
        text.push('\n');
        i += 1;
    }
    text
}

/// `word(i)` repeated up to `size` bytes on one line.
fn one_line(size: usize, word: impl Fn(usize) -> String) -> String {
    let mut text = String::with_capacity(size + 256);
    let mut i = 0;
    while text.len() < size {
        text.push_str(&word(i));
        i += 1;
    }
    text
}

fn pastes(size: usize) -> Vec<(&'static str, String)> {
    vec![
        (
            "words",
            lines(size, |i| format!("the quick brown fox {i} jumps over it.")),
        ),
        (
            "log lines",
            lines(size, |i| {
                format!(
                    "12:00:{:02} [INFO] [worker-{}] request {i} done",
                    i % 60,
                    i % 8
                )
            }),
        ),
        (
            "json",
            lines(size, |i| {
                format!("{{\"id\": {i}, \"user_name\": \"user_{i}\", \"tags\": [\"a_b\"]}},")
            }),
        ),
        (
            "quote lines",
            lines(size, |i| format!("> quoted line number {i} of a long mail")),
        ),
        (
            "formatted lines",
            lines(size, |i| {
                format!("some **bold {i}** and `code` and _more_ text")
            }),
        ),
        (
            "links",
            lines(size, |i| {
                format!("see [item {i}](https://example.com/{i}) now")
            }),
        ),
        (
            "fences",
            lines(size, |i| match i % 40 {
                0 => "```rust".to_string(),
                39 => "```".to_string(),
                _ => format!("let value_{i} = {i};"),
            }),
        ),
        (
            "unclosed brackets",
            lines(size, |i| format!("[{i} and more")),
        ),
        (
            "unclosed links",
            one_line(size, |i| format!("[{i}](no end ")),
        ),
        ("stars on one line", one_line(size, |i| format!("*{i} "))),
        (
            "backticks on one line",
            one_line(size, |i| format!("`{i} ")),
        ),
        (
            "one fence",
            format!("```\n{}\n```", lines(size, |i| format!("line {i}"))),
        ),
        (
            "one quote line",
            format!("> {}", one_line(size, |i| format!("{i} "))),
        ),
    ]
}

fn assert_linear(work: impl Fn(&str)) {
    for ((name, small), (_, large)) in pastes(SMALL).into_iter().zip(pastes(LARGE)) {
        let small_work = scan_work::measure(u64::MAX, || work(&small));
        let per_byte = small_work.div_ceil(small.len() as u64).max(1);
        let budget = per_byte * SLACK * large.len() as u64;
        println!("{name}");
        let large_work = scan_work::measure(budget, || work(&large));
        assert!(
            large_work >= large.len() as u64,
            "{name}: the work never ran"
        );
    }
}

#[test]
fn parsing_work_grows_with_the_text() {
    assert_linear(|text| {
        parse_format_markup(text);
    });
}

#[test]
fn splitting_work_grows_with_the_text() {
    assert_linear(|text| {
        assert!(split_markup_text(text, 4096).len() > 1);
    });
}
