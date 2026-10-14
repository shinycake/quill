# Revenue polish

Follow-up to `revenue-and-affiliates.md`. That PR left `admin-monetization` and `bots-earn-affiliate` unticked because of the differences it listed. This one closes them, checked against tdesktop (`info/info_layer_widget.cpp`, `mainwidget.cpp` `showNewSection`, `info/channel_statistics/earn/*`, `info/bot/starref/*`, `boxes/passcode_box.cpp`, `boxes/peers/replace_boost_box.cpp`, `statistics/*`, `api/api_earn.cpp`).

## What tdesktop does, and what changed

- **Where the pages open.** The plan said "full-width section". tdesktop does something else: `showSection(Info::ChannelEarn::Make(...))` asks `Info::Memento::createLayer`, which returns an `Info::LayerWidget` whenever the window is at least `infoMinimalWidth + 2 * infoMinimalLayerMargin` wide. So Monetization, Stars Balance and both affiliate pages open as a centered card over a dimmed window: `min(infoDesiredWidth = 392, width - 32)` wide, top inset `clamp(height / 24, 20, 40)`, a 56 px top bar with the back arrow, the title and the close button. Pages opened from it stack inside it. Quill now does exactly that (`src/ui/earn/layer.rs`): the layer replaces the 300 px info panel, Escape goes back a page or closes (tdesktop `WrapWidget::keyPressEvent`), a click outside closes, and the info panel it covered comes back when it closes. It fades in and out on the frame clock and snaps with "Interface animations" off.
- **Setup page bottom button.** tdesktop `setupBottom` pins the Start / Update button and the terms under the scroll (`setScrollBottomSkip`). Now pinned the same way.
- **Range slider under each chart.** The earn charts are tdesktop's statistics `ChartWidget` with `currencyRate` set. They now use Quill's shared kit chart (`ui/stat_chart`, gpui-kit `AreaChart` and the stacked columns from #761), which has tdesktop's footer as a kit range slider over a small preview. The shared chart learned the currency: values scale in TON (not nanotons) and the tooltip reads "Rewards in TON" / "Rewards in Stars" and "Rewards in USD" like `PointDetailsWidget`. Charts without a currency are unchanged.
- **"Forgot password?" in the withdrawal box.** tdesktop's box is `PasscodeBox` in check mode: the field ("Enter current password"), the error or "Hint: …" under it, "Forgot password?", then the description. Same now. "Forgot password?" follows `recoverByEmail`: with a recovery email it asks for the code at once and opens the two-step verification recovery page; without one that page offers the reset. The cloud password state is fetched when Collect or Withdraw is pressed (tdesktop reloads it when the button is set up).
- **"Learn more" placement.** tdesktop puts the about text with its inline "Learn more ›" in a `DividerLabel`: a full-width band. The section breaks (`AddDivider`) and the `AddDividerText` notes (Stars balance, Switch Off Ads) are bands too. Same now.
- **Failure toasts.** tdesktop toasts `"Failed: " + error` for the affiliate calls (`UpdateProgram`, `Revoke`, `ConnectStarRef`) and the bare error for revenue loads and withdrawals. Quill now uses the same shapes. Quill never stores TDLib's raw error message (it can hold private data, see `TdError`), so the text after "Failed: " is the error class ("invalid request", "not found", …), not the server's code.
- **Userpic transfer in the join box.** `CreateUserpicsTransfer` with `StarRefJoin`: 60 px photos 42 px apart with the arrow between them, and the `starrefJoinIcon` star on a Premium-gradient disc ringed in the window color at the recipient's corner. Same now.
- **Animated referral link icon.** tdesktop plays its `starref_link.tgs` once when the box opens. That file is part of tdesktop's GPL resources and Quill is MIT, so it is not bundled. The icon plays Quill's own one-time motion instead (a quarter-turn swing-in that settles with a small overshoot over 700 ms, on the frame clock, off with "Interface animations" off).
- **Program list paging.** tdesktop's `PeerListContent` loads the next page when the end is within three screens. Same now (was 200 px).

## Secondary text contrast

Idan flagged the gray text. Quill's own grays already pass WCAG AA. Palette themes did not: Day/Classic `windowSubTextFg` is #999999, 2.8:1 on white. `theme_colorize::readable` moves a color's lightness away from the background only until it reaches 4.5:1, keeping its hue. It now holds `text_muted()` and the kit's `muted_foreground` (axis labels, kit secondary text) to AA on an applied palette's window background, and the earn bands' text to AA on the band. Colors that already pass are untouched, so Quill's default look does not change.

## Left as is

- Bot balances are requested only for bots the user owns; the rows shown are the same.
- The charts keep one value axis with the dollars in the tooltip (owner request in the first PR; the dataviz rules forbid a dual axis).
- Going back to an earlier earn page shows what it loaded (tdesktop restores the page's saved state the same way).

## Verified

- Unit tests: layer geometry, `readable`, the three-screen preload, the driver test for the "Failed:" toast.
- GPUI tests in `src/ui/earn/tests.rs` (the bot chart's plot and range slider among them).
- Demo captures of `ready-earn-monetization`, `ready-earn-bot-stars` and the setup page, light and dark, before and after (linked from the PR).

No real withdrawal, payment or program change was made; everything ran on the demo sessions and the recording sender.
