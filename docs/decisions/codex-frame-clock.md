# One frame clock for animated content

Quill sat at about 60% CPU in a chat with an animated custom emoji or
sticker on screen. Custom emoji and stickers were multi-frame
`RenderImage`s drawn by GPUI's `img`. GPUI advances an animated image by
requesting a new frame on every display refresh: 120 Hz on ProMotion.
Each frame re-renders and relays out the whole window, and a profile
showed Taffy flexbox layout dominating.

tdesktop repaints only the animated area at the animation's own rate.
GPUI can't repaint a region on its own, so Quill now redraws only as
fast as the content needs:
- Animations decode into one single-frame image per frame
  (`StickerClip::frames`). The renderer picks the frame from the clip's
  elapsed time, and non-looping stickers rest on their last frame.
- `frame_clock` is one app-wide timer. Animated content asks for a tick
  at its rate as it renders: stickers at up to 60 fps (Lottie's rate),
  custom emoji and inline video at 30. The clock redraws at the highest
  rate requested and stops after a tick in which nothing animated
  rendered. The inline-video tick folds into it.

Measured in the same Saved Messages view (inline video, sticker and
custom emoji visible): about 60% → about 29% CPU. Scrolled away from chat
media, with the chat list's animated emoji still on screen: about 25%.
Everything still animates.
