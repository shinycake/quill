# Sticker send animation, Premium stickers section, animated emoji status

Branch `codex/stickers-emoji-bundle`. Parity items: `stickers-send-animation`,
`stickers-premium-section`, `emoji-status-animated`.

## Sticker flies into the chat (`ui/send_fly.rs`)

tdesktop (`ui/effects/message_sending_animation_controller.cpp`): choosing a
sticker in the panel notes its on-screen rectangle; when the sent message's
view is created, the sticker glides from there to its place over
`itemRevealDuration` (150 ms, ease-out quint; x eases a second time), growing
from the cell's size to the message's. It is skipped when animations are off
(`anim::Disabled`) or "Effects in messages" (`kChatEffects`) is on. It runs for
stickers, GIFs and plain text; a message that is only emoji is explicitly
skipped (`isolatedEmoji() || onlyCustomEmoji()`), so there is no emoji flight
to build. Text belongs to the composer work and is not touched here.

Quill: panel cells note their painted bounds (`cell_probe`); the click notes
the image the cell showed; after a successful send the flight is armed. The
sent message is recognised by chat, file, outgoing and a date no older than the
send (or pending). Its sticker is replaced by an empty box of the same size and
reports its place while it waits; an app-level overlay (over the conversation,
so the cached history slice is never rebuilt per frame) draws the picture along
the tdesktop curves, then the real sticker returns. No target within 2 s ends
the flight. New power-saving switches: `ChatEffects` ("Effects in messages",
bit 1<<10) also gates the delete dust (`ThanosEffect::Supported`), and
`EmojiStatus` (1<<9).

Differences: the flying picture is the frame the cell showed at the click (a
hovered animated sticker is mid-animation) held for 150 ms, where tdesktop
draws the live media; the demo sender gives messages date 0, so the flight is
for live sends only.

## Premium stickers section

tdesktop: the sticker panel's search row has a category strip
(`messages.getEmojiStickerGroups`); its Premium group lists the Premium
stickers (`getPremiumList`); every Premium sticker carries `StickerPremiumMark`
(lock; star with Premium); Premium stickers are hidden when Premium cannot be
bought (`PrepareStickers(skipPremium)`); choosing one without Premium opens the
stickers Premium preview with that sticker (`ShowSendPremiumError`).

Quill: `getEmojiCategories(emojiCategoryTypeRegularStickers)` fills a strip
under the sticker search field (hidden while typing, like tdesktop). A search
category runs `searchStickers` with its emoji; the Premium category runs
`getPremiumStickers` (TDLib's documented source for it). Results reuse the
panel's found-stickers grid. Locked stickers get a Premium-gradient lock disc
(star with Premium) in the panel and in the sticker boxes (replacing the old
"Premium" text), are hidden when `is_premium_available` is false and the
account lacks Premium (`Session::premium_badges_shown`), and choosing one opens
the Premium dialog on the stickers feature with that sticker first. The gate is
shared by the panel, suggestions and story replies.

Differences: TDLib's `getPremiumStickers` replaces tdesktop's client-side
`getPremiumList`; the preview is Quill's existing tile grid (clicked sticker
first) not tdesktop's vertical carousel; a chosen category is left with a back
button or by typing.

## Animated emoji status (`ui/status_emoji.rs`)

tdesktop wraps a status in `LimitedLoopsEmoji(kPlayStatusLimit = 2)`: two loops
from first paint, then the first frame; "Autoplay in premium status"
(`kEmojiStatus`) keeps it still. Shown in chat rows, the chat header, profile,
message sender names (Premium or channel senders other than the chat itself,
the star when no status), main menu and settings. Channels carry statuses too,
and all Premium marks follow `premiumBadgesShown`.

Quill: the animation is drawn by the slice animation layer (`LayeredClip` gains
`loops`), counted per surface and owner from its first showing, in chat rows,
the header and group sender names; the profile panel shows the still when it is
outside a layer. Channel statuses, the sender-name star/status, the profile
badges and the badge gating by `premium_badges_shown` are new. Quill has no
main-menu or settings account header, so there is nothing to add there.

Difference: group sender rows are rebuilt with the history, so a sender's
status change shows on the next rebuild.

## Verification

Unit tests (power flags, `LayeredClip` loops, flight curve, message matching,
status first-shown, request JSON, category parsing), a driver test with the
recording sender, state tests for sender/channel statuses and gating, and GPUI
UI tests (`media_panel/panel_tests.rs`: categories, hiding, locked click, the
whole flight from panel to landing, effects-off). A temporary demo showed the
strip, the lock discs and animated statuses in a row and the header (not
committed). Not verified: live TDLib answers for the categories and
`getPremiumStickers`, and a real Premium account.
