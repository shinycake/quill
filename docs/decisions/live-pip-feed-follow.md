# Live PiP follows a feed change

PR A of `quill-tools/designs/live-gaps.md`
(`parity:calls-live-pip-feed-follow`). It closes the deviation #883 left
open in `video-player-live-pip.md`: "The PiP's player does not follow a
feed change."

## Before

- A stream's feed is keyed by chat (`LiveKey::Stream { chat_id }`). A
  rejoin drains the feeder (`close_stream_feed`: the feed ends as
  `LiveEnd::Left` and goes) and the new connect opens a new one
  (`open_stream_feed`). A restart replaces the feed in one step and ends
  the old one as left.
- The stream window and the live story viewer each compared their
  player's feed with the current one on every draw and built a new
  player. Each had its own copy of that code.
- The live PiP got one fixed player when it opened. Any end of that
  player's feed, `Left` included, put the player in `Ended`, and the PiP
  window closed itself on `Ended`. A rejoin or a restart therefore closed
  the PiP, and a live story's presenter switch did the same.
- `live_pip_source_alive` needed `is_joined`. TDLib clears `is_joined`
  and sets `need_rejoin` when the network drops the call, so a
  server-driven rejoin also closed the PiP.

## After

- `video_player/live_follow.rs` holds the rule: `shows_feed`,
  `follow_live_feed` (a new player when the current one does not show
  the feed) and `follow_live_slot` (a host's slot follows the feed; no
  feed, no player; the old one is released). The stream window
  (`stream_picture_parts`) and the story viewer (`live_story_player`)
  use `follow_live_slot`. The PiP uses `follow_live_feed`.
- `PipWindow::replace_player(player, cx)` swaps the player in the open
  window and returns the old one. The window, its place and size, the
  pointer state and the controls' fade stay. Until the new player has a
  picture, the window keeps the old picture's aspect. If the window is
  gone or has already let go of its player, nothing changes and it
  returns `None`.
- The PiP view decides what an ended player means (`EndCheck` in
  `controls/pip/live.rs`):
  - `Failed`, for a file or a live picture: close, as before.
  - A live feed ended as `Over` (the stream is over): close, as before.
  - A live feed ended as `Left`: the window stays and shows the
    connecting state. That is the same spinner on a dark disc that the
    PiP shows while a player opens or stalls, after the same 500 ms
    delay, over the old last picture. The view tells the owner once per
    player through the new `LivePip::feed_left` hook (deferred).
- `check_live_pip` (`calls/live_pip.rs`) calls `follow_live_pip_feed`
  (`calls/live_pip_follow.rs`) on every sync, after the closed, lock and
  alive checks. It resolves the source's feed:
  - `Stream { call_id }`: the stream feed of the call's chat (the
    driver's, or the demo fixture's), through the same `stream_sink` the
    stream window uses.
  - `Story { chat_id, story_id }`: the watched story's feed (its stream,
    or the presenter's camera or screen), through `live_story_sink`, the
    viewer's own lookup.

  If the PiP's player does not show that feed, a player on the feed
  (or the slot's, if it already follows) goes into the window. The old
  one is released and the new one is stored in
  `group_call.stream.player` or `stories.live.player`. Enlarge then hands
  that same player to the stream window or the viewer, which keep it
  because it already shows their feed. With no feed yet (the rejoin's
  leave came before its connect) nothing changes, and the window keeps
  showing the connecting state.
- `live_pip_source_alive` for a stream now counts a call that is being
  rejoined on its own (`is_joined || rejoining()`).
  `ActiveGroupCall::rejoining` is `need_rejoin || reconnecting` while
  attempts are left (`rejoin_attempts < MAX_GROUP_CALL_REJOIN_ATTEMPTS`,
  the new name for the `3` that `join.rs` and `calls/error.rs` used), or
  while the last attempt's answer is pending (`reconnecting` cleared).
  When the last attempt fails, `calls/error.rs` sets `reconnecting` again
  with no attempts left, only the stream window's manual Rejoin remains,
  and the PiP closes as it does for a left call. Without that bound,
  TDLib's `need_rejoin` stays set and the PiP would show its spinner with
  no way out.
- What bounds the alive state for a stream: the attempt limit above,
  leaving the call (`left_group_call`), the stream ending
  (`StreamState::Ended`), the call going from the session, lock, and
  logout or an account switch. For a story: the story no longer being
  watched, or its stream ending.

## Deviations from the design, with reasons

- **Order in `check_live_pip`.** The design resolves the feed before the
  closed check. Here it runs after the closed, lock and alive checks, so
  no player is built for a window that is about to close. The order of
  everything else is unchanged.
- **The story viewer needed no new behaviour.** `live_story_player`
  already followed its feed, and the viewer does not close on a player's
  end (it closes on `StreamState::Ended` or when the call goes, in
  `check_live_story_ended`). Point 4 is done by moving it onto the shared
  helper, and a test covers the swap.
- **A call being rejoined counts as alive, while attempts are left.**
  The design said the PiP closes only when the source is gone. Without
  this, the rejoin case the design names would still close the PiP on
  TDLib's update, before any feed change. An edge: if a TDLib update
  with `need_rejoin` arrives while the last attempt is in flight, it sets
  `reconnecting` again and the PiP closes one attempt early.
- **A swap into a window that went.** `follow_live_pip_feed` builds the
  new player before `replace_player` can find the window gone; it then
  releases that player and leaves the slot as it was. A host's
  `follow_live_slot` may also release the PiP's current player first in
  a race; `release_live` is idempotent, so that is harmless.
- **Connecting state.** The PiP has no placeholder photo or
  "Connecting…" label. Its connecting state is its existing waiting
  spinner over the last picture, which is what the stream window's
  opening state reduces to at PiP size.
- **`LiveStoryUi::demo_feed`** is now `pub(in crate::ui)`, like its
  `player`, so the video player tests can swap the story's feed.

## Measurements

This is not performance work, and nothing was measured. The added cost
is one feed lookup per sync while a live PiP is open: a scan of the
chats for the call's chat, plus one mutex lock in the driver's feed map.
It runs at the main window's root render and after updates land. No live
PiP means no cost. A swap builds one player, the same work the stream
window does on a rejoin.

## Verified

- GPUI tests in `video_player/tests/live_pip_follow.rs`
  (`#[cfg(test)]` over `#[cfg(feature = "demo-capture")]`, hand-fed
  feeds, the spinner's delay on the test executor's clock via
  `advance_clock`, no sleeps):
  - a rejoin: the feed is left with nothing new yet, the PiP stays with
    the old player, and after 1 s it shows the spinner and the badge. A
    new feed then swaps the player in the same window, the slot holds the
    new player, the badge stays and the spinner goes;
  - a restart with no gap swaps without closing;
  - a stream that really ends (`StreamState::Ended`, feed `Over`) still
    closes the PiP;
  - a left feed with the call left closes the PiP;
  - while rejoins are left (one due, then the last one in flight) the
    PiP waits; once the last one failed it closes and the call stays for
    the manual Rejoin;
  - enlarge after a swap reopens the stream window with the swapped
    player, and the PiP lets go;
  - the live story viewer survives a feed swap: it stays open, watches
    the same story and plays a new player on the new feed;
  - a story PiP follows a feed swap in the same window, keeps the story
    joined, and enlarges back to the viewer with the swapped player.
- With the `Left` rule turned off, the rejoin test fails (the PiP
  closes). The existing `live_pip.rs` tests still pass.
- Captures, not committed, in
  `/Volumes/Idan1TB/quill-work/scratch/live-pip-follow-captures/`, from
  `ready-rtmp-pip` with a temporary, uncommitted swap 400 ms after the
  PiP opened:
  - `after-swap.png.window1.png`: the PiP plays the new feed (a
    different picture) in the same window with LIVE, the viewers and the
    controls;
  - `gap.png.window1.png`: the old feed left and no new one, the old
    picture under the spinner, with the badge.

## Not verified

- No real stream, rejoin or live story ran. Feeds were the demo
  fixture's and hand-fed sinks, so the driver's real order of
  `close_stream_feed`, TDLib's `need_rejoin` update and
  `open_stream_feed` is covered only by reading the code. Someone should
  check a real rejoin: join a public channel's stream, open the PiP and
  drop the network for a few seconds.
- The window code itself did not change, so placement and keep-alive on
  Windows and Linux are as #849 and #883 left them.
- The README line for this item still describes the old behaviour. A
  feature PR does not edit the README; the pipeline ticks it from the
  fragment and can reword it.
