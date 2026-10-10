# Chat header, bottom bars, window title, autoscroll

Parity ids: chrome-header-status, chrome-header-chips, chrome-window-title, chrome-composer-states, chrome-middle-click-scroll.
Not claimed: chrome-header-menu, chrome-discuss-buttons.

## What Telegram Desktop does

- Header (`history_view_top_bar_widget.cpp`): `PeerBadge::drawGetWidth` runs with `bothVerifyAndStatus`, so a Premium emoji status and the verified check sit side by side. SCAM and FAKE draw as a bordered label and replace the other badges. tdesktop has no "Restricted" label in the header.
- Window title (`MainWindow::updateTitle`): `Telegram` plus the unread total, or the open chat's name with its own unread count in front and the total after a dash. With several accounts it adds `@ Account`.
- Bottom bar (`HistoryWidget::updateControlsVisibility`): one full-width button replaces the composer. The order is blocked (Unblock, or Restart for a bot), not joined (Join channel, Join group, Apply to join group), channel mute/unmute, bot Start. The mute bar also carries a gift icon and a direct-messages icon.
- Autoscroll (`ui/widgets/middle_click_autoscroll.cpp`): a middle press anchors a point. Speed is 30 px/s per pixel beyond a 6 px dead zone, capped at 7200 px/s. A press shorter than 220 ms toggles the mode, a longer one scrolls while held.

## What changed

- `peer_badge::header_badges` and `Session::chat_header_badges` feed badges after the header title. Chat rows keep their one-badge rule.
- `quill::window_title` builds the title and `ui/window_chrome.rs` hands it to `Window::set_window_title` when it changes. That call exists in GPUI on macOS, Linux and Windows. The app name replaces "Telegram". Multi-account naming is not wired, so it never prints `@ Account`.
- `quill::chat_bottom_bar` decides the bar. `ui/bottom_action.rs` draws Unblock/Restart, Start, Join group and Apply to join group in place of the composer. Unblock needs no confirmation, as in tdesktop; Restart unblocks and then starts the bot. The channel footer reuses the same labels and gains a Discuss button when the channel has a linked group. The bot card no longer has its own START button.
- `quill::autoscroll` holds the speed math. `ui/autoscroll_ui.rs` tracks the anchor, draws a ring with two arrows, and feeds wheel events to the history list, because the message scroller exposes no pixel offset. The logical pointer visits the anchor for each event so the list's hover test passes, then returns.

## Not done

- Direct messages icon: `supergroupFullInfo.direct_messages_chat_id` is not parsed, and carrying it through the positional full-info plumbing is a separate change. This is why chrome-discuss-buttons stays open.
- Header menu: Statistics, Boosts, Create poll, Set wallpaper and Open in new window are still missing, so chrome-header-menu stays open.
- Restricted chip: tdesktop does not draw one in the header, so nothing was added.
- Escape does not end autoscroll. Any left or right click, a chat change or losing window focus does.

## How it was verified

- Unit tests cover badge ordering, the title format, the bar precedence and the autoscroll math.
- `--screenshot-demo ready-chat-header` with `QUILL_DEMO_HEADER` set to verified, fake, scam, unblock, restart, start, join-group, apply, channel and autoscroll. I looked at the captures: verified check, SCAM label, Unblock, Start, Apply to join group, and Mute with Discuss render as intended. The fake, restart and join-group captures were produced but not inspected.
- `QUILL_TRACE_TITLE=1` printed the title the window reports, for example `Dana Levi – (1)`.
- Autoscroll: a scripted middle click (`mid:x,y` in `QUILL_DEMO_CLICK`) at the anchor, then a pointer move above it, scrolled an 80 message history from the bottom to the top in 1.5 s, with the ring drawn at the anchor. A real mouse and the hold-to-scroll path were not tried.
- Not run on Linux or Windows.
