# codex/media-viewer-fit — full-window media viewer that never clips

## Problem
The viewer was a 720 px-wide panel with a fixed 720×480 frame and a header, caption, controls
and prev/next row stacked around it, inside a `max_h_full` box. In smaller windows the stack
overflowed and was cut off. The image also relied on `ObjectFit::Contain`, which the decoded
(pre-rendered) image source did not honor: a 3:2 photo in a 640×400 frame rendered at full
width and spilled past the frame.

## Now (Telegram Desktop's layout)
- The viewer fills the window on a near-black backdrop. Clicking outside the media closes it.
- A top bar holds the title ("Photo 1 of 2") and icon actions: rotate, share, save, show in
  chat, pin album, close. The labels move to tooltips and accessibility names.
- Prev/next are round arrows centered on the sides, hidden at the ends.
- Caption, playback error and the zoom/transport controls sit at the bottom, in white.
- The frame is recomputed from the window every render: width minus 80 px lanes per side,
  height minus the top bar and the bottom area.
- `MediaViewerItem::natural_size` carries the photo's largest size or the video's dimensions.
  The media is sized to its aspect-fit box (`media_viewer::fit_within`, tested), with axes
  swapped for quarter-turn rotations, and centered. Zoom/pan work in that box (`viewer_frame`),
  which resets when the window or item changes. Contain is now only the fallback when
  dimensions are unknown.
