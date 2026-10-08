# Audio player bar, playlist and OS media keys (gap-audit batch 14)

Reference: tdesktop `media/player/media_player_widget.cpp`,
`media_player_instance.cpp`, `platform/*` media controls.

## What shipped

- `src/ui/player_bar.rs`: a bar above the conversation (above the call bar
  and the chat's own bars) while a voice note or music file is active, even
  paused. Play/pause, previous/next (music), title and performer (voice:
  sender name), progress slider, time, speed, mute, repeat (off/one/all),
  order (in order/reverse/shuffle), close. Clicking the title opens the
  track's chat and jumps to the message.
- Playback now survives chat switches (`select_listed_chat` no longer stops
  it). Message ids repeat between chats, so "is this row the active one" and
  the play-toggle also compare the track's chat (`PlayerBarState::chat`).
  `toggle_audio_playback` and `pending_audio_play` therefore carry a chat id.
- `src/playlist.rs` (pure, unit tested): next/previous for music with repeat
  and order modes, shuffle rounds with back-walk, and the voice chain.
- Voice notes continue to the next newer voice note when it is unread and
  incoming; each started note is marked listened by the existing toggle.
- Music playlist = the chat's audio messages in the loaded history window,
  oldest first ("in order"). Auto-advance at the end honours repeat; a manual
  next/previous ignores repeat-one.
- `src/media_session/`: one seam, three backends. macOS
  `MPRemoteCommandCenter` + `MPNowPlayingInfoCenter` (objc2-media-player);
  Linux MPRIS on the session bus via zbus on a worker thread; Windows
  `SystemMediaTransportControls` from a `MediaPlayer` with its command
  manager disabled (no HWND needed), on a worker thread. Backends only queue
  commands; the 250 ms playback tick drains them and publishes state
  (deduplicated; seeks detected by position drift).
- Icons added to the scoped bundle in `main.rs`.

## Deviations / limits

- Round video messages are not part of the voice chain: they play through
  the video path, not the audio engine. The chain skips over only by
  stopping at them (the next-unread rule ends at any non-voice message).
- The playlist uses the loaded history window, not a Shared Media audio
  search, so very old tracks outside the window are not reachable until
  scrolled in.
- Volume is a mute toggle (existing shared volume), no hover slider.
- Media keys only reach the app while a track is active (the tick runs
  then); there is no "resume last track" from a cold OS Play.
- Linux MPRIS and Windows SMTC were compile-checked for their targets in
  isolation but not run; macOS was built and the bar captured light/dark
  (`ready-player-bar` demo).
