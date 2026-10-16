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

End state: bubble widths and tails match the reference metrics, media frames
are drawn (not empty), the time footer sits inside the bubble.

## Gotchas

- Scroll with `w:x,y,0,-200,s;w:x,y,0,-200,m;w:x,y,0,0,e`; a single wheel
  event without phases may not move the list.
- `rg QUILL_DEMO_HISTORY_ANIM src` for the history animation switches.
