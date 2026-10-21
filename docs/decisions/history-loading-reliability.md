# History loading reliability

Owner report (Idan, 2026-10-21): scrolling to the top or the bottom of a
chat sometimes loads nothing, and entering a chat sometimes shows nothing.

## How it was reproduced

`src/connect/tests/history_liveness.rs` drives the real `ConnectDriver`
against a model of TDLib's `getChatHistory` (from
`OrderedMessages::get_history` in TDLib 1.8.69): offset 0 returns messages
older than `from_message_id`, a negative offset includes it and newer ones,
and a page may come back short down to one message because TDLib stops at
the end of its in-memory run (Quill's own TDLib log shows the first page of
a chat answered with exactly one message). Answers arrive in any order. A
model of the history pane pages older while its first row shows and newer
while a row near the end shows, as `history_message_list` does. Random
opens, chat switches, scrolls, jumps to the latest, live incoming and
outgoing messages, deletions (of the latest message too), unread messages
that make the next open load around the read boundary, and "missed"
messages (the chat's last message moves without an `updateNewMessage` for
each message, which TDLib does after a long gap or sleep) run over 200
seeds. Whenever every request has answered, the open
chat must show messages, the window must have no gap, and the edge the
reader holds must have loaded to the end of the chat.

Before the fix, out of 300 seeds: 187 left the bottom stuck short of the
latest message, 66 left a gap inside the window, and 24 opened a chat with
nothing in it.

## Root causes

1. **A request blocked new ones while its answer was thrown away.** The
   in-flight check was per chat (`has_purpose_for_chat`), but the answer
   was accepted per view (`view_generation`, bumped on every chat switch).
   Open chat A, switch to B, come back to A before A's first page answers:
   the return trip sends nothing because A's page is in flight, then that
   page answers for an old view and is dropped. The window stays empty, the
   empty pane has no rows to trigger paging, and the chat shows nothing.
   The same shape after `jump_to_latest`: an older page still in flight
   blocked the latest page.
2. **"There are newer messages" was a stored flag updated only when a page
   landed.** When the chat's last message moved without an
   `updateNewMessage` for every message, nothing set `has_newer`, so the
   pane never asked for newer pages and the jump button only scrolled. The
   next live message was then appended to the window across the missing
   run. Reopening a chat kept its closed-chat tail for the same reason,
   even when the chat had moved past it.
3. **Pages were stored wherever they landed.** Any page for a shown chat was
   merged, even when the window had since been trimmed, released to a tail
   or replaced, which can also leave a gap.

## What tdesktop and Telegram X do

tdesktop keeps one request per direction (`_firstLoadRequest`,
`_preloadRequest`, `_preloadDownRequest` in `HistoryWidget`), cancels them
all when the chat changes (`clearAllLoadRequests`), and decides what to load
from scroll geometry after every change (`preloadHistoryByScroll`), not only
on scroll events. Its `History` tracks `loadedAtTop` / `loadedAtBottom` and
treats the chat's last message as the bottom. Telegram X's `MessagesLoader`
keeps one `isLoading` flag per loader, drops results for an old context,
and loads more at once when the first chunk is short.

## The fix

The window state is derived from what the window holds and what the chat is
known to have; requests are judged by whether their answer still fits.

- **`HistoryState::stops_short_of(chat_latest)`** replaces the stored
  `has_newer`. The window stops short when the chat's latest known message
  (its summary's last message, or the newest incoming id ever seen) is newer
  than the window's newest server message and not in it.
  `Session::history_has_newer` feeds it the summary. `newer_exhausted`
  remembers that TDLib answered "nothing newer" from a given newest
  message while a given latest id was known, so a last message that no page
  returns does not ask forever; it lapses once a newer message is known or
  the window no longer reaches that message. Deleting the newest incoming
  message lowers `latest_seen` back to what is loaded, so a deleted message
  is never something the window still has to reach.
- **Pages attach or are dropped.** `Session::history_page_detached` keeps a
  page only when it still extends the window: an older page asked from the
  message that is still the oldest, a newer page whose starting message is
  still loaded, a first page (the latest run, or the run around a message)
  into the window it was asked for. A first page that does not overlap what
  the window holds (live messages that arrived meanwhile, or a stale tail)
  replaces it instead of joining across a gap. A window replaced by a reset
  still marks its pages stale, so a page for a jump the reader left does not
  land later.
- **In-flight checks use the same rule.** `history_page_in_flight` counts
  only requests that would still attach, so a page for a replaced or
  trimmed window never blocks the one the window needs now, and an answer is
  never dropped while it blocks a new request.
- **Live messages** join the window only when it reaches the chat's latest
  message (or is empty). Otherwise they raise `latest_seen` and newer pages
  bring them, in order. A message the reader sends while the window waits
  for a page around the read boundary opens the chat at the latest instead.
- **A page around a message never marks the start of the chat.** Nothing
  older in it means only that TDLib's run in memory ends there; the top
  still pages through the server. Before, a chat opened with unread
  messages could stop loading at the top.
- **An older page waits for a first page on the way**, since that page may
  replace the window.
- **An empty pane asks for its first page** when nothing is on the way, so
  any path that leaves the window empty recovers on the next draw. The
  driver sends nothing for a chat known to be empty.
- **Reopening a chat** keeps its window only when it reaches the latest
  message (or covers the unread boundary); a failed newer page no longer
  blocks paging after the chat is reopened.

## Not changed

Reply threads (`ThreadView`) store their own `has_newer` the same way and
can fall behind in the same case; they were not part of the report and are
left for a follow-up. A newer page that fails still stops auto-paging until
the window is replaced or the chat reopened; the jump button replaces it.
Paging is still triggered by row rendering; `MessageScrollerState::
logical_scroll_top` from GPUI Kit 0.8 could drive it from scroll geometry
like tdesktop, which the simulation found no need for.
