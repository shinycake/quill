# Search inside shared media tabs

## What tdesktop does

- `data/data_shared_media.cpp:88` (`SharedMediaAllowSearch`): only the
  music, file, link and poll sections search. Voice, photos, videos and GIFs
  have no text to match and no search.
- `info/media/info_media_list_widget.cpp:91,955`: the Info top bar offers
  search once the section holds more than `kMediaCountForSearch = 10` items.
- `info/info_controller.cpp:462-516, 562`: a `SearchFieldController` feeds a
  `DelayedSearchController`; each keystroke produces a `SearchQuery` for the
  section's media type, and `mediaSource` asks the server with that query.
- `info/media/info_media_inner_widget.cpp:161-165` and
  `info/media/info_media_empty_widget.cpp:53-80`: the empty widget follows the
  query and shows `lng_media_file_empty_search` ("No files found"),
  `lng_media_song_empty_search` ("No music files found") or
  `lng_media_link_empty_search` ("No shared links found") instead of the
  "here yet" text.

## What Quill did before

The panel header had the tab strip and the calendar only; `SharedMediaTabState`
held no query and every `searchChatMessages` page went out with `""`.

## What changed

- `SharedMediaTabState::query` is the text the tab's items answer. Everything
  rendered (list, empty state, hidden count suffix and calendar button) is a
  function of that query and the results.
- `state/shared_media_search.rs`: `SharedMediaTab::searchable` (Files, Music,
  Links), `begin_search`, `select_tab_dropping_search`, and
  `Session::shared_media_search_shown` (the 10-item rule, or a query already
  set so the field never vanishes under the user).
- `connect/search/shared_media_search.rs`: `search_shared_media` sets the query
  and fetches the first page; first-page and older-page requests both carry the
  tab's query next to its filter, so paging works on results.
- `ui/shared_media_search.rs`: a kit `Input` with a search icon and clear
  button under the tab strip. Typing waits `SEARCH_DEBOUNCE`; an empty text
  returns to the plain list at once. The field is emptied on tab change and
  whenever the gallery is not showing it.
- Leaving a tab drops its query and results, and bumps the shared generation so
  a late answer for the dropped search cannot land.

## Deviations

- Voice has no search, as in tdesktop, although the task listed it. Polls
  search in tdesktop through their own list widget (`info/polls`, local
  filtering); not part of this change.
- tdesktop keeps a query per section in the navigation stack. Quill clears it
  when you leave the tab, as the task asked.
- Demo captures: the demo session has no server, so `ready-shared-media-search`
  seeds the answered state directly (`QUILL_DEMO_SHARED_MEDIA_QUERY` picks the
  text).

## Verification

Unit tests in `state/shared_media_search.rs`, a driver test with the recording
sender (`connect/tests/shared_media_search.rs`), a GPUI UI test
(`ui/shared_media_search_tests.rs`) and light and dark captures of the
`ready-shared-media-search` demo.
