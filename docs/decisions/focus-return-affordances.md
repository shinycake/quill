# Focus return for hand-drawn layers, and settings affordances

UX sweep findings 11 and 8.

## What tdesktop does

Closing the emoji/sticker/GIF panel, the share box and the composer dialogs
leaves the message field focused (`HistoryWidget` refocuses the field after a
panel or box closes). Slow mode in the group permissions box is a discrete
slider with a visible selected stop; sub-pages in settings are rows with an
arrow. Loading states in boxes show a spinner.

## What changed

- `src/ui/esc_stack.rs`: one shared `track_layer_focus`, run every frame from
  `app_render`. While none of the tracked layers (`FOCUS_RETURN_LAYERS`, plus
  the emoji panel) is open it remembers the focused element; when the last one
  closes, focus returns there unless the close path already moved it on
  purpose. It covers Escape, close buttons and any other close path with no
  per-layer code. Context menus keep their own mechanism.
- Slow mode options are kit buttons with the filled `selected` state instead of
  a check glyph that shifted the text.
- `src/ui/nav_row.rs`: kit-button navigation row (label + chevron); the
  Experimental settings entry uses it.
- "Loading invite links / join requests" show the kit `Spinner`.
- Session cards: Terminate and Details share one right-aligned row, so Details
  sits in one column on every card.

## Verified

- `focus_return_tests` (GPUI): emoji panel close, its close button, sticker
  panel Escape all return focus to the composer; all three fail with the
  tracker disabled.
- `advanced_ui_tests`: clicking the nav row opens the Experimental page.
- Before/after demo captures in
  `/Volumes/Idan1TB/quill-work/scratch/quality-sweep/fix-focus-affordances/`.

## Not done

- Slow mode is a button row, not tdesktop's slider (the kit slider is
  continuous; the eight TDLib values are discrete).
- The bare "Show" on Revoked links and the sessions list divider overshoot
  were not touched.
- Profile modal, earn layer and forward picker are in the tracked list but
  have no dedicated UI test (they share the same code path).
