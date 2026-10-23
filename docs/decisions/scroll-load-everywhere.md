# Scroll-triggered paging everywhere

## What Telegram Desktop does

Long lists never show a "Load more" button. The next page is asked for when
the reader scrolls near the end (or when the first page does not fill the
view), and a spinner row shows while it loads (`PeerListContent` preload,
`Ui::InfiniteRadialAnimation`, "Loading..." text).

## What changed

- `src/ui/scroll_load.rs` is the one helper: `near_end` and `load_due` are
  pure and unit-tested; `scrolled_near_end` reads a `ScrollHandle`;
  `watch` / `watch_scroll` run a check on every wheel event and once after
  each frame (`after_layout`), which covers "first page does not fill the
  view" and "a page arrived and the end is still in view"; `loading_row` is
  the shared kit-`Spinner` row; `Asked` stops an automatic look from
  re-asking at the same list length, so a page that returns nothing new
  cannot start a request loop (a wheel scroll may always retry).
- Load more buttons replaced in: giveaway members, subscriptions, Stars,
  received gifts, Grams history, blocked users, Saved Messages sublists,
  shared-media list tabs, story page posted and archive sections, story
  statistics public shares and public story search hits. The story viewers
  list already paged on scroll on main; it now uses the shared check.
- Hand-rolled near-end checks migrated (emoji sets, group members, sticker
  manager, earn programs, inline members, search tab results, story album
  picker); their `LOAD_MORE_DISTANCE`-style constants are gone.
- Dialogs that scroll their body use `scrollable_dialog_content_tracked` /
  `finish_tracked` to hand the scroll handle to the owner.
- Failed pages are retried by scrolling again, not on their own.
- Lang keys no longer used were removed.

## Left alone

- `src/ui/profile_panels/` and `src/ui/shared_media_profile.rs` (another
  thread): common groups, the profile grids and music tracks still have
  buttons.
- `ton_wallet_money.rs` (wallet is blocked by owner rule) and
  `calls/call_again.rs` (not in the brief).
- `event_log.rs` already pages on scroll to the list top (reverse list), a
  different check.

## How verified

Demo captures in light and dark with the next page on its way: Grams
history (`QUILL_DEMO_GRAMS=loading-more`), blocked users
(`ready-settings-privacy-blocked`) and the shared-media Files list
(`QUILL_DEMO_SHARED_MEDIA_LOADING_MORE=1`). The captures cannot show a real
page arriving from TDLib.
