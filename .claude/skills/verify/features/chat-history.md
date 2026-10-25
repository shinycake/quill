# Chat history and bubbles

The message list on the right: bubbles, media tiles, inline video and round
video notes, voice messages, service messages.

## Sub-features

- Text and media bubbles, albums.
- Inline video tile autoplay and round video notes.
- Voice playback bar.

## How to get to it (user POV)

Open any chat from the chat list and scroll the history.

## Driving it with --screenshot-demo

| Kind | State on screen |
| --- | --- |
| `ready-media` | a chat with photo and media bubbles |
| `ready-video-inline` | autoplaying video tile and round video |
| `ready-video` / `ready-video-note` | video bubble / round note with Play/Pause |
| `ready-voice` | voice record bar and playback |
| `ready-voice-playing` | active voice note: waveform scrubber and player bar (`QUILL_DEMO_SEEK_HOVER=0.7` shows the hover tint and time; also works on `ready-audio`) |

End state: bubble widths and tails match the reference metrics, media frames
are drawn (not empty), the time footer sits inside the bubble.

## Gotchas

- Scroll with `w:x,y,0,-200,s;w:x,y,0,-200,m;w:x,y,0,0,e`; a single wheel
  event without phases may not move the list.
- `rg QUILL_DEMO_HISTORY_ANIM src` for the history animation switches.
- Message effects: `ready-message-effects`. A click on the badge of
  "Happy birthday!" (`450,153;p:400`) plays its effect again;
  `QUILL_DEMO_EFFECTS=arrive` with `p:350` shows an unread message arriving
  and playing its effect once. Set `QUILL_RLOTTIE_PATH` to the vendored
  `librlottie.dylib` in a dev build, or the effect shows a renderer notice.
  The demo's effect animation is a placeholder circle.
- Messages arriving, reacted to and deleted in the open chat:
  `QUILL_DEMO_UPDATE_STREAM=4 QUILL_DEMO_STREAM_OPEN=1` with a pause step
  (`QUILL_DEMO_CLICK=p:7000`); `QUILL_TRACE_TICKS=1` prints row builds,
  splices and patches per second. `ready-group-commands` shows sender runs.
