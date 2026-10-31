# A chat switch leaves other windows' stickers playing

Status: done (Linux tests); the Mac check is for the lead.

## What was wrong

Every window draws stickers and custom emoji through one player
(`QuillApp::playback.stickers`, `sticker-player.md`). When the main window
opened another chat, or its chat moved to a window of its own,
`stop_sticker_playback` closed every clip, forgot every play head and
cleared every lone emoji's "played" mark, in every window. A chat open in
its own window lost all of it whenever the main window switched chats:

- each animated sticker and custom emoji there opened again: its still,
  then the first frame (its clip and frames were gone);
- a sticker playing once (Settings → Chat → "Loop animated stickers" off)
  played again from the start, and so did a lone emoji that had played
  and rested.

Clearing the played set is tdesktop's behavior for the chat that was left
(`stickerClearLoopPlayed` when its `HistoryInner` goes), not for chats other
windows show. Closing a chat window did the opposite: its heads stayed for
up to 10 minutes and its lone emoji's played marks until the main window
next switched chats, so the chat opened again meanwhile did not play its
once-stickers and lone emoji again.

## What changed

- The player notes which windows use each clip and each head
  (`UiClip::windows`, `Head::windows`): the window of every view that asks
  for a clip (`Player::prepare`, which now takes the window) or draws it
  (`Player::frame`).
- `Player::leave(window)`: the window let go of its chat. It forgets the
  heads only that window drew and closes the clips only that window asked
  for; a clip or head another window uses stays as it is.
- `QuillApp::leave_sticker_chat(window, chat)` runs it and forgets the
  chat's lone emoji plays (`EmojiPlays::leave_chat`). The main window
  calls it when it opens another chat (`select_chat_with`) or its chat
  moves to a window (`leave_open_chat`); a chat window calls it as it
  closes (`forget_chat_window`). The main window's closed frames are
  retired, since its last frame still shows them until it draws again; a
  closed chat window's are freed at once (`drop_frames`), since nothing
  shows them.
- `stop_sticker_playback` (everything, every window) remains for another
  account, the loop setting changing, and the app going.

In the main window nothing changes: what it used closes and forgets at
once, as before, so memory is the same and its new chat shows no frame of
the old one. Clips are still closed at the switch rather than left to the
5 s closer (`sticker-clip-memory.md`): flicking through chats would
otherwise keep each chat's clips, raw frames and canvases for 5 s more.

`StickerView` moved to `sticker_player/view.rs` unchanged, to keep
`sticker_player/mod.rs` under the 1000-line limit.

## Not changed

- The main window's own views outside the chat (animated custom emoji and
  emoji statuses in the chat list) still open again on a chat switch, as
  before: the reset is per window, and the chat list is in the main
  window. Sparing them would need heads and clips tagged by chat.
- The loop setting still restarts every window's stickers (their mode
  changed).

## Tests

- `animated_emoji::tests::the_main_window_opening_a_chat_leaves_a_chat_windows_emoji_playing`:
  the demo chat in its own window, the main window opens another chat.
  The window's lone emoji clips keep their first frames and its lone emoji
  stays played, its head where it was or further on. On main it fails:
  both clips closed and opened again (`[None, None]` first frames after
  the switch), and the lone emoji's head was gone and its played mark
  cleared, so it played again from the start.
- `animated_emoji::tests::the_chat_the_main_window_leaves_plays_again_and_closes_its_clips`
  and `a_lone_emoji_plays_once_and_a_tap_plays_it_again` (now leaving
  through a real chat switch): tdesktop's behavior for the chat left, and
  its clips close at once. Both pass on main and here.
- `animated_emoji::tests::a_chat_windows_emoji_close_with_it` now also
  checks that the closed window's chat forgets its lone emoji's head and
  played mark (fails on main).
- `sticker_player::tests::a_window_leaving_its_chat_keeps_what_another_window_uses`:
  clips and heads used by one window or by both, and a clip a chat window
  is still opening. Fails when `leave` stops everything.
- `animated_emoji::tests::leaving_one_chat_keeps_what_played_in_another`
  (core crate).

## Checking on a Mac

Release build with `--features demo-capture`, Settings → Chat → "Loop
animated stickers" off. Open a chat with animated stickers and lone
animated emoji in its own window ("Open in new window"), let them play
and rest, then click other chats in the main window.

- main: on each click the window's stickers and emoji blank to their
  stills and play again from the first frame.
- this branch: they stay on their last frame (lone emoji on their first)
  with no flash. Clicking back to a chat in the main window still plays
  its once-stickers again, as tdesktop does.
- Memory: with `QUILL_TRACE_STICKERS=1` the `clips` count drops at each
  switch on both builds by what the main window's chat used; on this
  branch the chat window's clips stay in it, and leave when that window
  closes.
