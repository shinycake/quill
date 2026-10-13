# Emoji and sticker panel: abort when custom emoji rows render

Branch `claude/fix-sticker-selector-crash-mhqy0z`. Bug fix, no parity item.

## What happened

Opening the emoji and sticker panel aborted the app (macOS crash report:
`panic_already_mutably_borrowed` in `QuillApp::render_panel_row`, under
`gpui::elements::list::StateInner::layout_items`). It happens as soon as any
custom emoji row is drawn: the panel opens on the Emoji tab, whose first rows
hold recently used custom emoji, and installed custom emoji packs follow the
standard emoji, so an account that ever used a custom emoji crashes on open
and any other crashes on scrolling to its packs.

Cause: `render_panel_row` asked the panel's `ListState` whether a custom emoji
row was above or below the viewport (`item_is_above_viewport` /
`item_is_below_viewport`), to keep overdraw rows still. The list calls row
closures from inside its own prepaint, while it holds its state with
`borrow_mut`, so the query's `borrow()` panicked. The panic unwinds into
AppKit's frame callback and the process aborts. Demo sessions and the existing
panel tests had no custom emoji rows, so nothing caught it.

## Fix

The panel reads which rows are on screen once per render, before the list
lays out (`on_screen_rows`, stored as `MediaPanel::on_screen`), and rows read
that snapshot. A row closure never touches the `ListState`.

The snapshot uses the list's measured row heights and its current scroll
position, so it matches what the row closure used to compute. Rows the list
has not measured yet count as visible, as before (first frame animates rather
than flashing a still). Scrolling notifies the view, so the snapshot is
refreshed on the next render.

## Test

`panel_tests::the_emoji_panel_opens_with_custom_emoji_rows` opens the Emoji
tab with recently used custom emoji and renders frames. Against the old row
code it fails with the same `RefCell already mutably borrowed` panic; with the
fix it passes and checks the custom emoji row counts as on screen while the
catalog's last row does not.
