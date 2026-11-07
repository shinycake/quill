# Fetch state: one value for loading, data and error

## Problem

Several resources keep `X_loading: bool` and an `X_error` (a bool or an
`Option<String>`) next to the data. The three can be set in combinations that
mean nothing (loading and failed at once, an error with no request) and every
writer has to remember to clear the other two.

## What changed

`crate::state::fetch::Fetch<T>` (`src/state/fetch.rs`):

- `Idle`: never asked.
- `Loading { stale }`: a request is in flight; the last answer stays.
- `Ready(T)`: the latest request answered.
- `Failed { error, stale }`: the latest request failed; the last answer stays.

The transitions are methods (`begin`, `succeed`, `fail`, `fail_dropping_data`,
`cancel`, `clear_error`), so a writer cannot leave a flag behind.

First users, in `SettingsState`:

- `sessions: Fetch<Vec<ParsedSession>>` replaces `sessions: Option<..>`,
  `sessions_loading` and `sessions_error`. A failed terminate or toggle keeps
  the list and shows the error line; starting the next mutation or a fetch
  clears it. `sessions_stale` (refetch trigger) and `sessions_mutating`
  (terminate buttons disabled) are separate facts and stay.
- `blocked_senders: Fetch<Vec<i64>>` replaces `blocked_senders: Option<..>`,
  `blocked_loading` and `blocked_error`. `blocked_total` stays (page count of
  the server's answer).

Also `CallsState::privacy_loading` is gone; it is `privacy_pending > 0`.

One behaviour difference: a mutation error that arrives while a fetch is in
flight now replaces the spinner with the error until that fetch answers. Before,
both flags could show at once.

## Pairs left for later

Plain `loading: bool` (and an error beside it) that could move to `Fetch`:

- Settings: `storage_stats_loading`, `auto_download_presets_loading`,
  `account_ttl_loading`, `connected_websites_loading`, `read_date_loading` /
  `read_date_error`, language `packs_loading`, `language_link.loading`.
- Calls: `recent_calls_loading`, `privacy_error` (the counter stays).
- Payments: `form_loading`, `star_subscriptions_loading`, `gift_code.loading`,
  giveaway `loading` / `premium_loading` / `stars_loading`.
- Chat list: `archive_settings_loading`. Auth: `change_number_loading`,
  `password_state_loading`.
- Stories and stats: `story_insights` `loading` / `forwards_loading`,
  `story_types.loading`, `stats_pages.loading`, `session_links_boosts` `loading`.
- Others: `search_tabs_state.recommended_loading`, `history_calendar.loading`,
  `sticker_gif_types` `loading` / `search_loading`, `info_types.loading`,
  `shared_media_profile.loading`, `web_app_types.grossing_loading`,
  `session_privacy_data.network_loading`.

Each needs its readers checked first: some are list-paging flags where "more
pages" matters as well as loading.
