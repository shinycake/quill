# A chat switch leaves other windows' stickers and the chat list playing

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
  (`UiClip::places`, `Head::places`; by window and part since the
  follow-up below): the window of every view that asks for a clip
  (`Player::prepare`) or draws it (`Player::frame`).
- `Player::leave(window, what)`: the window let go of its chat. It forgets
  the heads only that window drew and closes the clips only that window
  asked for; a clip or head another window uses stays as it is.
- `QuillApp::leave_sticker_chat(window, chat)` runs it and forgets the
  chat's lone emoji plays (`EmojiPlays::leave_chat`), unless another
  window still shows the chat (`chat_shown_elsewhere`). Plays are kept by
  chat, and one chat can show in two windows: the main window opens a
  comment thread in its discussion group while the group has a window of
  its own (`select_chat_with` with `thread_switch` skips the redirect).
  Closing the group's window then left the main window's lone emoji of
  that chat unplayed, so they played again. The main window
  calls it when it opens another chat (`select_chat_with`) or its chat
  moves to a window (`leave_open_chat`); a chat window calls it as it
  closes (`forget_chat_window`). The main window's closed frames are
  retired, since its last frame still shows them until it draws again; a
  closed chat window's are freed at once (`drop_frames`), since nothing
  shows them.
- `stop_sticker_playback` (everything, every window) remains for another
  account, the loop setting changing, and the app going.

In the main window, what only it used closes and forgets at once, as
before, so memory is the same and its new chat shows no frame of the old
one (one exception under "Not changed"). Clips are still closed at the
switch rather than left to the
5 s closer (`sticker-clip-memory.md`): flicking through chats would
otherwise keep each chat's clips, raw frames and canvases for 5 s more.

`StickerView` moved to `sticker_player/view.rs` unchanged, to keep
`sticker_player/mod.rs` under the 1000-line limit.

## Not changed

- The loop setting still restarts every window's stickers (their mode
  changed).
- Heads keyed by file alone are shared by every window:
  `("emoji-image", file)` (custom emoji in the composer, the emoji bar,
  panels; `custom_emoji_image`, `custom_emoji_image_parked`) and
  `("panel-sticker", file)` (`sticker_image`). A main-window chat switch
  forgot them before; now it keeps one a chat window drew too. With
  looping off, such an emoji in the main window that also shows in a chat
  window no longer plays again after the switch. Accepted: playing it
  again would restart it in the chat window.
- Kit dialogs hosted by a chat window are built outside its scope (the
  kit's dialog layer renders from `Root`, `third_party/gpui-component/src/root.rs`;
  `QuillShell::sync_kit_dialogs_in` opens them with a builder that sets no
  scope), so a player view in such a dialog would count as the main
  window's, and a main-window chat switch would close its clip and forget
  its head. No dialog animates stickers today. Setting the window's scope
  around the builder would change what every dialog hosted in a chat
  window builds from (today the main window's state), so it waits for a
  dialog that animates.

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
  through a real chat switch): tdesktop's behavior for the chat left. The
  lone emoji plays 500 ms (on the player's clock, which the test stops
  with `Player::freeze` and moves itself), the chat is left, its clips
  close at once, and opened again it plays from 0. Both pass on main and
  here.
- `animated_emoji::tests::a_chat_windows_emoji_close_with_it` now also
  checks that the closed window's chat forgets its lone emoji's head and
  played mark (fails on main), and that its clips close and their frames
  leave memory as it closes, with no wait.
- `animated_emoji::tests::closing_a_chat_window_keeps_the_plays_of_a_chat_the_main_window_shows`:
  the demo chat in its own window and in the main window (as a comment
  thread would be); closing the window keeps the lone emoji's played mark.
  Fails without `chat_shown_elsewhere`.
- `animated_emoji::tests::closing_a_chat_window_keeps_the_clips_the_main_window_shares`:
  the same two windows; closing the chat window keeps the clips both drew
  open, and the frame the main window shows stays in its atlas. Fails when
  `leave` closes every clip the closing window used.
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

## Follow-up: the chat list keeps its emoji on a chat switch

### What was wrong

`leave` worked by window, and the chat list is in the main window, so a
main-window chat switch also forgot the chat list's heads
(`("sidebar-emoji", id)` for custom emoji in previews and topic icons,
`("status", ListRow, chat, id)` for emoji statuses) and closed their clips.
On every switch each animated emoji there started over from its first
frame, even one at rest (looping off, or a status after its two loops).
tdesktop's chat list rows keep their emoji when another chat opens.

The same closing explains the rare fallback. Whatever reaches the closed
clip first opens it again: the chat list's animation layer painting a
row's old view (`Player::frame`), or the row built again
(`Player::prepare`). A row built before the reopened clip's first frame
is in has no view (`prepare` returns false), so it shows its fallback: the
star for a status (`title_badge_element`, no still), the emoji's alt text
in a preview. It lasts until the first frame lands and the pump redraws:
1 to 9 frames, while the new chat's stickers open on the same workers.
When the frame is in first, the row shows the emoji from frame 0
instead. The test below caught both on main right after a switch: the
preview's heads restarted at 0, the status's head gone and its reopened
clip without a frame.

### What changed

- A head belongs to a part of its window (`HeadId::of(part, name)`,
  `sticker_player::Part`): `Chat` (the window's open chat: history rows,
  dice, custom emoji in messages, lone emoji, effects, the header's and a
  sender's status, the profile column, which follows the open chat) or
  `Chrome` (the rest: the chat list, the composer and its emoji bar,
  panels, the send-effect preview, the story viewer). Each place that
  makes a head names its part.
- The player notes where each clip and head is used as a window and part
  (`Place`). A main-window chat switch (or its chat moving to a window)
  lets go of that window's `Chat` part only (`Leaving::Chat`); a chat
  window closing lets go of all of it (`Leaving::Window`), as before.
- So the chat list's emoji keep their heads (where they rested) and clips
  (their frames) across a switch: no restart and no frame without a view.
  The left chat still forgets its heads and closes its clips at once, so
  its once-stickers and lone emoji play again when it opens again.

Memory stays bounded: a `Chrome` clip nothing shows any more closes
through the closer 5 s later (`sticker-clip-memory.md`), and a `Chrome`
head goes after `HEAD_TTL` (10 min) or `MAX_HEADS`. What this keeps past a
switch is what the chat list shows anyway; the sticker and emoji panels,
which a switch closes, now close their clips 5 s later instead of at the
switch, as they do when closed without a switch.

Notes:

- A status animates only in the chat list and the main window's
  conversation (`status_glyph`'s `may_animate`); the profile column is
  drawn outside both slices, so its status is a still and its part changes
  nothing today. The header and profile are `Chat` because they show the
  open chat, which tdesktop builds again for the next one, so a status
  there plays its two loops again on return.
- File-keyed `Chrome` heads (`("emoji-image", file)`,
  `("panel-sticker", file)`) now survive a switch in the main window too,
  not only when a chat window drew them. With looping off, a custom emoji
  in the composer of the next chat that rested in the last one's stays at
  rest. Accepted: the same head is the emoji bar's and the panel's.

### Follow-up tests

- `animated_emoji::tests::the_chat_lists_emoji_stay_at_rest_when_the_main_window_opens_a_chat`:
  looping off, a row's emoji status and the demo chat's preview of three
  custom emoji play and rest (on the player's frozen clock), then the main
  window opens another chat. Their heads stay where they rested, the
  status's clip and the preview's clip stay open (the history's bigger
  clips of the same emoji close), the status has a frame at once, and so
  through the next frame and a settle. On main it fails right at the
  switch: `[Some((0ns, 1s)) ×3, None]` against
  `[Some((2.5s, 1s)) ×4]`.
- `sticker_player::tests::a_window_leaving_its_chat_keeps_the_rest_of_it`:
  the main window's chat-only clip closes and its head goes; the chat
  list's clip, a clip both parts draw and a chat window's chrome stay,
  heads where they were; the closer closes them 5 s after nothing shows
  them; a closing chat window lets go of its chrome too.

### Checking on a Mac

Release build with `--features demo-capture`, offline demo, Settings →
Chat → "Loop animated stickers" off, the fixture that gives chat list rows
animated emoji statuses and custom emoji previews. Let them play and rest,
then click through 20 to 50 chats.

- main: each click starts the chat list's emoji over from their first
  frame; now and then a status shows the blue star or a preview the plain
  emoji for a few frames.
- this branch: they stay on the frame they rested on, with no flash; a
  looping one plays on without a jump. Going back to a chat still plays its
  once-stickers and lone emoji again.
- `QUILL_TRACE_STICKERS=1`: the `clips` count still drops at each switch by
  what the chat used; the chat list's clips stay in it while the list shows
  them.
