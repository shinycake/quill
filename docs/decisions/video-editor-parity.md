# Video editor parity

The send-media video editor (trim, cover, crop, quality, rotate, mirror,
GIF) is rebuilt after tdesktop's `Editor::VideoEditor` and
`Ui::VideoTimeline`. The old dialog was a column of kit sliders whose knobs
were clipped at the track ends.

## What it follows

- **Layer.** The editor fills the window on a dark layer in both themes.
  tdesktop uses its `mediaview*` palette here, so a picture is judged on
  black. The geometry follows `applyGeometry`: a 20 px content skip, the
  timeline (95 px), the quality slider (32 px) and a 48 px pill bar at most
  422 px wide.
- **Timeline.** The strip holds up to 24 frames, with the count picked from
  the clip's aspect. Frames are cut one at a time off the UI thread and
  reloaded when the width changes. The grabs are Left, Right, Head and
  Window: 11 px handles, a 12 px hit slop and a 3 px playhead. The minimum
  selection is the larger of the minimum duration and 15 px of time. The
  duration label reads `00:01.2 – 00:05.0`. The size label sits on the right
  and shows only when it fits; it starts with `~` when it is an estimate.
- **Scrubbing.** A drag seeks at most every 120 ms. The 62 px bubble shows
  the frame under the pointer and stays up for at least 600 ms. When the
  pointer is released, playback resumes. A cover pick flies the bubble
  frame into the cover dot (420 ms, with a flash), and the dot grows from
  5 to 9 px.
- **Crop.** Outside the crop the picture fades by half. The crop has a 2 px
  frame, 20 px corner brackets and a thirds grid while you drag. Edges and
  corners resize; the inside moves. A sticker crop stays square.
- **Rotate and mirror.** These follow tdesktop's order: the turn comes
  first, then the mirror, both in display space. The crop turns and mirrors
  with the picture. The ffmpeg chain (`turn_filters`) and the live preview
  (`turn_plane`) apply the same order.
- **Quality.** Dots of 7 px and 11 px sit on a 2 px line with 4 px gaps and
  an 18 px side skip. The active dot slides in 120 ms. The size estimate
  follows `EstimateTranscodedSize`: 0.07 bits per pixel per frame,
  clamped to 600 kbit/s – 6.8 Mbit/s and capped at the source bitrate,
  plus 128 kbit/s of audio and container overhead.

## Choices

- tdesktop puts the layer over a blurred snapshot of the window
  (`ProcessBackground`). Quill does not read the frame back, so the layer is
  a solid near-black (#121212) instead. The bar is darker than the layer,
  as in tdesktop, where it reads as a hole in the backdrop. Times are
  zero-padded like `FormatDurationText`, so a label reads `01:14.1`.
- The kit `Slider` is gone from this editor. It has no notion of a
  two-ended range with a playhead between the ends, and its knob clipping
  at the track ends was the original complaint. The timeline and the
  quality dots are canvases with their own hit testing, drawn the way the
  photo editor draws its canvas.
- Animations run on the frame clock (`request_animation_tick`) and settle
  instantly when reduce motion is on.
- The bar's rotate and flip icons are Lucide (`RotateCwSquare`,
  `FlipHorizontal2`), added to `QuillIcons`. GIF stays a drawn badge.
- The avatar video panel (`profile_photo_sources/video.rs`) still uses kit
  sliders. It is a separate surface and is left for a follow-up.
