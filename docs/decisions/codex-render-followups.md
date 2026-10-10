# Render follow-ups: reply strips, timestamps, slot machines, live location

These items were left open by the render service media PR.

## What Telegram Desktop does

- `history_view_reply.cpp`: a reply strip shows the replied message's picture,
  a quote mark for manual quotes, "Name > Chat" for replies from another chat,
  and for a story reply the poster's name over "Story" with the story's
  picture. The strip is tinted in the sender's name color and, when the sender
  has a background emoji, repeats that emoji's first frame in that color at
  fixed offsets and opacities (`FillBackgroundEmoji`).
- `history_view_slot_machine.cpp`: a slot machine is a stack of stickers
  (background, three reels, lever). Each reel rests on the symbol its roll
  decodes to: the roll minus one holds one symbol per reel in two bits.
- `history_view_media.cpp` and `local_url_handlers.cpp`: a timestamp seeks the
  message's own voice message, song or video, the one it replies to, or a
  YouTube preview, which opens with `t=` set.
- `history_view_location.cpp`: the live status line is rescheduled for the
  moment its text changes.
- `lng_ttl_*_expired`: expired media become service rows ("Expired photo",
  "Expired video", "Round message expired", "Voice message expired").

## What changed

- Reply strips: replies to stories parse (`messageReplyToStory`) and show the
  poster over "Story" with the story's picture. `getStory` is requested once
  per story replied to. Clicking opens the viewer when the story is in the
  tray. Name colors of chats now come from `chat.accent_color_id` instead of a
  guess from the chat id.
- Emoji pattern: users and chats now keep `background_custom_emoji_id`
  (`updateNewChat`, `updateChatAccentColors`). The replied sender's emoji is
  resolved with the other custom emoji of the open chat, its still is tinted
  to the strip color (`reply_pattern.rs`) and drawn at Desktop's offsets,
  with the two extra copies on quote strips.
- Timestamps: `media_timestamp.rs` decides the target (voice, audio, video, or
  a YouTube preview that opens with the start time) and clamps the seek to the
  clip length.
- Slot machines: `diceStickersSlotMachine` keeps its five stickers and the row
  stacks them. The line under the picture names the three symbols and says
  "jackpot!" for three sevens. A machine with a missing layer falls back to the
  emoji.
- Live location: the countdown now reads "9 min" (seconds in the last minute)
  and the window redraws when that text changes: once a minute, once a second
  in the last minute, only while the open chat shows a running share and the
  window is active. The pace comes from `LiveLocationState::refresh_in_at`.
- Expired media: audited. Every `messageExpired*` becomes a service row with
  Desktop's wording. Only `messageUnsupported` and the two TON constructors
  reach the generic card, which Desktop also shows for unsupported content.
  The unreachable "This message has expired." branch in `history.rs` is gone.
- New demo kind `ready-render-followups` (English fixtures), views `media` and
  `replies` through `QUILL_DEMO_FOLLOWUPS_VIEW`.

## Not done

- A deleted story still reads "Story": `getStory` errors are not turned into
  Desktop's "Deleted story".
- The gift variant of the pattern is not drawn.
- The live countdown refreshes for any running share in the loaded history of
  the open chat, not only the ones scrolled into view.

## How it was verified

- Unit tests: slot symbol decoding and result lines, five-layer parsing and the
  fallback, label and refresh pacing of live locations, the session helper that
  drives the redraw, start-time URLs and seek clamping, pattern layout and
  tinting.
- Both demo views were captured and looked at: stacked machines with the
  jackpot line, "expires in 9 min", the expired photo row, linked timestamps,
  and strips with a picture, quote mark, other-chat name, story and pattern.
- Not run live: the timer redraw in a real window, clicking a timestamp, and
  `getStory` against TDLib.
