# Stickers and reactions: set box, custom emoji card, who reacted, corner buttons, auto replace

## What tdesktop does
- Tapping a sticker in a chat opens its set in `StickerSetBox`.
- The box has a three-dot menu: Share Stickers (or Share Emoji), Copy Link, and Archive Stickers for installed sticker sets. Copy Link toasts "Link copied to clipboard." Archiving closes the box with "Sticker set has been archived." The box has no report entry. The link is `addstickers/<name>` or `addemoji/<name>`.
- Tapping a custom emoji opens a preview of the emoji with the label "This emoji is from the {name} pack." Clicking the label opens the pack (`ShowReactionPreview`).
- Right-clicking a reaction chip lists who picked that reaction (`ShowWhoReactedMenu`). Chips show reactor avatars when few people reacted.
- Chat settings have "Reply button on messages" and "Reaction button on messages" (`cornerReply`, `cornerReaction`, both on by default). Hovering a message shows those buttons beside it.
- "Replace emoji automatically" (`replaceEmoji`) switches the composer's instant replaces between the full set and the text-only set (dashes, guillemets, `:shrug:`).

## What changed
- `src/sticker_set_box.rs`: pure link, wording and archive rules, with tests.
- Sticker messages with a set open the set box on click. `StickerSetViewStage::Ready` now carries the set name and whether it holds custom emoji (`stickerSet.name`, `sticker_type`). The box footer has a "..." menu with Share, Copy Link and Archive. Share puts the link in the existing chat chooser, so it lands in a composer unsent. Archive reuses the existing `changeStickerSet` call with is_archived.
- Custom emoji in message text are clickable. A tap sends `getStickerSet` for the emoji's set (`RequestPurpose::CustomEmojiPack`) and shows a card with the emoji, the label and a View button that opens the set box. The card closes itself after six seconds.
- `src/reaction_who.rs`: hover text for a chip (names, "and N more", or the count) and the rule for when a chip can open the list. Right-clicking a chip opens the message menu on the reactor list. With several reactions it opens that reaction's tab, fetched as soon as the audience loads (`Session::wanted_reactor_tab`).
- `src/corner_buttons.rs` plus two switches in Settings: Reply and React buttons appear beside a hovered bubble before the "..." button. The react button opens the message menu, which leads with the reaction strip.
- `src/emoji_replace.rs`: instant replacement after a single typed character. Text replacements always run. Emoticons and `:name:` shortcodes run with the new "Replace emoji automatically" switch in Appearance.

## Not done
- Sticker send animation (the sticker flying from the panel into the chat), the emoji set style picker and animated emoji statuses were not started.
- The double-click-to-reply setting is missing, so reactions-corner-settings stays open. Albums only get the "..." button, because the album row has no session at hand.
- The emoticon table is a short list of common ones, not tdesktop's full generated table, so emoji-replace-auto stays open. Backspace does not undo a replacement.
- The custom emoji card shows a still, not the animated preview.

## How verified
- Unit tests for the link and wording rules, tooltip text, corner button visibility, and instant replacement.
- Demo captures (English fixtures) of the sticker set box with its menu button, the reactor list on the thumbs-up tab, the custom emoji card, and the Appearance emoji switches. Hover states, the dropdown popup and typing replacement could not be captured and are untested in a live window.
