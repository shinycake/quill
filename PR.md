Long forum topics stay bounded in memory and page back to the latest message as you scroll down

## What was wrong

growth-4 (B16): reading back through a long forum topic kept every page in memory while the topic stayed open. Topic pages were inserted with no trim, and a live message only dropped rows above the reader. A topic had no way to page its newest end back in, so it could never drop it. The finding estimates about 3.5 KB per row, so 30,000 rows is about 100 MB until the topic is left.

Comment and reply threads were already capped on main (ThreadView::trim_after_page and trim_live), so this PR only moves their window bookkeeping into the shared type and keeps their behavior.

## What changed

- TopicHistory now has a newer side, as the thread view already had: has_newer, newer_failed, window_epoch, stale_pages, plus latest_seen. The thread view now keeps these in its TopicHistory instead of in separate fields, and thread trims run through the same TopicHistory code (src/state/topic_window.rs).
- An older topic page past HISTORY_WINDOW_CAP (1500) drops the newest end down to 1200 rows. The window then stops short of the latest message, and a new GetTopicHistoryNewer request (searchChatMessages with a negative offset, src/connect/groups/forum_window.rs) pages it back in as the reader nears the bottom.
- A newer page past the cap drops the oldest end. Older pages continue from the oldest kept row.
- Live messages trim the rows far from the reader in both directions (the same live_trim_span as the main history). While the window stops short of the latest, a live message past its end waits outside it. An own send replaces the window with the latest page, as tdesktop's showAtEnd does.
- A page asked for before its end of the window moved is dropped, so no row lands across a gap. If such a page fails, the topic does not show Retry.
- In a topic, the jump-to-latest button replaces a window that stops short of the latest message with the latest page. The topic's window epoch reaches the scroller so it re-anchors.
- A closed topic that stops short of the latest drops its window, so it opens at its latest page again.

This matches tdesktop, where topics and replies use HistoryView::ListWidget, a sliding window around the viewport.

## What ran

- cargo fmt, and cargo clippy -D warnings for `--features demo-capture --all-targets` and `--no-default-features --all-targets`: clean.
- cargo test --features demo-capture --lib: 6864 passed. The only failure is the chmod 0555 test, which fails when run as root.
- cargo test --features demo-capture --bins with the conversation, history, topic and thread filters: 149 passed.
- New state tests in src/state/tests/topic_window_cap.rs cover: an older page past the cap, scrolling back down to the latest, a live message held outside a short window, an own send in a short window, the cap at the latest and with the reader near the top, a stale newer page, jump to latest, and closing a short topic. New driver tests in src/connect/tests/forum_window.rs check the newer request shape, deduping, and jump to latest.
- Measured as rows held: reading 4,501 rows back through one topic now keeps at most 1,500 rows (the test asserts this). With the page trim switched off, that test and five others fail, which shows they fail without the fix.
- The existing test a_forum_topic_on_screen_drops_only_rows_above_its_reader asserted the uncapped behavior. It is renamed and now asserts the cap.

## What did not run

- Bytes per row were not measured. The 3.5 KB per row figure comes from the finding, and only row counts were measured here.
- The app was not driven by hand to scroll a real long topic. No live TDLib account is available in this environment.
- cargo check for x86_64-pc-windows-gnu: that target is not installed here. The change has no platform-specific code.
- macOS: no platform-specific code changed, so a Mac check has nothing extra to build.

## What stays open

- Saved Messages sublists also store a TopicHistory, and their pages (apply_saved_topic_history) still insert with no cap. growth-4 did not name them. The same trim and newer-page path could be added with its own request.
