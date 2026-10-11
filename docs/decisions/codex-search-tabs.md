# Global search tabs

Parity item: `parity:chatlist-search-tabs`.

## What tdesktop does

`Dialogs::Suggestions` (`dialogs/ui/dialogs_suggestions.cpp`) shows a strip of
tabs under the search field. `TabKeysFor` gives the order: Chats, Channels,
Apps, Posts, Photos, Videos, Downloads (only while the download manager has
entries), Links, Files, Music, Voice.

- Chats: recent chats and the top-people strip.
- Channels: "Channels you joined" (five rows, then Show more / Show less)
  and "Similar channels" from the channel recommendations.
- Apps: the apps you use and the popular ones.
- Posts: `PostsSearch` runs `channels.searchPosts`. The first request is
  free; the intro shows how many free searches are left and offers a paid
  search with Stars when they are spent.
- Photos, Videos, Links, Files, Music, Voice: `Info::Section::Type::GlobalMedia`,
  the shared-media list of every chat. Only Links, Files and Music take the
  typed query. Results page as you scroll.

## What Quill had

`SearchScope` (My messages, Public posts, Apps), a "Content" chip row that
joined photos and videos, and a chat-type row. Message results were replaced
by each new query and never paged. There was no Channels tab, no separate
Photos and Videos, and no quota line for posts.

## What changed

- `src/search_tabs.rs`: `SearchTab` (the ten tabs, in tdesktop's order) and
  its mapping to and from `GlobalSearchFilters`. Photos and Videos are new
  `SearchMediaKind` values (`searchMessagesFilterPhoto` / `Video`). Channels
  is the channel chat type (`searchMessagesChatTypeFilterChannel`), so a typed
  query lists channel posts and channel chats.
- The tab strip replaces the scope chips, the Content row and the Apps entry
  chip. It is one row that scrolls sideways (a plain scroll container of tab
  buttons, the vertical wheel too, and a picked tab scrolls into view). The kit
  `TabBar` was not used: its overflow menu hid the active tab in the narrow
  sidebar. Chat type (without Channels, which has
  its own tab), archive and date still narrow a typed query on the tabs that
  run `searchMessages`.
- Media tabs list their kind of message from every chat, also on an empty
  field (`searchMessages` with an empty query and the tab's filter). Rows use
  `SharedMediaItem::label_for` (now shared with the gallery) and month headers
  like the shared-media list; a row opens the message in its chat.
- Channels tab on an empty field: joined channels from the session (five,
  then Show more) and `getRecommendedChats` ("Similar channels", asked once
  per session).
- Paging: `SearchTabsState` keeps `next_offset`. "Load more" sends
  `searchMessages`, `searchPublicMessagesByTag` or `searchPublicPosts` with it
  (new purposes `SearchMessagesMore` and `SearchPublicPostsMore`, which carry
  the search generation so a late page of an old query is dropped). A failed
  page keeps the cursor and the button says so.
- Posts tab: `getPublicPostSearchLimits` runs with each public-posts query
  (it is free) and the strip shows "N free searches left today".
  `searchPublicPosts` still always sends `star_count: 0`.
- New code lives in `src/search_tabs.rs`, `src/state/search_tabs_state.rs`,
  `src/connect/search/tabs.rs`, `src/telegram/requests/search_tabs.rs` and
  `src/ui/search_ui/tab_strip.rs` / `tab_results.rs`; the search UI only gained
  small hooks.

- Downloads tab: listed between Videos and Links only while the download list
  has entries, like tdesktop. It shows the same Active / Failed / Recent
  sections as the downloads panel (`download_sections`, shared), narrowed by the
  typed text. No request is sent.
- Photos, Videos and Voice ignore the typed text, as `consumeSearchQuery` does
  (it only feeds Posts, Downloads, Links, Files and Music). The text is kept
  for the other tabs.
- Paging is infinite scroll: the results scroll under the fixed strip, and the
  next page is requested when the end is within 240 px, or when a page left the
  list shorter than the viewport. A "Loading…" row shows while it runs; only a
  failed page shows a retry button.
- Paid public-post search: after the free searches are gone, the Posts tab shows
  `Search for ★N` (N from `getPublicPostSearchLimits`) and when a free search
  unlocks. The button only opens an inline confirmation; the Stars are spent
  when the user accepts it (`search_public_posts_paid`, the only request with a
  non-zero `star_count`). The driver refuses unless the open query is a plain
  one whose free search was refused. Without Telegram Premium the tab says the
  search needs it instead of offering the button. Quill has no Stars balance
  check: a low balance comes back as a failed search.

## Left out

- tdesktop's small-balance top-up flow before a paid search.
- Apps tab: kept as it was (apps you use and grossing apps); tdesktop's
  popular-apps list has no TDLib method in 1.8.68.

## How it was checked

- Unit tests: tab order and labels, tab to filter mapping and back, shared
  filter constructors, request JSON (page offset, filters, schema lines),
  paging (append without repeats, one page at a time, end, failure and retry,
  a new query drops the cursor), quota text, media rows and month starts.
- Driver tests (`src/connect/tests/search_tabs.rs`): each media tab sends its
  filter, empty field on a media tab, back to Chats asks for recents,
  recommendations once, channel query filter, paging for messages, media and
  posts, quota parse, every `searchPublicPosts` free.
- Visual check with a temporary demo capture (not committed): Channels, a media
  tab with month headers and "Load more", and Posts with the quota line.
- Driver tests for the new parts: Photos, Videos and Voice send nothing for typed
  text and keep it, Links, Files and Music take it, Downloads sends nothing,
  the paid search is refused while free ones remain or for a tag, and carries the
  reported price only when called.
- A temporary demo capture showed the single-row strip, scrolling results and the
  paid confirmation.
- No paid search and no live Telegram request was made.
