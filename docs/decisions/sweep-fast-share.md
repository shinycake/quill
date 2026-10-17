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
- Protected chats and secret chats never get the share button. Tooltip and
  accessible label reuse the existing "Forward" and "Go To Message" strings.

## Deviations
- tdesktop's Ctrl+click (share to Saved Messages) and the web-page text
  messages from bots are not covered; bot text with a link preview gets no
  button.
- Forward permission uses the chat's protected flag, as the menu does before
  `messageProperties` arrives.

## Verified
Screenshot demo `ready-showcase` with `QUILL_DEMO_SHOWCASE=channel`: hover
capture in light and dark shows the button first in the row; a click opens the
forward picker. Go-to-original is covered by the predicate test only (no demo
fixture with a forward source).
