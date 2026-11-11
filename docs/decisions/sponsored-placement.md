# Channel ads are placed in the history the way Telegram Desktop places them

Issue #1427 (2026-11-10): a channel's ad pulled the reader back to the bottom.
The ad was drawn below the message list and only while the list was at the
bottom, so scrolling up hid it, the list grew into its space, landed at the
bottom again and the ad came back. #1429 moved the ad under the last row. The
owner then asked for Telegram Desktop's placement in full.

## What Telegram Desktop does

`Data::SponsoredMessages` (`data/components/sponsored_messages.cpp`) keeps one
entry per ad of the fetched list. An entry is in the history once it has an
item, a real history item that scrolls with the posts. How entries become
items depends on the list's `messages_between` (`postsBetween`):

- **0, `append`.** Nothing is shown when the chat opens. The history scroll's
  bottom content request (`HistoryWidget`, `setBottomContentRequest`;
  `ElasticScroll::requestBottomContent` in lib_ui) runs when a wheel,
  trackpad or Down/PageDown scroll would go past the end of a history loaded
  at its bottom. It adds the next entry after the last message, and the
  scroll runs into it. Each later scroll past the end adds the next entry. An
  entry whose media is still downloading waits (`MediaLoading`). A list
  received more than five minutes ago adds nothing (`TooEarlyForRequest`).
  While an entry is not added, the bottom is not the end and the pull to the
  next channel waits (`hasUnshownFor`).
- **More than 0, `inject`.** After the first load and every load of newer
  messages, entries are put between posts. The first counts from the first
  unread message (`ShowAtUnreadMsgId`) or the message the chat was opened
  at; with nothing unread none is placed. Each later one counts from the ad
  before it. An entry goes after the message where at least
  `messages_between` messages and two scroll heights of content follow the
  start. When the loaded history ends first, the chat's first entry goes at
  the end of it and later ones wait.

An added item stays after the message it followed, so newer posts land below
it. Leaving the chat removes every item (`clearItems`). A refetch does not
replace a list while any of its entries is shown.

An ad counts as viewed once the visible area's bottom passes the ad's height
less its button and bottom padding (`Element::markSponsoredViewed`).

## What Quill does

- `ChatSponsoredMessages::placed` lists the placed ads in order, each with the
  message it follows. Leaving the chat (`open_chat` to another chat,
  `leave_open_chat`) clears it, and a refetch keeps the placements of ads
  still in the list. `Session::open_sponsored_layout` reads the mode from
  `messages_between`.
- `Session::place_sponsored_at_end` is `append`. The history's capture-phase
  wheel listener (`pull_wheel`) and PageDown call
  `QuillApp::sponsored_bottom_request` with the scroll's distance, which asks
  for an ad when the viewport is less than that distance from the end. An ad
  placed there takes the event, so the pull to the next channel does not
  start on it. `Session::sponsored_unshown_at_end` is `hasUnshownFor`.
- `QuillApp::inject_sponsored` is `inject`. It runs after the main history's
  rows are in, with the start from `Session::sponsored_inject_start` and row
  heights from `MessageScrollerState::estimated_row_heights`. Rows not measured
  yet count at the mean measured height; tdesktop lays out each item
  instead.
- `sponsored_spots` draws each placed ad under the row of the message it
  follows, outside that row's tracked bounds, and measures that row again
  when its ads change. The report picker and the outcome banner hang under
  their ad. "Hide ads" goes under the last ad placed.
- An ad whose media is not downloaded is placed but not drawn, and counts as
  unshown, so the order stays tdesktop's.

## Differences left

- tdesktop also places ads after a jump to a message (`_showAtMsgId`); Quill
  counts only from the first unread message.
- Quill counts album items as messages, as tdesktop does, but the two screen
  heights use estimated heights for rows not laid out yet.
- Only the main history window of the chat shows ads; a channel in its own
  window draws them from the session's open chat, as before.
