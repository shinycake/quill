# Message effects playback

Parity id: `premium-message-effects-play`. tdesktop sources are under
`Telegram/SourceFiles/`.

## What tdesktop does

- A message keeps one flag, `MessageFlag::EffectWatched`
  (`history/history_item.cpp`, `applyInitialEffectWatched`). It is decided
  once, when the message reaches the client. A message sent from this
  client starts unwatched. An outgoing message from the server (sent from
  another device) starts watched. An incoming message starts unwatched
  when it is past the chat's read marker. Reading it later on another
  device does not change the flag.
- When the message is shown while the window is active, the history calls
  `markEffectWatched` and `playEffectOnRead`
  (`history/view/history_view_emoji_interactions.cpp`). The flag clears
  whether or not the effect plays. With power saving `kChatEffects` on,
  nothing plays by itself, but a click on the badge in the time info
  (`history/view/history_view_bottom_info.cpp`) always plays it.
- The effect is drawn at `2 × 112` px, centred on the badge icon and moved a
  third of its width toward the middle of the chat, mirrored for incoming
  messages (`computeRect`).
- The badge icon and the effect animation preload as soon as the message
  is in view (`Reactions::preloadEffectImageFor`, `preloadEffect` in
  `data/data_message_reactions.cpp`).

## What Quill had

Playback already existed: the badge (`src/ui/message_effect_badge.rs`), the
overlay (`src/ui/emoji_effect.rs`, shared with emoji interactions and
Premium sticker effects, on the frame clock) and the pass that starts
effects (`src/ui/message_effect_play.rs`). Offline it played. The
"has it played" state was worked out in the UI on every render and stored in
a thread-local set. That differed from tdesktop in three ways:

- An incoming message read on another device while Quill was in the
  background lost its effect. Its id fell behind the read marker before it
  was ever shown.
- A message sent from here lost its effect when TDLib gave it its final id.
  The unwatched state was `pending`, which ends at that moment.
- The effect animation only started downloading when the effect was asked
  to play. On a cold cache the first play waited for the download.

Chat windows also shared the paint notes with the main window. Each history
render in a chat window cleared the main window's badge positions.

## What changed

- `src/message_effect_watch.rs` holds the tdesktop flag in the session
  (`Session::stickers.effects.watch`). `note_message_effect` decides it when
  a message is stored (`upsert_message`, and the new-message path outside
  the loaded window). The send-succeeded and send-failed updates carry it
  to the final id. Both sets are bounded.
- The badge reads the flag. The play pass marks each unplayed badge that is
  on screen, plays it unless power saving turns chat effects off, then
  redraws the conversation so the badge stops asking for frames.
- `thumb_file_ids_to_download` now also fetches the effect's animation (the
  emoji reaction's `effect_animation` or the Premium sticker's
  `premium_animation`), not only the icon.
- Effects play over the main window only. Badges in a separate chat window
  are not noted, and its history no longer clears the main window's notes.
- Demo stage `QUILL_DEMO_EFFECTS=arrive` on `ready-message-effects`: an
  unread message with an effect arrives in the open chat.

## Not done, or different from tdesktop

- Separate chat windows do not play message effects, emoji interactions or
  Premium sticker effects. tdesktop plays them in every window. The overlay
  is drawn by the main window's conversation slot only.
- tdesktop holds an incoming message's flag until the chat's read marker is
  known (`applyEffectWatchedOnUnreadKnown`). Quill counts a chat whose read
  marker is unknown as read. TDLib sends the chat before its messages, so
  this only matters for a message of a chat Quill has not seen.
- The flag lives in memory. A restart forgets which messages played.
  tdesktop also keeps it only in memory.

## Verification

- Unit tests in `src/message_effect_watch.rs`: which arrivals start
  unwatched, one automatic play, no reset when the message is stored again,
  reading elsewhere, the final id, and the bound.
- Driver tests in `src/connect/tests/message_effects.rs`: the animation is
  downloaded with the icon, and the flag follows a real update sequence
  (pending send to final id, sent elsewhere, unread then read elsewhere,
  already read).
- GPUI tests in `src/ui/send_effects/tests.rs`: an arriving unread message
  plays once, not again after its effect ends and the history is drawn
  again, and a tap on its badge replays it. With power saving on, it does
  not play by itself but is marked as played, and a tap still plays it. A
  tap on the badge of a message that already played plays it.
- `/verify` on `ready-message-effects` in light and dark: the chat opens
  with nothing playing (both effect messages already read or sent), the
  `arrive` stage shows the effect over the new message, beside its badge,
  shifted toward the chat's centre, and a click on the "Happy birthday!"
  badge replays its effect. The demo's effect animation is a placeholder
  circle, so the captures show placement and timing, not a real effect.
- Not run on a live account. Real effect animations and the owner's
  Premium account are unverified.
