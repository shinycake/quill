# Changes made by the Quill project

This directory is gpui-component 0.8.1 from crates.io (Apache-2.0,
`LICENSE-APACHE`), used through `[patch.crates-io]` in Quill's `Cargo.toml`.
Commit `6be84cd7` added the unmodified registry copy (without `.cargo-ok`,
`.cargo_vcs_info.json`, `Cargo.lock` and `Cargo.toml.orig`), so
`git diff 6be84cd7 -- third_party/gpui-component` shows every change.
(First vendored at 0.7.3 in `255bf944`; carried to 0.8.0 in
`docs/decisions/gpui-kit-0-8-0.md` and to 0.8.1 in `docs/decisions/gpui-kit-0-8-1.md`.)

`src/tab/tab_bar.rs`: the sliding indicator sprang (280 ms) whenever the
selected tab's left or width changed, so a badge appearing in a tab restarted
the spring and kept the window redrawing every frame. The animation parameters
now carry a jump counter that is part of the spring key. A change of the
selected tab keeps the key and springs as before; a bounds-only change
(selection unchanged) bumps the counter, and a fresh spring adopts its first
value immediately, so the indicator jumps. The indicator carries a
`tab-indicator` debug selector under `test` / `test-support`. Background:
`docs/decisions/folder-tabs-no-badge-spring.md`.

0.8.0's "sliding indicator appears after the first frame" (#3574) is a
different fix: it records where render placed the indicator
(`TabIndicatorBounds::placed`) and asks for one more frame when the selected
tab was measured elsewhere. It still retargets the same spring when only the
selected tab's bounds change, so a badge would still spring the indicator.
The patch is kept and works with it: the extra frame is the one that bumps
the jump counter. `TabVariant::Folder` (#3415) draws no sliding indicator.

`src/message_scroller.rs`: `MessageScrollerState::top_anchor` and
`scroll_to_item_offset` expose the list's top row and its pixel offset, so
Quill can remember where a chat was left and return to it
(`docs/decisions/sweep-scroll-restore.md`). Re-apply both when re-vendoring.
`top_anchor` gates on the list's follow state, not on `is_scrolled_up`, so a
position set by `scroll_to_item_offset` reads correctly before the next layout.
0.8.0 adds `MessageScrollerState::logical_scroll_top` (#3485), which reads
the same position but not the follow state, and has no setter, so both
Quill methods stay.

`src/theme/{mod,mono_font}.rs`: `installed_font_names` is public, and
`prefetch_installed_font_names` fills the same once-per-process list on a
background task. Quill starts the prefetch when the platform exists, so theme
init finds the list ready and the font picker reads the same list instead of
listing the fonts a second time on the main thread (commit `cfdb6d7e`,
`docs/decisions/launch-time-fonts.md`).

`src/highlighter/{mod,syntect_backend}.rs`: `highlighter::capture_names` is
public with the `syntect` feature (without `tree-sitter`). It returns the
highlight names of a text for a language, so Quill colors message code blocks
from its own theme tokens and runs the parse on a background thread
(`docs/decisions/code-syntax-highlight.md`). Re-apply when re-vendoring.

`src/message_scroller.rs`: `MessageScrollerState::distance_to_edges` passes
`ListState::estimated_distance_to_edges` through, so Quill's history asks for
the next page while three screens are left (`docs/decisions/history-preload.md`).
Re-apply when re-vendoring.

`src/message_scroller.rs`: `MessageScroller::with_jump_button_shown_after`
keeps the jump-to-latest button hidden until the viewport is more than that
distance above the last row (tdesktop `historyToDownShownAfter`, 480px), and
`jump_button_always` shows it wherever the list is scrolled, for a history
whose loaded window stops short of the latest message. Background:
`docs/decisions/unread-open-landing.md`.

`src/dialog/dialog.rs`: `Dialog::wrap_surface` wraps the dialog's surface
(everything above the backdrop) in an element the caller passes. The base
dialog lays the surface out in place but prepaints and paints it in a
deferred pass after the rest of the window, so an element that sets
something up for its subtree while it draws has to sit inside the surface.
Quill puts its bounded image cache there, so pictures in dialogs leave
memory once off screen (`docs/decisions/dialog-images-bounded-cache.md`).
Re-apply when re-vendoring.

`src/message_scroller.rs`: `MessageScroller::wrap_jump_button` wraps the
built-in jump-to-latest button, while it is drawn, in an element the caller
passes. Quill marks the button as covering the rows' animations
(`anim_layer::occluder`) with its own bounds, only while it shows
(`docs/decisions/codex-history-anim-layer.md`). Re-apply when re-vendoring.
