# Fast share button beside channel posts

## What tdesktop does
`Message::displayFastShare` (history_view_message.cpp) shows a round button
beside the bubble when forwarding is allowed and the message is a broadcast
channel post, a forward from a channel in a private chat (incoming), or an
incoming bot message with media (`allowsFastShare`: files, photos, games, web
pages, grouped media). `displayGoToOriginal` shows it for forwards that keep a
source peer and message (Saved Messages); the click then jumps to the
original instead of forwarding. A plain click opens the share box.

## What changed
- `MessageForwardInfo.source` now carries TDLib's `messageForwardInfo.source`
  chat and message id.
- `corner_buttons::fast_share` is the pure show/hide decision (unit tested);
  `corner_buttons::span` reserves room for the extra slot.
- `src/ui/history/fast_share.rs` builds the facts from the session (chat kind,
  bot user, protected content, forward origin) and the kit round button, first
  in the hover row: Forward icon opens the existing forward picker
  (`begin_forward_one`), ArrowUpRight jumps via `go_to_message_in`.
- Forwarding is allowed when the chat is not protected, the message is not
  ephemeral (tdesktop `IsAnchoredEphemeral`; Quill has no anchored list, so
  every message with ephemeral content counts), and, when TDLib's cached
  `messageProperties` answer is for this message (the one open in the
  message menu), its `can_be_forwarded`. No request is made per hover; other
  messages use the chat flag, as the menu does before the answer arrives.
  Secret chats map to "other", so they never get the button.
- Forwards from a channel in a private chat skip the button where
  tdesktop's `showForwardsFromSender` is true: Saved Messages, the Replies
  chat and the Verification Codes chat. Imported forwards are not modelled
  by Quill, so that clause is left out.
- Bot media follows `allowsFastShare`: every `File` subclass (documents,
  audio and voice, photos, GIFs, videos, video notes), games, albums
  (`media_album_id`, so each row of an album gets it) and web page previews
  (a text message with a link preview). The first review read this set as
  too wide; tdesktop's `Photo` and `Gif` views do inherit `File`, which
  returns true, so Quill matches it. Stickers, polls, locations and plain
  text stay out.
- Go to original is hidden in reply threads (comments), tdesktop
  `Context::Replies`; forum topics are not reply threads. Quill has no
  pinned-messages list or monoforum view (the main history list is the only
  caller), so `isPinnedContext` and `Context::Monoforum` need no handling.
- Tooltip and accessible label reuse the existing "Forward" and "Go To
  Message" strings.

## Deviations
- tdesktop's Ctrl+click (share to Saved Messages) is not covered.
- tdesktop's `!externalReply() || !hasBubble()` clause of the go-to-original
  rule has no equivalent yet.

## Verified
Screenshot demo `ready-showcase` with `QUILL_DEMO_SHOWCASE=channel`: hover
capture in light and dark shows the button first in the row; a click opens the
forward picker. Go-to-original is covered by the predicate test only (no demo
fixture with a forward source). Forward `source` parsing has envelope tests
(present, null, zero ids).
