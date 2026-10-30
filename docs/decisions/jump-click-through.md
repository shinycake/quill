# Floating controls over the history take the click

## What was wrong

With the jump-to-latest button over a clip in the history, one press both
jumped to the latest message and opened the media viewer on the clip under
the button.

GPUI hit-tests every hitbox under the pointer. A hitbox with the default
`HitboxBehavior::Normal` leaves the ones behind it hovered, so
`hitbox.is_hovered(window)` is true for the button and for the clip. Each
`on_click` checks that its own hitbox is hovered on mouse down and on mouse
up, so both clicks fired. The media listeners are not at fault: they check
hover, as they should. The controls drawn over them never said they cover
what is under them.

Only a hitbox with `BlockMouse` (`.occlude()`) or `BlockMouseExceptScroll`
(`.block_mouse_except_scroll()`) makes the hitboxes behind it not hovered.
Panels and menus over the history already set it (the emoji / sticker / GIF
panel, the message menu, kit popups and context menus, the selection-mode
row overlay). These floating controls did not:

| Control | Where | What a click also did |
| --- | --- | --- |
| Jump-to-latest button | kit `MessageScroller` (`message_scroller.rs`) | opened the clip, photo or GIF under it |
| "@" / heart / poll corner buttons, with their badges | `jump_buttons.rs` `corner_button` | the same |
| Floating date pill (opens the calendar) | `history_fx.rs` `scroll_date_pill` | the same |
| Emoji suggestion bar over the history, between its tiles | `emoji_bar.rs` | the same (the tiles stop propagation on press; the padding and gaps did not) |

The composer's mention, hashtag, command and inline-result lists are laid
out in the composer's column, not over the history.

## What changed

Each of these controls now calls `.block_mouse_except_scroll()`, the GPUI
default for something that floats over other content: what is under it is
not hovered, so it gets no click, no press style, no hover style and no
tooltip. Scroll events still reach the history, so the wheel over the button
scrolls the chat as before. The fix is on the controls, not on the media: any
row content under them (links, reactions, buttons) is covered the same way.

The inline day separators keep their pill as it was (only the floating pill
blocks); `date_pill_button` now returns the pill element so the floating
pill can add this.

## Test

`video_player::tests::inline::the_jump_button_over_a_clip_takes_the_click`
(`ready-video-inline` demo): shows the jump button (unread messages below),
scrolls the inline clip under it (the window is shortened so the button can
reach the clip, which sits near the top of this short chat), clicks the
button's center, and checks the chat is at the latest message and the media
viewer is closed. It then scrolls the clip back to that spot (the wheel at
the button's center), clicks the clip beside the button, and checks the
viewer opens. On the old code the first click jumped and opened the viewer
("the clip under the button opened").
