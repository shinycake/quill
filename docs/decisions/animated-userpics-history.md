# Video userpics beside messages and in the chat list

Parity: `parity:premium-animated-userpics-history`.

## What tdesktop does

- `HistoryInner::validateVideoUserpic` (history_inner_widget.cpp:1741,
  called from the userpic paint at :1881) and the same method of
  `HistoryView::ListWidget` (history_view_list_widget.cpp:3259, 3298)
  play a sender's video photo in place of the userpic beside their
  messages when `peer->isPremium() && !peer->userpicPhotoUnknown() &&
  peer->userpicHasVideo()`. Each widget keeps one
  `Dialogs::Ui::VideoUserpic` per peer in a map and drops the entries it
  did not paint in the last pass. There is no cap on the map.
- `Dialogs::InnerWidget::validateVideoUserpic` (dialogs_inner_widget.cpp
  :1105, :1806) does the same for chat-list rows (main list, archive,
  folders, and the filtered chats of a local search), but not for Saved
  Messages or the Replies chat. Global search results and topic rows keep
  the still.
- The chat header (`TopBarWidget`) and every other small userpic keep the
  still. The profile cover has its own player (Quill: `video_avatar.rs`).
- `VideoUserpic` streams `PhotoSize::Small` (TDLib's `small_animation`)
  and falls back to the large clip, muted, looping, from the start (not the
  main frame). An unknown photo calls `updateFullForced` to learn its
  clips. Frames are taken only while
  `!isGifPausedAtLeastFor(GifPauseReason::Any)`: any layer, the tabbed
  (emoji/sticker/GIF) panel, inline results, a round video playing with
  sound, a media preview, or an inactive window freezes them, and the clip
  reader stops decoding once frames are not shown.
- No power-saving flag gates it: `PowerSaving::kAnimations` is not
  checked, and neither is "Autoplay GIFs".

## What Quill does

- Envelope: `user.profile_photo` keeps its `id` and `has_animation`
  (`ParsedUser.photo_id`, `photo_has_animation`). `ChatPhotoAnimation`
  carries `small_file_id` beside the large clip; both clip files are
  cached with the photo.
- State: `Session::userpic_video(user_id)` is `Still`, `Unknown` or
  `Clip(file)`. `Clip` is the small clip (else the large one) of the full
  info photo (personal, main or public) whose id is the user's current
  photo. `take_userpic_full_info_ask` asks `getUserFullInfo` once per
  (user, photo), so an answer without the photo never loops.
  `ConnectDriver::fetch_userpic_full_info` sends it.
- Pool (`src/ui/userpic_videos.rs`): one `VideoPlayer` per (surface, user),
  for the History and ChatList surfaces, opened muted and looping with
  `Purpose::Inline { max_edge }` at the userpic's size in device pixels
  (the existing inline clip path: AVPlayer on macOS, FFmpeg on Linux and
  Windows, decoding off the UI thread).
  - A slot whose row did not render in its surface's previous pass goes
    (tdesktop's map cleanup).
  - A sweep after each pass, and every 2 s while a player is awake, plays
    what was painted in view, pauses what was not, and puts a player off
    screen for 2 s to sleep (no decoder; `inline_players::off_screen_step`
    is shared). A sleeping slot painted again gets a new player.
  - At most 24 players hold a decoder. Past that the slot off screen
    longest sleeps first; with none off screen the new userpic keeps its
    still.
  - Everything holds still while no Quill window is in front, or the media
    viewer, a story, a dialog, the profile modal, the emoji/sticker/GIF
    panel, inline results or a round video with sound is up
    (`sync_userpic_videos`, tdesktop's `GifPauseReason::Any`).
  - A scale change restarts the players at the new size. A clip that fails
    keeps the still for good.
- Drawing: the userpic's still is drawn and the clip's `video_frame` over
  it, inside an `anim_layer::tile`, so new frames redraw only the
  conversation's or chat list's animation layer, asked for by the frame
  clock. Nothing is drawn with a repeating `with_animation`.
- Where: beside a user's messages wherever the history shows sender
  userpics (`history_sender_avatar`), and on a private chat's row in the
  chat list except Saved Messages (`chat_row_userpic`).

## Deviations

- "Autoplay GIFs" off or data saver on keeps the still, like the profile
  cover. tdesktop plays them regardless. Quill's autoplay switch is the
  user's way to stop clips everywhere, so the userpics follow it.
- The power-saving "animations" switch does not stop them, matching
  tdesktop.
- Quill's search results (its own layout, not the filtered chat list) keep
  the still. tdesktop plays them in filtered chat rows of a local search.
- The 24-decoder cap is Quill's; tdesktop has none.
- On a GPU surface (AVPlayer on macOS, NV12 on Windows) the circle is cut
  by painting the color behind it over the corners. For chat rows that
  color is the row's fill as of its last render; hovering redraws no row,
  so a hovered row's corners keep the resting fill. The presence dot has
  the same limit today.

## Measurements

Method (2026-10-25, macOS 27 on Apple silicon, release build with
`demo-capture`): the `ready-group-video-userpics` demo, a 1200x820 pt
window on a 2x display, `QUILL_ASSUME_ACTIVE=1` so the clips play while
another app has focus. Each run launches the binary, waits 45 s, reads
`phys_footprint` with `footprint -p`, then averages `top` %CPU over 30
one-second samples. Baseline: the same binary and chat with
`QUILL_DEMO_USERPICS=still`, where the users' photos are not videos.
Runs alternate still and video, three of each. Numbers are relative to
that baseline; they are not a comparison with other apps.

| Mode | Footprint (MB, runs 1-3) | CPU % of one core (runs 1-3) |
| --- | --- | --- |
| Still photos | 61, 59, 62 | 0.3, 0.0, 0.0 |
| 11 video userpics playing | 111, 112, 112 | 21.0, 20.1, 21.9 |

- 10 senders beside messages plus one chat-list row play: three demo
  clips at 24 and 30 fps, not in step.
- With the FFmpeg backend (`QUILL_VIDEO_BACKEND=ffmpeg`, the Linux and
  Windows decoder) one run gave 111 MB and 22.5 %. The decoder is not what
  costs: software decoding at 64-92 px costs the same as AVPlayer.
- A `sample` of the video run shows the main thread out of its run loop
  about 30 % of the time, mostly in `Window::draw` and the Metal present.
  So the CPU is the window redrawing at the clips' combined frame rate
  (inferred: two unsynchronized rates give up to ~54 draws a second).
- Most of the +50 MB is the window animating at all: Quill frees its spare
  Metal drawables 3 s after the display link goes idle (#1004), which the
  still baseline gets and any playing clip does not. For comparison, the
  existing inline-video demo with two clips measured 104 MB and 4.4 % with
  the same method.
- Held (window in the background, a layer or the emoji panel open) the
  players pause and nothing redraws for them.

## Chat windows

A chat in a window of its own plays its senders' clips too, with its own
players (`Surface::History(Some(chat))`), as each tdesktop history widget
keeps its own map. See `premium-effects-chat-windows.md`.
