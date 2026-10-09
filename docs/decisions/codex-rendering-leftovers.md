# B17: message rendering leftovers and media viewer

## Already in main (verified, no change)
- URL hover tooltip: `SelectableRichText::on_range_hover` + `LinkTarget::tooltip` (`message_text.rs`).
- Code blocks: language header plus a hover Copy button (`pre_block`).
- Sender avatar sits beside the last bubble of a run (spacer on earlier rows), `conversation.rs` identities.
- Chat-photo change service row with its photo tile; protected-content toggle wording; story rows (text).
- Birthday row: TDLib 1.8.67 has no birthday message content (tdesktop's is a chat-top suggestion bar), so nothing to render.

## What tdesktop does and what changed
- **Grouped bubbles / tails** (`history_view_message.cpp`, `message_bubble.cpp`): a bubble joined to its neighbour has a small radius on the avatar side; the last bubble carries the tail. Quill now gives incoming bubbles a small bottom-left corner (outgoing: bottom-right), and a small top corner when the previous row is the same sender with no day divider or unread marker between (`BubbleLook::joined_above`, `bubble_corners`). The tail is the squared corner only; no tail glyph is drawn.
- **Contact card** (`history_view_contact.cpp`): avatar, name, phone and Message / View contact / Add contact. Buttons follow `ContactCardState` (self, known, unknown, phone only). Add contact opens the add-contact dialog prefilled from the card.
- **Location** (`history_view_location.cpp`): static tile. `getMapThumbnailFile` is requested once per place for the open chat (`MapThumbs`, `maybe_request_map_thumbs`), downloaded like a photo thumbnail, drawn with a pin and click-to-open-maps.
- **Dice** (`history_view_dice.cpp`): `messageDice.final_state` (regular dice) is parsed into `DiceContent::final_sticker` and played once through the sticker pipeline (stops on its last frame whatever the loop setting says). Slot machines (five stacked stickers) keep the emoji face; "Rolled N" always states the value.
- **Composer**: a message that is exactly one dice emoji sends `inputMessageDice`. The attach menu gains Contact (address-book chooser) and Location (typed, validated coordinates; tdesktop uses a map picker) panels sending `inputMessageContact` / `inputMessageLocation` with the composer's reply and send options.
- **Paid media**: `messagePaidMedia` previews (`paidMediaPreview`) render as a blurred locked card with the Stars price, item count and video length, plus caption. Unlocking (`payForPaidMedia`) is not wired, so it is informative.
- **Suggested profile photo**: View Photo and (incoming) Set as My Photo (`setProfilePhoto` with `inputChatPhotoPrevious`, using the new `photo_id`).
- **Media viewer** (`media_view_overlay_widget.cpp`): sender name plus "today at HH:MM" header (`local_time::viewer_stamp`), Copy Frame for videos, "Saved to Downloads" (`media_viewer::saved_note`), "View All Media" (opens the chat's Shared Media gallery). The status toast now paints above the viewer; before, viewer notices ("saved", "copied") were hidden under the overlay.

## Skipped
Gift and giveaway cards, forwarded-story card, slot-machine reels, dice replay on click, unlocking paid media, copy-frame while the macOS native surface plays, "View all" paging the viewer itself.

## Verification
Unit tests: dice final-state / slot machine / paid media / suggested photo parsing, share request shapes, coordinate validation, dice routing and contact/location sends through the driver, map tile request-once-then-download flow, `viewer_stamp`, `saved_note`. Demo captures (`--screenshot-demo ready-rendering-leftovers`, `QUILL_DEMO_RENDERING_VIEW=bubbles|cards|service|viewer|contact|location`) in light and dark were inspected. Not verified live: real `getMapThumbnailFile`/dice stickers, accepting a suggested photo, sending contact/location, Copy Frame clipboard contents.
