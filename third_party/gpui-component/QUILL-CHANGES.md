# Changes made by the Quill project

This directory is gpui-component 0.7.3 from crates.io (Apache-2.0,
`LICENSE-APACHE`), used through `[patch.crates-io]` in Quill's `Cargo.toml`.
The commit before the one adding this file holds the unmodified registry copy
(without `.cargo-ok`, `.cargo_vcs_info.json` and `Cargo.lock`), so a diff
against it shows every change.

`src/tab/tab_bar.rs`: the sliding indicator sprang (280 ms) whenever the
selected tab's left or width changed, so a badge appearing in a tab restarted
the spring and kept the window redrawing every frame. The animation parameters
now carry a jump counter that is part of the spring key. A change of the
selected tab keeps the key and springs as before; a bounds-only change
(selection unchanged) bumps the counter, and a fresh spring adopts its first
value immediately, so the indicator jumps. The indicator carries a
`tab-indicator` debug selector under `test` / `test-support`. Background:
`docs/decisions/folder-tabs-no-badge-spring.md`.

`src/message_scroller.rs`: `MessageScrollerState::top_anchor` and
`scroll_to_item_offset` expose the list's top row and its pixel offset, so
Quill can remember where a chat was left and return to it
(`docs/decisions/sweep-scroll-restore.md`). Re-apply both when re-vendoring.
`top_anchor` gates on the list's follow state, not on `is_scrolled_up`, so a
position set by `scroll_to_item_offset` reads correctly before the next layout.
`MessageScrollerState::user_scrolls` counts wheel and trackpad scrolls (the list's
scroll handler), letting Quill tell a reader's scroll from a programmatic one.
`note_user_scroll` and `is_scrollbar_dragging` let Quill count keyboard scrolls, sends
and jumps to latest, and see a scrollbar drag, as reader moves (a pending position
restore ends on any of them).
