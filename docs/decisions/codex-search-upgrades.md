# Search upgrades (batch B6)

## What tdesktop does

Read from `Telegram/SourceFiles/dialogs/ui/dialogs_suggestions.cpp`,
`dialogs/ui/top_peers_strip.cpp`, `settings/sections/settings_privacy_security.cpp`
and `Resources/langs/lang.strings`.

- Empty search shows a "Frequent contacts" strip (`lng_recent_frequent`) of
  people (avatar + first name) above "Recent" (`lng_recent_title`).
- Right-click on a strip tile: "Remove from Recent" (`lng_recent_remove`) and
  "Remove all & Disable" (`lng_recent_hide_top`), the latter behind a
  confirmation (`lng_recent_hide_sure`).
- Every Recent entry has "Remove from Recent"; the "Clear" link on the
  heading asks `lng_recent_clear_sure` first.
- Settings > Privacy > "Suggest frequent contacts" (`lng_settings_top_peers_*`)
  turns the strip off and on.
- Global search has the tabs My Messages / This Chat / Public Posts
  (`lng_search_tab_*`) and the chat-type filter All / Private / Group /
  Channels, plus "From archive" (`lng_search_filter_from_archive`).
  A hashtag click opens the same tabs for that tag.
- In-chat search shows "N of M" (`lng_search_messages_n_of_amount`) and
  "From: user" (`lng_dlg_search_from`).

## What changed in Quill

Requests (`src/telegram/requests/chat_list.rs`), all with shape tests:
`searchChatsOnServer`, `searchPublicPosts` (always `star_count: 0`, so Quill
never pays for a search), `searchPublicMessagesByTag`, `getTopChats`
(`topChatCategoryUsers`), `removeTopChat`, `removeRecentlyFoundChat`.
`searchMessages` also carries `chatListArchive` and `max_date` now.

State (`src/state/search_types.rs`):

- `server_chat_ids` are merged behind the offline `searchChats` hits
  (`merged_chat_ids`). They deliberately do not gate the search status: the
  existing three-request flight and its tests are unchanged, and a late or
  failed server answer can only add rows (an `Empty` search becomes `Ready`).
- `top_chats` / `top_chats_disabled` back the strip. The flag follows the
  TDLib option `disable_top_chats` (`updateOption`), and a `getTopChats`
  reply that was already in flight cannot bring the strip back.
- `foundPublicPosts` is parsed into its own payload; an exhausted free quota
  (`are_limits_exceeded`) is shown as a note, never paid for.
- `SearchConfirm` and `top_menu` hold the inline confirmation / menu rows, so
  they live with the search state instead of in new `QuillApp` fields.

Filters (`src/search_filters.rs`): `GlobalSearchFilters` gained `archived`
and `scope` (`MyMessages` / `PublicPosts`); `SearchDateRange::Older` closes
the window with `max_date`. `tag_query` recognises a single `#tag` / `$TAG`.
Public-posts scope sends one request (`searchPublicMessagesByTag` for a tag,
`searchPublicPosts` otherwise) and hides the chat/content/date groups, which
have no meaning there.

UI (`src/ui/search_ui.rs`, `src/ui/privacy.rs`):

- Frequent-contacts strip with a right-click row (Remove from Recent /
  Remove all & Disable / Cancel); inline confirmations for Clear and Hide.
- A remove button on every Recent row (optimistic, `removeRecentlyFoundChat`).
- Search tabs "This chat / My messages / Public posts" and a "From archive"
  chip in the filter bar. In the in-chat search bar a hashtag query shows the
  same tabs, so a hashtag click can move to My messages or Public posts.
- Privacy overlay: "Suggest frequent contacts" switch.

Already present before this batch and now declared: the in-chat "From:"
member picker, jump-to-date calendar and "N of M" counter
(`src/state/search_types.rs::position_label`, `src/connect/tests/find_in_history.rs`).

## Not done / deviations

- The "Apps" tab (popular mini apps) is separate (`bots-apps-tab`), so
  `chatlist-search-tabs` is not declared.
- The frequent-contact menu opens on right-click only (no long-press / menu
  key yet); "Remove all & Disable" and the Privacy switch are the keyboard path.
- "Show all" / "Collapse" of the strip is not implemented; the strip scrolls
  horizontally instead.
- No pagination of public posts (first page, `SEARCH_LIMIT`).
- Frequent contacts cover people only; `disable_top_chats` is assumed to be
  the TDLib option name (not listed in the 1.8.67 schema comments) and could
  not be checked against a live account.

## Verification

- Unit tests: request shapes (`tests_chats.rs`), reducer
  (`src/state/tests/search_upgrades.rs`), driver
  (`src/connect/tests/search_upgrades.rs`), filters (`search_filters.rs`).
- Demo captures (English fixtures, no live account):
  `--screenshot-demo ready-search-frequent` and `ready-search-public`.
- `quill-tools/gate.sh`: GATE OK.
