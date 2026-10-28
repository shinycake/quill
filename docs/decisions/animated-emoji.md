# Lone emoji as animated stickers

When a message is a single standard emoji, Telegram sends it as
`messageAnimatedEmoji` and the official apps draw it as a 112 pt animated
sticker with no bubble. Quill used to parse that content as plain text and
show the glyph at the large-emoji size (36 pt). The animation only played as
the full-screen tap effect, so a sent 👋 looked like a big glyph that came to
life only when clicked. This change draws the sticker itself.

## What a message gets

`quill::animated_emoji::emoji_media` decides, from the parsed text alone,
how a message of only emoji is drawn. It checks in this order:

1. One to three custom emoji (lines count toward the three, as in
   tdesktop's `CustomEmoji` dimension) are drawn as stickers at 112, 78 or
   58 pt. These are tdesktop's `SizesInfo()` scales of 1, 0.7 and 0.52
   over `Sticker::EmojiSize()` (`history_view_custom_emoji.cpp`).
2. A lone emoji that came with an animated sticker is drawn as a 112 pt
   sticker (`st::maxAnimatedEmojiSize`, `chat.style`).
3. Anything else that qualifies for large emoji keeps the glyph path
   (`LARGE_EMOJI_SIZE`, tdesktop's `largeEmojiSize`).

All of these sizes live in one table (`SIDES`). The "Large emoji" setting
turns off all three cases. tdesktop does the same: in
`Element::refreshMedia` (`history_view_element.cpp`), `largeEmoji()` gates
the custom emoji, the emoji sticker and `LargeEmoji` alike.

## Parsing

`messageAnimatedEmoji` now becomes `MessageContent::Text` with the emoji
text kept and a new `TextContent.animated: Option<AnimatedEmoji>`: the
sticker, its size, the skin tone (`FitzType::from_tdlib`), the sound file
and `is_regular_emoji`. Everything that reads the text (previews, search,
copy, quotes, translation) keeps working unchanged. When the sticker is a
custom emoji's, the parser puts the `CustomEmoji` entity back over the
text, so the message is drawn and copied like the custom emoji it is. A
sticker with file id 0 (TDLib sends one for a custom emoji it has not
loaded yet) leaves plain text until `updateMessageContent` brings the real
one.

## Playback

The lone emoji plays through the streaming player (`sticker-player.md`) at
112 pt, recoloured to its skin tone by the player's fitz pass.

- It plays once, then rests on frame 0. This matches tdesktop's
  `Sticker::paintAnimationFrame`: `playOnce` is true for an emoji sticker,
  and `_oncePlayed` stops it on the first frame afterwards.
- Each message plays at most once per chat visit. tdesktop keeps
  `HistoryInner::_animatedStickersPlayed` and clears it when the history
  widget is destroyed (`stickerClearLoopPlayed`). Quill keeps
  `EmojiPlays` in the UI's playback state and clears it in
  `stop_sticker_playback`, which runs when the chat closes.
- Sent messages play as soon as they are drawn, and received ones play
  when they scroll into view. The player only starts a head once its first
  frame is on screen, so a message above the fold waits.
- A message sent from Quill keeps its pending id as the play key after
  `updateMessageSendSucceeded` (`EmojiInteractions::renamed`). Without
  that, the final id would start a second play just as the first ended.
- Tapping plays the sticker again on a new head (one generation per
  replay) and asks TDLib for the 336 pt effect through
  `clickAnimatedEmojiMessage`, anchored on the sticker as before. When the
  other side's tap effect (`updateAnimatedEmojiMessageClicked`) starts on a
  message, that message replays too, which is tdesktop's behaviour.
- A tap or effect while the sticker is still playing does not restart it.
  tdesktop's `_oncePlayed` logic happens to let a second tap restart
  mid-play; Quill lets the play finish, which looks less jumpy.
- A head that played and was then forgotten (the player drops idle heads
  after 10 minutes) rests on a separate head fixed at frame 0, so it never
  starts playing again by itself.

Custom emoji messages loop like other custom emoji (`sticker_mode`, which
also respects "Loop animated stickers").

## Power saving and reduced motion

- With "Animated emoji" in chats off (`power_saving::Flag::EmojiChat`), the
  lone emoji shows its first frame and does not play.
- With "Animated stickers" in chats off (`Flag::StickersChat`), big custom
  emoji show their stills, as tdesktop does for custom emoji stickers.
- While the sticker downloads, the block shows the sticker's thumbnail,
  else the emoji glyph at the large-emoji size.

## The effect on read: not implemented

tdesktop also plays the tap effect once, by itself, when an unread incoming
lone emoji in a private chat is marked read: `HistoryInner` calls
`EmojiInteractions::startAutoplay` (`chat_helpers/emoji_interactions.cpp`). It picks the animation from the
local "EmojiAnimations" pack (`emojiStickersPack().animationsForEmoji`)
and only flags the message locally (`markEmojiInteractionWatched`); the
other side is told nothing.

TDLib does not expose that pack. Its only way to get an interaction
animation is `clickAnimatedEmojiMessage`, which also queues a
`sendMessageEmojiInteraction` to the chat partner. Using it for autoplay
would tell the other person that this user tapped their emoji every time
a message is read, so Quill leaves autoplay out. If TDLib adds a way to
fetch the animations set without a click, autoplay can call the same
effect overlay with the unread-to-read transition as its trigger.

## Left out

- Emoji sounds. TDLib sends a sound file for some emoji. tdesktop does not
  play them, so neither does Quill. The file is parsed and kept for later.
- Custom emoji that need repainting (`needs_repainting`, drawn in the text
  colour) are not tinted when drawn big.
- No run on Linux or Windows hardware. The code path is the same streaming
  player and GPUI layout on every platform, with no platform-specific
  calls.

## Tests and verification

- Parsing (`message_animated_emoji_tests.rs`): skin tone and sound, no skin
  tone, a custom emoji gets its entity back, an unknown sticker stays text.
- Sizes and play state (`src/animated_emoji/tests.rs`): 112 pt sticker,
  glyph fallback, 112/78/58 custom sizes, lines count toward the size,
  play once then rest, a tap gives a new head, leaving the chat forgets.
- Rename (`emoji_interaction.rs`): a sent message keeps its play key.
- UI (`src/ui/animated_emoji_tests.rs`, on the `ready-animated-emoji`
  demo): the lone emoji is a sticker in its skin tone, it plays once and a
  tap plays it again (a second tap mid-play does not restart it), leaving
  the chat resets it, and custom emoji take 78 and 58 pt.
- Screenshot demo `ready-animated-emoji` shows an incoming 👋, an outgoing
  👋🏽 (`QUILL_DEMO_EMOJI_TONE=0..6` picks the tone) and the two- and
  three-emoji custom rows. `ready-emoji-tap-effect` now uses the same hand
  sticker. The demo hand (`demo-emoji-fitz.tgs`) has a fitz colour table
  so the skin tone is visible.
