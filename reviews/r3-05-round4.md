# Re-review of r3-05 third fix round (Opus, 2026-11-12). Verdict: merge

Line numbers are on origin/claude/sweep-r3-05.

Third fix round: 9c9729f5c. Status of the bullet in reviews/r3-05-round3.md:

- Premium gate skipped for a locked pick in the reaction selector: resolved. `react_from_panel` now calls `toggle_reaction` before `close_media_panel` (src/ui/media_panel/mod.rs:341-345), so `react_premium_feature` reads the message's `premium_locked` options before the exit in `set_media_panel_mode` forgets them (src/ui/media_panel/mode.rs:127-134). `open_premium_at` does not touch the panel mode, so the close that follows still runs the forget. The requested test is in: `a_locked_reaction_in_the_selector_opens_the_premium_preview` (src/ui/media_panel/reaction_rows_tests.rs:154) uses a free account and opens the selector from the menu. It picks a locked custom emoji and checks that the Premium preview opened, that no reaction went out, and that no book entry is left. The non-locked path is unchanged apart from the order: the reaction is sent, then the panel closes. No other caller depended on the panel being closed first (src/ui/media_panel/custom_emoji_cell.rs:267, :308 return right after).

The commit adds no unwrap, expect, indexing, RefCell borrows, stage flags, allow-list lines or baseline lines. The only visibility change is `react_from_panel` becoming `pub(super)`, for the test. Every guard script passes on the tip. check-bundle-dlopen.py needs a package directory, so it does not apply to a source tree.

Notes: none.
