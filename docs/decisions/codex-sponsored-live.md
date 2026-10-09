# Sponsored messages in live channel history

Telegram API Terms 3.3 require third-party clients to show sponsored messages in channels. Quill fetched them (`getChatSponsoredMessages` on channel open) but only drew them in the `ready-sponsored` demo fixture, which swapped the whole history pane for ad rows. This change renders them in live history.

## What tdesktop does

- `SponsoredMessages::request` (data/components/sponsored_messages.cpp): channels and bots only, skipped while an ad is displayed and for five minutes after the last response (`kRequestTimeLimit`).
- `inject`: the first ad is appended after the last message; further ads are spliced in after `messages_between` messages. Ads are `isSponsored` items inside the history list.
- `history_view_list_widget.cpp` / `Element::markSponsoredViewed`: the view is registered while painting, once the bottom part of the ad (above its button) is inside the visible area; `SponsoredMessages::view` sends it once and rate-limits repeats.
- The ad carries an "Ad" label (`lng_sponsored_message_title`) and a menu: About these ads, Report ad, Hide (Premium; non-Premium sees the Premium preview).
- Clicking the ad/button calls `clicked`.

## What changed

- `Session` (state, reducer side): `ChatSponsoredMessages` now tracks `viewed`, `dismissed` and `fetched_at`; `open_sponsored_tail` picks the ad to show (first not dismissed, content media downloaded, history window at the latest message, not in a topic view, not after ads were hidden); `take_sponsored_views` marks ids viewed exactly once; `sponsored_fetch_due` implements the five-minute window; a `Ok` report dismisses that ad and `AdsHidden` sets `sponsored_hidden`.
- `ConnectDriver::view_sponsored_messages` sends the view. TDLib 1.8.67 has no `viewSponsoredMessage`: `schema/td_api.tl` documents sponsored messages as viewed through `viewMessages` ("only when the entire text of the message is shown on the screen (excluding the button)"), so it sends `viewMessages` with the sponsored id, `messageSourceChatHistory`, `force_read=false`, new purpose `ViewSponsoredMessages` (fire-and-forget). Failed sends release the id for a retry. Hide ads follows tdesktop's `HideSponsoredClickHandler`: for a Premium account `hide_sponsored_messages` sends `toggleHasSponsoredMessagesEnabled(false)` (`schema/td_api.tl:14854`) and the `ok` hides every ad for the session; for a non-Premium account the session records the "needs Telegram Premium" notice and no request is sent. Reporting follows TDLib: `reportChatSponsoredMessage` with an empty `option_id`, show the options of `reportSponsoredResultOptionRequired`, resend with the chosen option's id (opaque bytes echoed back, never invented).
- UI: `QuillApp::sponsored_footer` draws the ad card under the history list only while `!is_scrolled_up()` (and a report picker / outcome banner when active). It records the ids it painted, and `report_visible_sponsored` (frame start, next to `report_visible_history`, window-active only) forwards them to the driver. The card shows an "Ad" chip ("Recommended" when TDLib flags it), the title, content, sponsor info, the sponsor button (`clickChatSponsoredMessage`; media clicks already did this) and an ellipsis menu: About this ad, Report ad (when `can_be_reported`), Hide ads (Premium).
- The demo-only `sponsored_demo` pane swap is gone; the `ready-sponsored` fixture now shows channel posts with the live footer.

## Differences from tdesktop

- Only the first ad is shown, at the tail; `messages_between` mid-history injection is not done (the footer sits under the list rather than inside the scrolling flow, which avoids touching the virtualized row model).
- "About" is an inline panel instead of a box; the Premium upsell for non-Premium accounts is TDLib's `PremiumRequired` outcome banner.

## Verification

- Tests: `tests/replay_sponsored_live.rs` (fetch, store, tail selection, view once, retry after failed send, refetch window, report and hide), `src/connect/tests/sponsored.rs` (driver sends one `viewMessages` with `[9001]`, one `clickChatSponsoredMessage`, no refetch inside five minutes).
- Visual: `QUILL_DEMO_CAPTURE=... quill --screenshot-demo ready-sponsored <dir>` shows three channel posts and the ad card (Ad chip, title, text, sponsor lines, Shop now button, ellipsis menu) under them.
- Not verified: against a live account (no live actions taken); hide ads for a Premium account against real TDLib; the dropdown menu and click paths (static capture only).

## Clippy under --features ui

`cargo clippy --features ui --all-targets -- -D warnings` failed on main (`src/tray.rs` clone on a Copy type, `src/tray_mac.rs` indexed loop) and, once those compiled, on about 170 further UI-binary lints. Separate commit: machine-applicable `cargo clippy --fix` changes, the two named spots, hand fixes for the rest, and a UI-wide allow for `too_many_arguments`, `type_complexity` and `unnecessary_unwrap` (GPUI builders; `live` checks) in `src/ui/mod.rs`. Behavior is unchanged; the gate (core and UI tests) passes and the UI clippy command finishes clean.
