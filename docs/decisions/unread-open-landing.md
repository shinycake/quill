# Opening a chat at its unread messages

## What tdesktop does

- `HistoryWidget::countInitialScrollTop` scrolls so the "Unread messages" bar
  is at the very top of the view: `unreadBarTop()` is the bar's top plus its
  margin, plus the date badge above it, so neither the last read message nor
  the date shows above the bar.
- `countAutomaticScrollTop` only creates the bar when the first unread
  message is above `scrollTopMax + bar height - margin`. When the whole
  unread run fits on the last screen it opens at the bottom with no bar.
- The bar reads `lng_unread_bar_some` ("Unread messages"), with no count
  (`History::addUnreadBar`).
- The down button (`CornerButtons::downClick`) shows the history at
  `ShowAtUnreadMsgId`: the first unread message under a new bar while any
  are left, the bottom otherwise. Its badge is the chat's unread counter.
- `cornerButtonsDownShown`: the button shows past `historyToDownShownAfter`
  (480px) from the bottom, while unread messages are below the view, and
  always while the history is not loaded at its bottom.

## What Quill does

- The opened history scrolls the bar's row to the top with the offset found
  last time (`unread_bar_anchor.rs`). After the next paint, the history
  measures how far the bar is from the top and corrects the offset. If the
  bar can't reach the top because the list is at its end, the bar is
  dropped (`Session::hide_unread_bar`) and the history follows its tail. The
  bar is in a row, so its offset can't be known before layout. Reads stay
  held back (`opened_with_unread`) after the bar is dropped.
- The jump-down button calls `ConnectDriver::jump_down`, which brings the
  bar back at the chat's read boundary (`Session::show_first_unread`). If
  the window holds that message, it re-anchors there. If not, it loads the
  window around the boundary. With nothing unread, it jumps to the latest
  messages as before. The End key still goes to the latest.
- The kit scroller gained `with_jump_button_shown_after` and
  `jump_button_always` (third_party/gpui-component/QUILL-CHANGES.md). Quill
  passes 480px, or zero while the chat has unread messages (they're below
  the view, since rows on screen are read), and `always` while the window
  stops short of the latest.

Reply threads keep their own bar and jump to their latest replies. The
landing check applies to them too, but the jump-down change doesn't.
