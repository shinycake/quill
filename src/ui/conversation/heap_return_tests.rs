//! What handing freed heap back gives on Linux (glibc `malloc_trim`, from
//! `quill::free_heap::release`) after the frees of a long scroll and of a
//! chat left behind (`docs/decisions/idle-heap-return.md`). Measurements
//! that print what they read, not checks. Run each in a process of its
//! own, with every thread on malloc's main arena, as Quill's UI thread is:
//!
//! ```text
//! MALLOC_ARENA_MAX=1 slot-cargo <wt> test --features demo-capture --bin quill \
//!     -- --ignored --nocapture heap_return::right_to_left
//! ```
use super::*;
use quill::free_heap;
use quill::state::CLOSED_TAIL_KEEP;

const POSTS: i64 = 1500;
const MB: f64 = 1_000_000.0;

/// Posts in a right-to-left script, as long as the paragraph cache's
/// measured ones (`paragraph-cache-widths.md`): 110 words, a number and a
/// link, about 1,170 bytes. Only text with right-to-left letters is laid
/// out through the paragraph cache.
#[gpui_kit::test]
#[ignore = "a measurement: prints resident memory"]
fn right_to_left_posts(cx: &mut TestAppContext) {
    measure(cx, |n| format!("{} https://example.com/{n}", words(n, 110)));
}

/// Posts as long, in Latin letters: gpui lays them out in its own
/// per-frame cache, and the paragraph cache never sees them.
#[gpui_kit::test]
#[ignore = "a measurement: prints resident memory"]
fn left_to_right_posts(cx: &mut TestAppContext) {
    measure(cx, latin_post);
}

fn latin_post(n: i64) -> String {
    let mut seed = n as u64 * 7919 + 1;
    let words: Vec<String> = (0..110)
        .map(|w| {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let len = 2 + (seed >> 33) % 6;
            let word: String = (0..len)
                .filter_map(|i| char::from_u32(u32::from(b'a') + ((seed >> (i * 5)) % 26) as u32))
                .collect();
            if w == 30 {
                format!("{word} 2024,")
            } else {
                word
            }
        })
        .collect();
    format!("{} https://example.com/{n}", words.join(" "))
}

/// Resident memory, and malloc's heap in use and free inside it, in MB.
#[derive(Clone, Copy)]
struct Reading {
    resident: f64,
    in_use: f64,
    free: f64,
}

fn read() -> Reading {
    let statm = std::fs::read_to_string("/proc/self/statm").expect("statm");
    let pages: f64 = statm
        .split_whitespace()
        .nth(1)
        .and_then(|pages| pages.parse().ok())
        .expect("resident pages");
    // SAFETY: both only read process state.
    let (page, info) = unsafe { (libc::sysconf(libc::_SC_PAGESIZE), libc::mallinfo2()) };
    Reading {
        resident: pages * page as f64 / MB,
        in_use: info.uordblks as f64 / MB,
        free: info.fordblks as f64 / MB,
    }
}

/// [`POSTS`] posts from `post` in the group, scrolled through newest to
/// oldest; then another chat opened and the group's history cut to the
/// tail the prune keeps; then every layout let go. What each step leaves
/// resident, and what a release then gives back. Main releases at none of
/// these steps: only after the pictures on screen change.
fn measure(cx: &mut TestAppContext, post: fn(i64) -> String) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    paragraph_cache::reset_in_tests();
    open_group(&app, vcx);
    let updates: Vec<String> = (0..POSTS)
        .map(|n| {
            let (id, date) = (1000 + n, 1790632000 + n * 60);
            let text = serde_json::to_string(&post(n)).expect("a string");
            format!(
                r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":31,"sender_id":{{"@type":"messageSenderUser","user_id":21}},"is_outgoing":false,"date":{date},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{text},"entities":[]}}}}}}}}"#
            )
        })
        .collect();
    let refs: Vec<&str> = updates.iter().map(String::as_str).collect();
    apply(&app, vcx, &refs);
    drop(refs);
    drop(updates);
    let rows = app.read_with(vcx, |app, _| app.history.rows.len());
    free_heap::release();
    let mut table = vec![("posts loaded, released", read(), None)];
    let releases = free_heap::releases();

    // Newest to oldest, a frame each: the paragraph cache lays each post
    // out and lets the oldest layouts go past its 64 MiB.
    scroll_through(&app, vcx, (0..rows).rev());
    redraw(vcx);
    assert_eq!(
        free_heap::releases(),
        releases,
        "the test clock stood still"
    );
    let noted = free_heap::freed();
    table.push(("scrolled to the oldest post", read(), Some(noted)));
    free_heap::release();
    table.push(("  then released", read(), None));

    // Another chat opened; the group's history down to its newest
    // messages, as the prune leaves a closed chat (the demo keeps every
    // history, so they are deleted here).
    app.update(vcx, |app, cx| {
        let session = app.demo_session.as_mut().expect("demo session");
        session.open_chat(ChatId(11));
        cx.notify();
    });
    frames(vcx);
    let gone: Vec<String> = (1000..1000 + POSTS - CLOSED_TAIL_KEEP as i64)
        .map(|id| id.to_string())
        .collect();
    let delete = format!(
        r#"{{"@type":"updateDeleteMessages","chat_id":31,"message_ids":[{}],"is_permanent":true,"from_cache":false}}"#,
        gone.join(",")
    );
    apply(&app, vcx, &[delete.as_str()]);
    drop((gone, delete));
    let noted = free_heap::freed();
    table.push(("chat left, its history pruned", read(), Some(noted)));
    free_heap::release();
    table.push(("  then released", read(), None));

    // Every layout the cache holds let go, as it lets the least recently
    // used go when other text takes their place.
    paragraph_cache::reset_in_tests();
    table.push(("laid-out text let go", read(), None));
    free_heap::release();
    table.push(("  then released", read(), None));

    eprintln!("\n{POSTS} posts, {rows} rows.");
    eprintln!("MB: resident, heap in use, free in malloc, noted for release");
    for (what, r, noted) in table {
        let noted = noted.map_or(String::new(), |bytes| format!("{:7.1}", bytes as f64 / MB));
        eprintln!(
            "{what:32} {:7.1} {:7.1} {:7.1} {noted}",
            r.resident, r.in_use, r.free
        );
    }
}
