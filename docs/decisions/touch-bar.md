# Touch Bar

Branch `touch-bar`. Brings Telegram Desktop's Touch Bar to Quill on Macs that have one. Reference:
tdesktop `platform/mac/touchbar` (`mac_touchbar_manager.mm`, `mac_touchbar_main.mm`,
`mac_touchbar_audio.mm`, `mac_touchbar_media_view.mm`, `items/mac_pinned_chats_item.mm`,
`items/mac_scrubber_item.mm`, `items/mac_formatter_item.mm`, `mac_touchbar_controls.mm`).

## What tdesktop does, and what Quill does

- **Which bar.** tdesktop's root bar is empty while the app is locked or recording a voice
  message. It shows the player while a song plays and the main bar otherwise when an account is
  signed in. Quill computes the same choice in `touch_bar::model::bar`.
- **Pinned chats.** 30 pt circles, 30 pt apart, starting 15 pt in. The order is Archive (when it
  holds chats), then Saved Messages when it is not pinned, then the pinned chats. An unread badge
  is 15 pt high and grey when the chat is muted. Past three digits the count shows as ".." and its
  last two (`ComputeUnreadBadgeText` with two digits allowed). An online dot is 8 pt with a 1.5 pt
  black ring and grows or shrinks over 150 ms. A tap opens the chat. Dragging a pinned chat by 10
  pt or more reorders it with a 150 ms slide and sends the new order on release.
- **Main bar accessory.** With a selection in the composer, the formatting popover appears:
  Bold, Italic, Underline, Strikethrough, Quote, Monospace, Clear formatting and Link in a
  scrolling segmented control, each segment 1.4 times its label. Otherwise, when the open chat
  can take stickers, the sticker and emoji popover appears: two 92 pt segments, each opening a
  free-scrolling scrubber with 10 pt spacing. Stickers show favorites, recent stickers (without
  the favorites) and the first five installed sets, or "No stickers found" when there are none.
  Emoji show recent emoji, then the categories. A sticker is sent and the popover closes. An
  emoji goes into the composer and the popover stays open.
- **Player.** Play/Pause, Previous, Next, a seek bar and Close, with Track Position available
  through "Customize Touch Bar…" (customization id `quill.touchbar`, allowed items as
  tdesktop's). The seek bar seeks once the touch ends, read from the current event's
  ended touches as `CreateTouchBarSlider` does, and is left alone while touched.
- **Media viewer.** A photo shows Rotate. A video shows Rotate, Fullscreen, Picture-in-Picture,
  Play/Pause, a seek bar and the position. Its seek bar seeks while it moves
  (`playbackControlsSeekProgress`).
- **Popovers.** A formatting tap applies the format and closes the popover. New sticker or emoji
  lists wait for the next segment tap (`_waitingForUpdate`), so nothing moves under a finger;
  sticker pictures that finish downloading show at once.
- **Option.** Settings › Advanced › Experimental settings › Interface › "Disable Touch Bar (macOS
  only).", tdesktop's `touchbar-disabled`. It defaults to on for Macs without a Touch Bar: Apple
  silicon models other than MacBookPro17,1 and Mac14,7, as tdesktop's `HasTouchBar` decides.

## Where Quill differs, and why

- Quill's media viewer is an overlay in the main window, not a separate window, so its controls
  replace the main bar while it is open instead of riding on a second window's bar.
- The option applies at once. tdesktop asks for a restart because it builds the bar at launch.
- Quill has no sticker long-press preview anywhere yet, so the scrubber has none either.
- Stickers are sent through Quill's normal sticker send, so a pending reply is kept, as it is
  for a sticker picked in the window.
- Icons: AppKit's Touch Bar template images where tdesktop uses them (Play, Pause, Skip, Rotate,
  Full Screen, Italic). The picker segments use the kit's sticker and smiley icons, as tdesktop
  uses its own. Archive and Saved Messages use the SF Symbols `archivebox.fill` and
  `bookmark.fill`.

## Structure

- `touch_bar/model.rs`: pure state and geometry (which bar, pins, reorder math, picker lists,
  labels), unit-tested in `model_tests.rs`.
- `touch_bar/mod.rs`: reads the app into the model and runs touches as app commands.
- `touch_bar/mac/`: the AppKit side. Touches become `Command`s in a queue that a task on the
  window drains, so no AppKit callback runs app code or borrows app state. Syncs are throttled
  to 33 ms and skipped while the bar is disabled.
- Linux and Windows have no Touch Bar. Every action on the bar is also in the window, so nothing
  is lost there.

## Verification

None of the Macs Quill is built on has a Touch Bar, and Xcode no longer ships the Touch Bar
simulator. With `demo-capture`, `QUILL_TOUCH_BAR_PREVIEW=out.png quill --screenshot-demo
ready-stickers out/` builds every bar from the demo and renders their item views into a PNG. That
is how the bars here were checked. Touches (tap, drag to reorder, popovers, scrubber picks, the
slider's release) need a Mac with a Touch Bar, so `platform-touch-bar` stays open until one is
tried. `touch_bar/tests.rs` covers a launch crash this branch hit and
fixed: reading the app while it was still being built.

## VoiceOver

`scripts/macos-voiceover-smoke.sh` now walks six flows with the VoiceOver cursor and records
everything VoiceOver speaks. Running it needs VoiceOver on, with AppleScript control allowed. That
is a system setting the owner turns on, so `platform-voiceover` stays open until that run.
