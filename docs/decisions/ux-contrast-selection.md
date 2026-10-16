# Selected-row treatment, AA contrast misses, dialog scroll boundary

Branch `ux-contrast-selection` (sweep-ux findings 3, 5 and 6).

## What tdesktop does

- `dialogs.style` / the palette: the active chat row is a solid `dialogsBgActive` fill with
  `dialogsNameFgActive`, `dialogsTextFgActive`, `dialogsDateFgActive` (white), no leading bar. The
  unread badge on the active row inverts (`dialogsUnreadBgActive` white, `dialogsUnreadFgActive` blue).
- The muted unread badge is `dialogsUnreadBgMuted` (light grey) with white text in the day theme.

## What changed

3. Selected chat rows: the leading `border_l_2` accent bar is gone from `chat_row` (list rows and
   `static_chat_row`), `archive_row` (its bar was transparent, only shifted the content 2 px) and the reply
   quote picker (`reply_options_ui`, selected quote part is now a filled row plus the existing check). The
   selected row keeps the rounded 25% primary fill (the Design Guides' "selected fill, no leading bar") and
   gets its own secondary foreground (`selected_secondary_fg`: theme foreground at 0.78), used for the date,
   preview, sender line, glyphs and status marks. `chat_list_preview_line_layered` takes the text colour as a
   parameter for this.
   `share_box_ui` (forward bar) and `suggested_posts_ui` (offer bar) use the bar as a quote bar, not as a
   selection marker (same as tdesktop's reply bars), so they are unchanged.
5. Outgoing bubble meta (time, ticks, views, edited) is full opacity on outgoing messages (was 0.7); incoming
   keeps 0.7. Destructive colours (`ensure_danger_aa` in `kit_colors.rs`, applied for palette themes and for
   Quill Light / Dark) are darkened (light) or lightened (dark) to 4.5:1 against the window background and the
   hovered danger wash; the destructive button fill keeps 4.5:1 against its label.
6. The shared dialog scroll body (`scrollable_dialog_content`) now ends in a hairline and 8 px padding, so the
   edge where content scrolls under the footer is a visible boundary. The edit-profile bio textarea was
   `auto_grow(1, 1)` inside a fixed 40 px box; it is now `auto_grow(3, 8)` without the fixed height.

## Owner decisions (not changed)

- Muted unread badge: white on `#b0b0b0`-ish grey is 2.17:1 in the light theme. tdesktop uses the same grey
  (`dialogsUnreadBgMuted`), so Quill is not changed. Owner decision: keep tdesktop's colour, or darken it.
- The selected row is a tinted fill (guide-compliant selected fill) rather than tdesktop's solid blue with
  white text. A solid fill needs every nested colour (sender accent, draft red, badges, emoji layers)
  re-themed; the tint with a foreground tuned for AA keeps the existing look and reaches 8.05:1 / 9.60:1.
  Owner decision if exact tdesktop parity is wanted.

## Not fixed

- Forward panel Schedule/Send in dark (2.5-2.7:1): the buttons are in the kit's disabled state, which is
  exempt from AA, and the fix is in `forward.rs` (select-on-Enter / enabling) owned by another builder.
- "Terminate all other sessions" on its wash: still 2.4:1 in the demo capture. The button is a kit
  `danger()` button; its text kept the kit's red after the token change, so it does not read the shared token
  (needs a look at the kit button variant). Per-row "Terminate" is fixed.

## Contrast before / after (demo-capture PNGs, 1280x860, `measure.py`, peak colour vs dominant bg)

| where | before | after |
|---|---|---|
| date on selected row, light | 3.18 | 8.05 |
| preview on selected row, light | 3.18 | 8.05 |
| date / preview on selected row, dark | 6.22 | 9.60 |
| outgoing bubble time + ticks, light | 3.34 | 5.19 |
| outgoing bubble time + ticks, dark | 3.07 | 4.63 |
| "Terminate" on white, light | 3.76 | 6.47 |
| "Terminate all other sessions" on wash, light | 2.90 | 2.42 (unchanged, see above) |
| forward Schedule / Send, dark (disabled) | 2.69 / 2.52 | unchanged (skipped) |

Captures: `/Volumes/Idan1TB/quill-work/scratch/quality-sweep/fix-ux-contrast/` (light and dark: chats,
sessions, forward, showcase, profile-edit, update-failure, storage-usage).

## Tests

`selected_row_tests::selected_secondary_text_meets_aa`, `kit_colors::tests::danger_colours_reach_aa`,
`footer_alpha_tests::outgoing_meta_is_opaque`, `bio_rows_tests::bio_shows_at_least_three_rows`. Gate OK.
Unverified: other storage-usage / appearance dialogs were captured but only the edit-profile and sessions
boundary were inspected by eye; the outgoing-meta rule treats a message with a receipt or pending state as
outgoing.
