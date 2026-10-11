# Dice emoji list and animated emoji taps

## What tdesktop does
- `stickers_dice_pack.cpp`: a message that is one dice emoji sends as a dice. The list comes from the server.
- `emoji_interactions.cpp` and `history_view_emoji_interactions.cpp`: tapping a lone emoji in a one-to-one chat plays a bigger effect sticker (3x the emoji) beside the message, sends `sendMessageEmojiInteraction`, and plays the partner's taps. When an incoming effect starts it sends `sendMessageEmojiInteractionSeen` (once a second per emoji). Effects are dropped when the message is not on screen; at most five play at once.

## What was already in Quill
`updateDiceEmojis` parsing, the server list in the attach menu, and routing a lone dice emoji to `inputMessageDice` (`dice_emoji_in`). Only the click update was missing, so the dice item stayed unchecked.

## What changed
- TDLib does the pack lookup and the tap request itself: `clickAnimatedEmojiMessage` returns the effect sticker (404 when the usual animation is enough), `updateAnimatedEmojiMessageClicked` brings the partner's, and `chatActionWatchingAnimations` replaces the "seen" request. New `src/emoji_interaction.rs` (request builders, queue, throttle), `src/telegram/envelope_interaction.rs` (parse the `sticker` answer and the update), `src/state/domains/stickers/interaction.rs` (queue the effect when its chat is open), `src/connect/emoji_interaction.rs` (driver calls; private chats only, like tdesktop).
- `src/ui/emoji_effect.rs`: a lone emoji (big emoji on, no entities, no preview) gets a click handler and notes where it is painted. Effects start only for a visible message, reuse the sticker clip decoder (`layered_clip`), play once from the clip's own clock, 5x the emoji height, hugging the bubble side, with tdesktop's random shift, and keep the frame clock ticking while they play. Incoming ones send the watching action.
- Hooks: `message_text_block`, `history_message_list` (reset the visible-emoji map), `conversation_slot` (overlay child), `layered_clip` visibility.

## Differences from tdesktop
The effect is anchored to the emoji glyph (the start of the text block, not the footer's reserved space) and mirrored for messages on the left: the decoded frames are flipped once when an effect starts and cached (two stickers at most). Effects past five wait and start by tdesktop's `checkDelayed` rule (200 ms apart, and with three playing one must be half played); a waiting effect is dropped after 3 s. Not done: autoplay when an unread emoji scrolls into view (TDLib has no equivalent) and the "watching animations" label in the top bar.

## How verified
Unit tests: flip, start rule, request JSON, queue bound, seen throttle, parsing of the answer and update, reducer in open and other chats, driver tap/404/group/watching, server dice list routing, geometry. gate.sh OK. Visual: a temporary demo (not committed) drew the effect sticker beside a lone thumbs-up in the ready-chats capture. Not verified live: real `clickAnimatedEmojiMessage` answers, the partner update, clicking by hand.
