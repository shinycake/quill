# History motion: new-message reveal, selection mode, reply/edit thumbnail

Branch `codex/history-motion`. tdesktop paths are under `Telegram/SourceFiles` (lib_ui under `~/Developer/Reference/lib_ui`), read-only.

## B13 New-message reveal

tdesktop: `history/history_widget.cpp:8607` `startItemRevealAnimations` (main chat) animates `st::itemRevealDuration` = 150 ms (`ui/chat/chat.style:58`) with `anim::easeOutCirc`; `revealItemsCallback` (:8572) lowers the list height by the not-yet-revealed height, so a bottom-pinned history scrolls the new rows up from below the edge. It is dropped if the user is not at the bottom (:8584). The `ListWidget` variant (`history_view_list_widget.cpp:2731`, replies/scheduled views) uses `easeOutQuint`. The backlog note said easeOutQuint; the main history uses **easeOutCirc**, which is what Quill follows.

Quill: `motion.rs` (`reveal_shift`, `MotionState::reveal_now`), `history_fx.rs` (`reveal_viewport`, `RevealProbe`, `history_reveal`). On an append of at most 3 rows while following the tail, window active and the rows filling the viewport, the scroller is laid out `extra` px taller and slid down by `shift`, inside an `overflow_hidden` filler, so new rows rise from the bottom edge. `extra` is the painted height of the new rows (measured by a canvas after the rows paint; 56 px guess for the first frame). A second arrival carries the remaining shift over. Stops when settled, scrolled away, or window inactive (snap). Short histories (row 0 at the top edge) do not animate: nothing is bottom-pinned.

## B16 Selection mode

- `history_view_list_widget.cpp:1713` `inSelectionMode`: check fade `st::universalDuration` = 120 ms (`lib_ui/ui/basic.style:135`), linear, reversing from the current value.
- `history_view_message.cpp:2313`: check circle at the row's right edge, `st::msgSelectionCheck` 20 px, 5 px above the bubble bottom, sliding in about 15 px while fading; outgoing bubbles slide left by `msgSelectionOffset` = 30 px (`AdditionalSpaceForSelectionCheckbox`, history_view_element.cpp:1598).
- `ui/effects/round_checkbox.cpp:294/195`: 160 ms linear check; fill closes in over the first 75 % (`bgDuration`), tick wipes in over the run. Quill: green (`theme.success`) disc scaled by the fill share and a left-to-right tick wipe, ring in the background color over a dark translucent disc.
- `history_view_top_bar_widget.cpp:1750` `toggleSelectedControls`: selection buttons slide down over `st::slideWrapDuration` 150 ms (`basic.style:100`) with easeOutCirc; labels are `lng_selected_forward` / `lng_selected_delete` with the count in the button ("Forward 2", "Delete 2") and `lng_selected_clear` = "Cancel" at the right (lang.strings:5935-5937). There is no separate "N selected" label. Quill: `with_selection_bar` overlays that bar on the conversation header; the old banner above the composer no longer shows while selecting in the open chat. The normal header stays still under the sliding bar (tdesktop also moves it up; percent offsets of an auto-height flow child are not reliable in the layout engine). Digit roll-over animation on the numbers and the joined button corner radii are not done.
- Existing behavior kept: row click toggles, whole-row selection tint (now fading with the check), copy, delete confirm, forward picker. Album rows still have no check (existing gap).

## B24 Reply / edit bar

tdesktop `history/view/controls/history_view_compose_controls.cpp`: `FieldHeader` has **no** show/hide height animation: `updateVisible` -> `ComposeControls::updateHeight` (:5740) resizes immediately (`st::historyReplyHeight` 49 px). Only the pinned/message bar swaps content with `defaultMessageBar.duration`. So the height animation asked for in the brief is **not implemented**, deliberately, to match tdesktop. The thumbnail is: `paintEditOrReplyToMessage` (:788) draws a `st::historyReplyPreview` (32 px, `ui/chat/chat.style:65`) square, `RoundSmall` (4 px), 10 px (`msgReplyBarSkip`) before the text. Quill: `composer_thumb.rs` (`thumb_sources`, `bar_thumbnail`) for photo (downloaded size, else minithumbnail), video, GIF, video note, sticker; none for spoiler/secret media. The send button and its morph are untouched.

## Verification

- `gate.sh`: GATE OK (core 1495, ui 78). New unit tests in `motion.rs` and `composer_thumb.rs`.
- Demo captures (settled, 1200x800): `ready-select-mode`, `ready-reply-media`, `ready-edit-media`; `ready-reveal` at 1200x320 settled and frozen at 40 ms with `QUILL_MOTION_HOLD_MS=40` (new debug env var that freezes all history motion at an elapsed time). Not exercised in a running live window: the real-time animation feel, the selection fade/slide (only settled), and the check fill timing.
