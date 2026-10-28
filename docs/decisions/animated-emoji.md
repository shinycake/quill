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

## The effect on read

tdesktop plays the tap effect once by itself when an unread incoming lone
emoji in a one-to-one chat is shown while the history is being read.
`HistoryInner` collects those rows (`startInteractions`) and calls
`EmojiInteractions::startAutoplay` (`chat_helpers/emoji_interactions.cpp`),
which picks an effect at random from the local "EmojiAnimations" pack and
plays it with `incoming = false`. The other side is told nothing. Quill
does the same (`quill::emoji_animations`).

`clickAnimatedEmojiMessage` is never used for this. It queues a
`sendMessageEmojiInteraction`, which would tell the partner that this user
tapped their emoji every time a message is read.

- The pack. tdesktop loads `inputStickerSetAnimatedEmojiAnimations`.
  TDLib has no request for that input, but the set also answers to its
  short name, which tdesktop's birthday effect already uses
  (`info_profile_birthday_effect.cpp`). Quill sends `searchStickerSet`
  with name "EmojiAnimations" the first time a one-to-one chat shows rows,
  and again two hours after each answer or failure, tdesktop's
  `kRefreshTimeout`. `StickersPurpose::EmojiAnimations` routes the answer.
- The map. `stickerSet.emojis` lists each sticker's emoticons. TDLib fills
  it from `sticker_emojis_map_`, the raw emoticons of the set's packs
  (`StickersManager::get_sticker_set_object`), so the keycap tags come
  through: TDLib's own click path reads them from the same map
  (`get_emoji_number`). As in tdesktop's `collectAnimationsIndices` and
  `IndexFromEmoticon`, "1⃣" to "9⃣" are indexes 0 to 8, "🔟" is 9, and
  the sticker's other emoticons are the emoji it animates. Variation
  selectors are dropped from the key, so "❤️" and "❤" match.
- Choosing. An emoji with no effects of its own falls back to the same
  emoji without its skin tone, and the coloured hearts fall back to the
  red heart (`chooseInteractionEmoji`). One effect is picked at random.
- The trigger. `report_visible_history` hands the rows the active window
  shows to `ConnectDriver::autoplay_emoji_effects`, just before
  `viewMessages` marks them read. A row plays when the chat is one-to-one
  and the message is incoming, sent, newer than
  `last_read_inbox_message_id`, and drawn as one big emoji with "Large
  emoji" on: a lone animated emoji, a lone custom emoji or a lone large
  glyph, which is tdesktop's `CanPlayEmojiInteraction`. With "Animated
  emoji" in chats saved by power saving (`Flag::EmojiChat`, the gate the
  lone emoji sticker uses), nothing plays and the message is not marked,
  as in tdesktop.
- Once. The pack state remembers the messages that played this session,
  like tdesktop's `EmojiInteractionWatched` flag. A read message also
  stops counting as unread.
- Playback. The effect goes into `EmojiInteractions` with `incoming:
  false`, and the window plays it like a tap's: 336 pt, anchored on the
  sticker, mirrored for a message on the left, with the lone emoji under
  it playing along. Since `incoming` is false, no
  `chatActionWatchingAnimations` goes out either.
- Before the pack first arrives, rows that qualify wait (eight at most).
  Their effects play when the pack comes, if the chat is still on screen.
  A failed request drops them.

## Left out

- Emoji sounds. TDLib sends a sound file for some emoji. tdesktop does not
  play them, so neither does Quill. The file is parsed and kept for later.
- Custom emoji that need repainting (`needs_repainting`, drawn in the text
  colour) are not tinted when drawn big.
- An effect that waited for the pack's first arrival plays however late
  the pack comes. tdesktop drops a pending effect after two seconds
  (`kMaxDelay`), but it loads the pack at startup and never waits for it.
  The window still drops an effect whose message is off screen.
- tdesktop counts an unread row as shown once its bottom edge is in view.
  Quill uses the rows the history list rendered, the same set it views
  with `viewMessages`, which can include a row just past the edge. That
  row's effect is dropped (it has no visible anchor) and, once read, it
  does not play later.
- The effect on read has not run against a live account. The pack's shape
  comes from TDLib's source, not from a real answer.
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
- The pack (`src/emoji_animations_tests.rs`): index tags with and without
  the variation selector, the map by emoji and index, skin tone and heart
  fallbacks, picks, refresh timing, the wait list.
- The trigger (`src/connect/tests/emoji_autoplay.rs`, recording sender):
  the pack is asked for once with `searchStickerSet`, groups neither load
  nor play, an unread incoming emoji plays once with `incoming` false,
  read, outgoing, text and effect-less messages do not, power saving and
  "Large emoji" off keep it still, an emoji read before the pack arrives
  plays when it does, a failed request retries later. No test sees a
  `clickAnimatedEmojiMessage` or `sendChatAction` go out.
- Parsing (`tests_media.rs`): `stickerSet.emojis` stays aligned with the
  stickers it describes.
- UI (`src/ui/demos/emoji_read_tests.rs`, on the `ready-emoji-read-effect`
  demo): the effect starts over the unread hand through
  `report_visible_history`, and does not repeat.
- Screenshot demo `ready-emoji-read-effect`: the demo pack gives the hand
  one effect, and the capture shows it playing over the unread message.
