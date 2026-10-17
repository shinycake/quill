# Live story Stars effect

Parity item `stories-live-stars-effect`. When someone else sends Stars to
the live story you watch, a badge with their userpic and the count rises
from the Stars button, and a Lottie burst plays around it. tdesktop
references are to `Telegram/SourceFiles`.

## What tdesktop does

- `history/view/controls/history_view_compose_controls.cpp:159-163`:
  `kMaxStarEffects` 4, `kStarEffectDuration` 2 s, rotation up to 12
  degrees, scale from 0.3 to 0.7.
- `:1122-1201` (`StarEffect`): the badge is a pill painted once into an
  image. Its background is the level's `bgLight`
  (`StarsColoringForCount`), its height is the 12 px font
  (`whoReadDateStyle`) plus `groupCallEffectPadding` (3, 1, 3, 1). It
  holds the circle userpic (`groupCallEffectUserpicPadding` 1, 1, 3, 1,
  so the userpic is the height less 2), then `starIconEmojiSmall` (the
  gold `chat/mini_stars` icon) and the grouped count in white.
  `shift = RandomIndex(360) / 360.` is picked once per effect.
- `:1972-1982` (`startStarsEffect`): a new effect drops the oldest when
  four are in flight.
- `:1984-2081` (`setupStarsEffectsCanvas`): the canvas is
  `effectSize * 2` wide and `effectSize * 4` tall, where `effectSize` is
  `reactionInlineImage * 2` (64), so 128 by 256. It is centred
  horizontally on the Stars button and its bottom is the button's bottom.
  It sits over everything and takes no clicks. Per effect, with progress
  `p` from 0 to 1:
  - `left = shift * (width - effectSize)`;
  - `top` goes from `height - effectSize` to 0;
  - opacity rises over the first eighth and falls over the last eighth;
  - `scale = 0.3 + 0.4 * opacity`;
  - `rotation = sin(-pi/2 + pi * (shift + p)) * 12` degrees;
  - the 32 px target box is at `(left + 16, top + 16)`. The badge is
    centred in it and drawn scaled, rotated and faded around its centre.
- The burst is `Ui::ReactionFlyAnimation` with `effectOnly`
  (`ui/effects/reaction_fly_animation.cpp:130-147, 182-201`). Its effect
  Lottie plays once at its own frame rate, untinted ("Must not be colored
  to text"), in a 64 px box centred on the target. It is drawn after the
  badge's transform is restored, so at full opacity and unscaled, and it
  follows the badge up. When the Lottie is not loaded when the effect
  starts, that effect has no burst.
- The Lottie comes from `Reactions::choosePaidReactionAnimation`
  (`data/data_message_reactions.cpp:626-664`): one of the bundled
  `star_reaction_effect1-3` picked by `randomLoadedFrom` (shuffle, the
  first if loaded, else any loaded one, else the first, which starts its
  download).

## What Quill does

- `quill::live_story::star_effect_frame(progress, shift)` is the motion
  above as a pure function, with its constants. `burst_order(seed, n)`
  is the order `randomLoadedFrom` tries the candidates in.
- `FlyingStars` (the viewer's effect in flight) carries `shift` (one of
  360 steps) and `seed`, both rolled in `LiveStoryUi::fly`, so the frame
  is a function of state and the clock. The newest four are kept.
- The layer is drawn by the Stars button (`live_effects.rs`): an absolute
  128 by 256 box centred on the button with its bottom on the button's
  bottom, painted after the story frame so it overlaps it like tdesktop's
  canvas, and with no mouse handlers.
- The badge follows tdesktop's metrics: 12 px font, 16 px line, 1 px top
  and bottom padding, the userpic at the height less 2, the gold star and
  the white count. GPUI cannot scale a div tree, so the scale is applied
  to the metrics (font, line, userpic, paddings).
- The burst picks the first candidate in `burst_order` whose clip was
  decoded before the effect started (`randomLoadedFrom`'s "loaded"). If
  none was, the seed's own candidate is used once it is decoded, and the
  layer always asks for that one, which starts its download and decode.
  The frame shown is the one at the effect's age, played once at the
  clip's rate, in a 64 px box at `(left, top)`, at full opacity.
- The effects clock is `LiveStoryUi::now()`: the real time, or the
  screenshot demo's pinned instant. The frame clock
  (`request_animation_tick`) redraws while an effect is in flight.

## Burst source

tdesktop's `star_reaction_effect1-3.tgs` are part of the GPLv3 tdesktop
tree and are not bundled; that licence decision is pending with the owner.
TDLib serves no animation for a paid reaction (`reactionTypePaid` has no
fields). Quill uses the server's generic reaction bursts instead:
`getCustomEmojiReactionAnimations` (the `inputStickerSetEmojiGenericAnimations`
set tdesktop's `chooseGenericAnimation` draws from). They are asked for
once per session, the first time a live story is joined, and kept in
`Session::stories.live_bursts` (`LiveBursts`: unasked, requested, loaded).
A failed request goes back to unasked, so the next join asks again. These
bursts are not star-coloured, so their frames are tinted with
`chrome::stars_gold()`: each pixel keeps its alpha and takes the gold
scaled by its luminance. The tint is made once per clip and cached, as
`mirrored_frames` does for message effects.

## The swap point

`star_burst(session)` in `live_effects.rs` is the only place that names
the source. It returns `StarBurst { candidates, tint }`. Moving to
bundled `.tgs` files (option A) changes that function and adds the
assets; the motion, the pick, the layer and the badge stay.

## Deviations

- No rotation. GPUI cannot rotate text or a div tree. Rendering the badge
  into an image to rotate it would need text and avatar rasterising
  outside the element tree, which GPUI does not offer cleanly.
- The scale is emulated with scaled metrics, so text is laid out at the
  scaled size instead of a scaled bitmap. At 0.3 it is very small in
  both.
- The burst is the server's generic reaction animation tinted gold, not
  tdesktop's star burst, until the owner decides on the GPL files.
- A clip still decoding when its effect starts is drawn once decoded, at
  the effect's age, if no earlier clip was ready. tdesktop's lazy frame
  generator has no decode wait; Quill's decode is a background job.

## Demo

`ready-live-story-stars` pins the effects clock and shows three effects
at progress 0.1, 0.3 and 0.7 with different shifts, over a burst
fixture (`docs/screenshots/fixtures/demo-star-burst.tgs`, an original
test-only Lottie made by `scripts/make-star-burst-fixture.py`). The real
asset comes from the server.
