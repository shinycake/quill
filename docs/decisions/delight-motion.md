# Motion and feedback for surfaces that come and go

Almost every transient surface in Quill used to appear and vanish in one
frame: the status toast, the reply, edit and forward bars above the
composer, the pinned bar, the emoji panel, the corner "@" and heart
buttons. This round gives each one the short entrance and faster exit
Telegram Desktop gives its equivalents, on the existing motion system
(`src/ui/motion.rs` `Glide`, the frame clock), without touching the
session state: the state stays the truth and a small helper follows it.

## What Telegram Desktop does

| Surface | tdesktop | Where |
| --- | --- | --- |
| Toast | fades in over `durationFadeIn` 200 ms, out over `durationFadeOut` 1000 ms, no movement | `lib_ui/ui/toast/toast.cpp:40`, `:109`; `lib_ui/ui/widgets/widgets.style:1556` |
| Pinned bar | `Ui::SlideWrap` toggles with `slideWrapDuration` 150 ms; `MessageBar` swaps its text over `defaultMessageBar.duration` 160 ms, the old lines sliding out by `msgReplyBarSkip` towards the message that was shown and the new ones in from the other side | `ui/chat/pinned_bar.cpp:94`, `ui/chat/message_bar.cpp:144`, `chat_helpers.style:1041` |
| Reply / edit / forward header | shown at once (`FieldHeader::updateVisible`), no slide | `history/view/controls/history_view_compose_controls.cpp:598` |
| Emoji / sticker / GIF panel | `Ui::PanelAnimation` from the corner nearest the button: width from 50 % over the first 60 % of `emojiPanShowDuration` 200 ms, height from 30 % over 90 %, opacity from 20 % over 30 %; hides with an opacity fade | `chat_helpers/tabbed_panel.cpp:373`, `:360`; `lib_ui/ui/widgets/widgets.style:930`; `chat_helpers.style:518` |
| Corner buttons | each fades over `historyToDownDuration` 150 ms; the mention / reaction stack sits above the scroll-down arrow and rises with its shown value | `history/view/history_view_corner_buttons.cpp:442`, `:530`; `chat_helpers.style:1227` |
| Scroll-to-bottom button | bottom-right corner (`historyToDownPosition`), over the content | `history_view_corner_buttons.cpp:558` |
| Chat-list unread badge | no animation | `dialogs/ui/dialogs_layout.cpp` |

## What Quill does now

`src/ui/surface_reveal.rs` holds `Presence<T>`: it follows an `Option<T>`
once per frame, glides `shown` 0 → 1 on an entrance duration and back on
an exit duration, keeps returning the last `T` while the surface leaves,
reports a content swap (`previous`, `swap`) when the value changes while
shown, and `moving` so the view asks the frame clock for frames only
then. A `scope` (the chat id) snaps the surface when the chat changes, so
a bar never slides out of one chat into the next. One `Presence` per
surface lives in `FrameUi::surfaces`; `slide_open` clips a bar to
`shown` of its measured height (`MeasureSize`, the height of the last
frame) with its content attached to the composer or the header edge, and
folds the column gap into the slide so siblings do not jump.

- Toast (`app_render/status_toast.rs`): 200 ms fade in with an 8 px rise,
  120 ms fade out. The exit is Quill's `universalDuration` exit, not
  tdesktop's 1 s fade: a dismissed toast should be gone when the next one
  arrives. The rise is Quill's; tdesktop's toast does not move.
- Reply, edit and forward bars (`composer_ui/bar_slide.rs`): 150 ms slide
  open with the content fading in, 120 ms shut. tdesktop snaps these; the
  slide is the one its pinned and sponsored bars use (`Ui::SlideWrap`),
  applied so the composer grows instead of jumping.
- Pinned bar (`message_actions/unpin_from_banner.rs`): the same slide, and
  the `MessageBar` text swap (160 ms, 10 px) when a click steps to the
  next pinned message.
- Emoji / sticker / GIF panel (`media_panel/reveal.rs`): tdesktop's
  `PanelAnimation` shares from the bottom-left corner over 200 ms, a 120 ms
  fade out. The clipped panel is the real panel laid out at full size,
  not a scaled copy.
- Corner buttons (`jump_buttons.rs`): 150 ms fade with an 8 px rise,
  120 ms out, and the stack lifts over the jump-to-bottom button while
  that shows, by the kit's own rule for showing it. The jump button moves
  to the bottom-right corner where tdesktop keeps it (`wrap_jump_button`
  in `conversation/history_message_list.rs`), under the stack, instead of
  over the middle of the last message. The kit already fades that button
  over 200 ms.
- Press feedback (`pressable.rs`): `pressable()` adds the shared keyboard
  focus ring (`focus_ring`, also used by menu rows), and
  `focus_ring_visible()` adds the ring alone to a surface with hover of
  its own. Added to: the composer context bar body, the player bar title,
  the group call side buttons, the mute button, "load more", the error and
  rejoin rows, the media viewer sender, the appearance chips and
  swatches, the settings profile row.

Reduced motion (`cx.reduce_motion()`, the OS setting) and interface
animations off (power saving, `Glide::go`) snap. Under `cfg(test)` a
presence snaps unless the test set a clock (`set_test_clock`), as
`panel_reveal` does, so the UI tests stay deterministic.

Not done: the chat-list unread badge. It already scales and fades in
(`quill::row_fx`, 150 ms); the exit was left as a snap on purpose
(`RowFxMap::observe`), and animating it would mean every row keeping a
count after the state dropped it, for hundreds of rows.

## Verified

Screenshot demos with `QUILL_MOTION_HOLD_MS` to freeze every motion at a
mid point, and settled frames, light and dark:

- `ready-status-toast-info`: the toast at 60 ms (faint, 2 px low) and
  settled.
- `ready-composer-bars`: the reply bar at 60 ms of its slide (half open,
  content faint) and settled; after a click on its close button at 60 ms
  of the exit.
- `ready-pin`: the pinned bar at 60 ms and settled.
- `ready-stickers` / `ready-chats-composer` with a click on the emoji
  button: the panel at 60 ms (grown from the corner, faint) and settled.
- `ready-unread-landing`: the jump button in the corner, the corner
  stack above it.

Not verified: a live TDLib session (the demos inject state), Linux and
Windows (no platform code in this change), screen-reader focus order
after the new tab stops, and the real feel of the frames at 120 Hz (the
hold shows single frames).
