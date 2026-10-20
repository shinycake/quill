# Unread bar when its message is deleted

## What tdesktop does

The "N Unread Messages" bar belongs to the message view it sits above.
`History::addUnreadBar` stores the first unread view in `_unreadBarView` and
creates the bar on it (history/history.cpp:2676-2681). When that view is
removed, `History::mainViewRemoved` hands `_firstUnreadView` to the next unread
message but only nulls `_unreadBarView` (history/history.cpp:1632-1634). The bar
disappears with the deleted row and is not recreated on the next one; a bar
only comes back when the chat is reopened. Sending a message destroys it too
(`destroyUnreadBar`, history/history.cpp:2683, called for outgoing items in
`newItemAdded`). The bar never slides to another message.

## What Quill did

The divider is derived from `unread_anchor` (the last read id): the first
incoming message with an id above it. Deleting that message left the anchor
alone, so the rule picked the next incoming message, and the bar jumped below
any outgoing message in between ("Reply from the session reducer.").

## Change

`History::remove` clears `unread_anchor` when the removed message is the one the
bar sits above (`History::unread_bar_message`, the same rule the rows use). The
bar is a pure function of the anchor, so it leaves with the row. Removing any
other message leaves it alone. The count (`unread_at_open`) is irrelevant once
the bar is gone.

## Verification

Unit test `deleting_the_message_under_the_unread_bar_removes_the_bar`. Demo
capture of ready-chats with a temporary hook deleting message 101 shortly after
open, light and dark: before, the bar sits above "Hello from injected JSON.";
after, no bar and the other rows unchanged.

## Read state stays separate from the bar

tdesktop keeps `_firstUnreadView` (where the chat opened, which gates
reading) apart from `_unreadBarView` (the divider); `mainViewRemoved` nulls
only the latter. Quill mirrors that: `HistoryState::opened_with_unread`
holds back the newest-row `viewMessages` fallback until the UI reports
visibility, while `unread_anchor` only drives the bar. Deleting the bar
message before any report therefore drops the bar and still reads nothing.
Jumping to the latest and sending end both (`end_unread_open`).

Window trim and tail prune go through the same `drop_unread_bar_for`
rule as deletes, computed once per batch.

Known gap: thread views (`ThreadState::unread_anchor`, `threads.rs`,
`session_thread.rs`) still slide the bar to the next incoming message when
its message is deleted.
