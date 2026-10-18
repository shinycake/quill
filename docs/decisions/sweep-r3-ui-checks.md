# Round 3 UI checks: theme previews, keyboard reach, chrome notify flag

## What tdesktop does

tdesktop's theme and accent previews are drawn from the theme being shown,
and every settings row and box button is a focusable control, so Tab reaches
all of them. Its deferred repaints cannot get stuck for a window that is
still alive.

## Findings and changes

1. Literal white and black in `appearance/mod.rs` and `chat_palette.rs`.
   Captured Appearance in light, dark and high contrast
   (`ready-spellcheck-toggle`). The theme thumbnails are fixed pictures of
   each theme by design, the selected accent ring follows the active theme
   (dark ring on light, white ring on dark), and the dialog text and
   surfaces follow the theme and the high-contrast palette. The remaining
   literals are the white text on the accent fill (accent maths) and the
   transparent unselected swatch border. Nothing was wrong, so the palette
   code is unchanged. The demo now honours `QUILL_DEMO_THEME=light` so the
   light capture can be taken.
2. Keyboard reach. Tab passes through the Settings rows, the Privacy box
   rows and the folder editor: every row has a tab stop (`tab_index(0)` or a
   kit Button, Switch or Checkbox), and the kit's dialog focus trap keeps
   Tab inside the dialog. One gap found: the folder tab context menu items
   had click handlers but no tab stop. They now have `tab_index(0)`. The
   archive, playlist and story context menus have the same shape and are
   left for their own change.
3. `finish_chrome_notify` (row_chrome.rs). The deferred notify cleared
   `chrome_notify_pending` only inside `in_window_scope`. A live window
   whose session view is gone makes that return `None`, which left the flag
   set and silenced chrome notifies for the window. The `None` path now
   clears the flag on the window's slot (`clear_slot_chrome_notify`).
4. `premium_ui/promo.rs`: each doc comment sits on its own function.

## How verified

Capture passes with `--screenshot-demo` (light, dark, high contrast;
Settings and Privacy Tab sequences) and a GPUI test,
`a_pending_chrome_notify_clears_when_the_window_lost_its_view`.
Not shown by captures: the folder tab menu does not open in the demo, so
the new tab stop there is covered by the idiom only.
