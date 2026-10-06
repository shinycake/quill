# Animated stickers in the picker, animated custom emoji, Escape closes the viewer

## Problems (owner feedback, verified live)
- Stickers animated only after being sent, never in the picker. GPUI's
  `img` advances an animated image's frames only for an element with an
  id (the frame state lives there). The picker's sticker image had none.
- Custom emoji never animated anywhere: they always drew their still
  thumbnail.
- Escape didn't close the media viewer.

## Telegram Desktop
Custom emoji animate in messages, in the picker and in reactions;
stickers animate in the picker. tdesktop decodes small custom-emoji
frames into a shared cache.

## Decision
- The picker's sticker and custom-emoji images carry element ids.
- Two playback caches (`PlaybackSize`): stickers at 128 px / 16 clips,
  custom emoji at 64 px / 48 clips. The decoders take an edge size
  (`decode_tgs_sized`, `decode_webm_sized`).
- Message rows look up their custom emoji's animations
  (`message_custom_emoji_frames`) and the text renderer prefers them over
  the stills, so all custom emoji in a message animate.
- The picker animates the hovered sticker or custom emoji, like stickers
  already did. Animating every visible picker emoji (~80) would need
  ~150 MB of frames or constant re-decoding; that waits for a
  frame-on-demand renderer.
- Escape closes the media viewer (after any open menu).

## Verified live
Hovered Retro Font emoji animate in the picker; sent to Saved Messages,
they render big, without a bubble, and animate.
