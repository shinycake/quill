# Round video messages autoplay (tdesktop)

tdesktop loops downloaded round video messages muted in the chat. A click
plays the message once from the start with sound (it becomes "viewed"),
then it returns to the muted loop.

Quill drew a still circle with a play disc, and played through the old
ffmpeg frame path. Now round videos join the inline players
(`inline_video`):
- they autoplay muted and looped under the same conditions as videos
  ("Autoplay videos", no data saver, not secret, clip downloaded);
- a click calls `InlineVideos::toggle_sound`: restart with sound (muting
  any other), mark the message opened if it was unseen, and show the time
  left. At the end the clip falls back to its muted loop;
- GPUI's `surface` can't clip to a circle, so a generated mask
  (`circle_mask`: the history's color with an anti-aliased transparent
  circle, cached per size and color) covers the square video's corners.
  The unseen accent ring draws above the mask.

Not verified live: no round video message was available in a chat I may
open without side effects. The demo fixture renders through the
synthetic history, not the session history. The mask math has a unit
test, and the player path is the one verified for videos in #391.
