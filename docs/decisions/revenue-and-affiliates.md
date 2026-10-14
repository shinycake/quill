# Revenue and affiliate programs

Three parity items: `admin-monetization`, `bots-earn-affiliate` and the missing pieces of `auth-debug-toggles`. Each was checked against tdesktop (`info/channel_statistics/earn/*`, `info/bot/earn/*`, `info/bot/starref/*`, `settings/settings_credits*`, `intro/intro_widget.cpp`, `settings/settings_codes.cpp`) and TDLib 1.8.68 first.

## Channel Monetization (`admin-monetization`)

tdesktop shows a "Monetization" entry in a channel's top-bar menu, after Statistics, to admins who can view revenue. It opens a section with the TON part (ad impressions, ad rewards, available/last month/total, Collect, Switch Off Ads) and the Stars part (Stars revenue chart, balance, Withdraw, Buy Ads), plus a TON/Stars history.

Now:
- The menu entry is in `chat_header_menu.rs` and opens an `InfoPanelTarget::Earn` page.
- Core in `src/earn/*`; the session reducer is `state/domains/payments/earn.rs`; driver calls are in `connect/earn.rs`. The requests are `getChatRevenueStatistics`, `getChatRevenueTransactions`, `getStarRevenueStatistics`, `getStarTransactions` (all/incoming/outgoing), `getChatRevenueWithdrawalUrl`, `getStarWithdrawalUrl` and `getStarAdAccountUrl`.
- Limits come from TDLib options: `star_withdrawal_count_min` and `_max`, `can_withdraw_chat_revenue`, `affiliate_program_commission_per_mille_min` and `_max`. The affiliate switches come from the app config (`starref_connect_allowed`, `starref_program_allowed`). A failed config read marks it loaded so the page never waits forever.
- Charts: the same three graphs and periods as tdesktop (ad impressions, ad rewards in TON, Stars revenue; the bot page shows its Stars revenue). They are drawn with gpui-kit's charts, not the hand-drawn statistics chart: `LineChart` with `step_after` for impressions (tdesktop's step line) and `BarChart` for the revenue graphs (tdesktop's column chart), in the theme's `chart_2` color, with the kit's grid, axes and hover tooltip (see `src/ui/earn/chart.rs`). The graph's `yTickFormatter` picks the currency. The value axis reads in TON or Stars with short three-digit labels, the line's axis tops out at a round number, and the tooltip shows the date, the amount in the currency and its USD value.
- USD decimals follow tdesktop's `ToUsd`: 2 by default. The channel page uses 0 after the point for Stars, the bot page 3 (`kMinorLength`), and TON uses 3.
- Withdrawal only opens the Fragment URL TDLib returns. No funds move inside Quill. The 2FA step uses tdesktop's order: "check password" box, then the password field. The password is sent once and the field is emptied.
- Learn box with the diamond easter egg, TON transaction box (with "View in Blockchain Explorer"), Stars receipt, and the boost box that a locked "Switch Off Ads" opens.
- Also wired in: the channel profile's balance row, the "Affiliate programs" row in channel settings, and the Stars dialog's "View statistics" and "Earn Stars" buttons.

## Bot Stars and affiliate programs (`bots-earn-affiliate`)

- Bot Stars revenue reuses the same page (`EarnScreen::Stars`). Bot profiles and the bot edit box show the Toncoin and Stars balances and the Affiliate Program row.
- Setup for own bots (`starref/setup`): Commission and Duration sliders with stops from the TDLib limits, then "View existing programs", "End Affiliate Program", the ConfirmUpdate and ConfirmEnd boxes, and the "Available in" countdown while a change is locked. The request is `setChatAffiliateProgram`.
- Join as a user or channel (`starref/join`): My Programs (`getConnectedAffiliatePrograms`) and the program search sorted by Profitability, Date or Revenue (`searchAffiliatePrograms`). Both page on scroll.
- Joining, the referral link box and Revoke work as in tdesktop. The calls are `getConnectedAffiliateProgram`, `connectAffiliateProgram` and `disconnectAffiliateProgram`. Recipients come from `getOwnedBots` and `getCreatedPublicChats`.

## Test mode on the sign-in screen (`auth-debug-toggles`)

The code shipped earlier. This PR adds the two missing pieces:
- A GPUI test types `testmode` on the real sign-in screen (the phone step) and checks the "Test Mode" label appears, then goes away when typed again. The screenshot demo's sign-in screen now takes the codes. With no connection behind it, `testmode` only flips what the screen shows: no account files are written and nothing connects. The live path is unchanged (tdesktop `Domain::addActivated`).
- Demo kinds `ready-settings-advanced` and `ready-settings-experimental` capture both pages. The Experimental page offers "Import" only when the clipboard holds a valid code, as tdesktop does.

## Charts and secondary text (owner feedback)

The first version reused Quill's statistics chart: the ad rewards were a solid green block with the TON and USD rulers painted over the bars, so the gray ruler text was hard to read. The owner asked for gpui-kit's charts, styled by the Design Guides and the dataviz rules. Before and after captures (light and dark, `ready-earn-monetization` and `ready-earn-bot-stars`) were reviewed locally and are linked from the PR.

- One value axis, labels outside the plot, so no text sits on a bar. The USD amount moved from a second axis into the tooltip (the dataviz rules forbid a dual axis).
- Series use the kit's theme chart color in light and dark. Single-series charts have no legend; the title names the series and the subtitle names the period and the unit.
- Secondary text uses the existing tokens: `text_muted()` (tdesktop `windowSubTextFg`) for subtitles and the kit's `muted_foreground` for axis labels. In Quill's default look they measure about 6:1 (`#59636e` on white) and 4.7:1 (`#737373` on white) in light, and above 6:1 in dark, so they pass WCAG AA. The shared token does not pass when a tdesktop palette theme is applied: Day/Classic `windowSubTextFg` is `#999999`, about 2.8:1 on white. That token is global, so this PR leaves it alone.
- The range footer (tdesktop's minimap slider under each chart) is gone: the kit charts have none. The full period is always shown.

## Differences from tdesktop

- The earn pages live in the 300 px info panel, not a full-width section.
- Charts are gpui-kit charts in the theme color, with one value axis and the USD value in the tooltip, and no range footer (see above).
- The setup page's bottom button scrolls with the page instead of being pinned.
- "Learn more ›" sits inline at the end of the wrapped text.
- The join box's userpic transfer is a plain arrow between avatars.
- The referral link icon is static, not a Lottie animation.
- The program lists load the next page within 200 px of the end.
- Bot balances are requested only for bots the user owns (the visible result is the same).
- Failure toasts read "Couldn't …" in Quill's tone.
- The withdrawal password field has no "Forgot password?" link.
- Going back to an earlier earn page shows the data it already loaded, without fetching again.

## Tests

- Core unit tests: `src/earn/tests.rs`.
- Driver tests with the recording sender: `src/connect/tests/earn.rs`.
- GPUI tests: `src/ui/earn/tests.rs` covers the Monetization page and Collect, the learn box, the link, join, revoke and setup confirm boxes. `src/ui/advanced_ui_tests.rs` covers `testmode`.
- Screenshot demos: `ready-earn-*` and `ready-settings-*`.

No performance work in this PR, so no measurements.
