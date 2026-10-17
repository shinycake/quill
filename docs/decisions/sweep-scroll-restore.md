# Scroll position restore per chat

## What tdesktop does
`HistoryInner` counts `History::scrollTopItem` / `scrollTopOffset` on every scroll
(`countScrollState`), forgets them while the view is at the bottom
(`forgetScrollState`, history_inner_widget.cpp ~4585) and whenever the chat is shown at an
explicit target (history_widget.cpp 2850, 3110, 3339). Showing a chat at its default
position reads the saved item back, so leaving a chat mid-history and returning lands on the
same message.

## What changed
- `src/state/scroll_memory.rs`: `ScrollMemory`, an LRU (300) of `(chat, topic, thread)` to
  `(anchor message id, pixel offset)`, plus `scroll_open`, the pure restore/jump/default rule.
- `history_message_list.rs`: each render records the top row while the reader is away from the
  tail and forgets it at the tail; when a chat is opened and its rows exist, the saved anchor is
  used if its message is in the loaded window, otherwise the old highlight / unread divider /
  bottom behavior. A jump highlight wins and drops the entry; replacing the window of the open
  chat (jump to latest, date jump) forgets it. Switching accounts clears the memory.
- Vendored `gpui-component` scroller gained `top_anchor` and `scroll_to_item_offset`
  (listed in its QUILL-CHANGES.md; re-apply when re-vendoring).
- Anchors are message ids plus an offset inside the row, so row height changes do not move them.

## Verified
Demo `ready-chats` with `QUILL_DEMO_STRESS=3,300`: scroll to message 12-18, switch to Demo
chat B, back to Demo chat A. The returned view is pixel-identical in layout to the one left.
Not shown: real TDLib windows that reload on open (anchor outside the window falls back).

## Review follow-ups
- An anchor outside the loaded window no longer loses the entry. The chat shows its default
  position, the saved position is neither recorded over nor forgotten, and it is restored when a
  window or page containing the anchor arrives. The wait ends on the reader's own wheel or
  trackpad scroll (`MessageScrollerState::user_scrolls`), an explicit jump, or leaving the chat.
  Not done: requesting history around the anchor on open as tdesktop does; Quill relies on the
  window loading the anchor by itself.
- `top_anchor` gates on the list's follow state instead of the previous layout's extent, so the
  restored position reads correctly on the restore frame (test in `message_scroller.rs`).
- The position is also recorded from the scroller's observer, so it follows every scroll and
  does not depend on a full app render.
- Verified: scroll up in Demo chat A, Demo chat B, Demo chat A, and again B, A. The view after
  one and two round trips shows messages 7 to 13 at the same offsets, light and dark.
