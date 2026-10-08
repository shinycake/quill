# Viewer: GIFs, Shared Media paging, keys, caption custom emoji

tdesktop references are under `Telegram/SourceFiles/media/view/media_view_overlay_widget.cpp`.

## B32 GIFs / animations in the viewer
- tdesktop streams an animation video-only with `options.loop = !_stories` (:5614) and creates no playback controls for it (`streamingRequiresControls`, :1491-1494). Quill: new `MediaViewerKind::Animation`; the viewer shows no transport (zoom row only, as for photos), no sound, and loops.
- Clicking a GIF in a chat row opens the viewer (`animation_attachment` got a `viewer` argument); the Play disc keeps toggling inline playback. Secret GIFs never open.
- Playback reuses the existing viewer video path:
  - macOS: AVPlayer (`NativeVideo`), volume 0, restarted by the viewer tick when it ends. `.gif` files (not an AVPlayer format) use the frames path.
  - Other platforms: ffmpeg frames (`animation::viewer_loop_frames_cancelable`, 480 px, at most 200 frames) cycled by the playback clock, wrapping at the end. No ffplay (GIFs are silent).
  - No ffmpeg / unplayable: the thumbnail stays, a red error line shows, and an "Open externally" button (`platform::open_local_file`) appears for videos and GIFs.
- Window inactive: looping clips pause and resume on reactivation (`sync_viewer_window_activity`; tdesktop pauses in the background, :584).
- Animation rules: no `with_animation`; native GIFs request the shared frame clock at 30 fps (`request_animation_tick`), frames GIFs at their fps (max 30). Regular videos keep their previous refresh.
- Not done: video notes (round videos) are not opened in the viewer; Quill plays them in their bubble.

## B32 Shared Media paging
- tdesktop pages the viewer over `SharedMediaWithLastSlice`. Quill: `SharedMediaItem` keeps the `HistoryMessage` for the Media and GIFs tabs; clicking a photo/video/GIF row calls `open_shared_media_viewer` (other rows still jump to the message). The viewer list is the tab's loaded items, oldest first (left = older, as in the chat viewer), header "Photo 27 of 30" from the tab's `total_count`.
- Near the start of the loaded list (`VIEWER_PRELOAD_AHEAD` = 6) the viewer asks for the next older page: `GetSharedMediaMore` -> `searchChatMessages` with the tab filter and `from_message_id = next_from_message_id`. `begin_fetch_more` allows one page in flight, keeps the generation, dedupes by id (TDLib repeats `from_message_id`), and stops when `next_from_message_id == 0` or a page adds nothing. A failed page can be asked for again. `MediaViewer::merge_older` prepends and keeps the current item.
- Delete, album pin and delete-pruning use chat history, so they are not offered / skipped for Shared Media items outside the loaded history (Show in Chat works).

## B33 Keys (`handleKeyPress`, :7308-7470)
- `viewer_key_action` (pure, tested) maps: Space / K / Enter play-pause (:7329, :7416); J / L seek -/+10 s (:7338; `kSeekTimeMsLong`); Alt or Ctrl(Cmd)+Enter toggles video full screen (:7324). In video full screen the arrows seek 5 s, `0` restarts and `1`-`9` jump to n/10, Escape leaves it (:7383-7401); otherwise arrows page, Escape closes. Handled in the existing keystroke interceptor; skipped while a dialog is open so the delete confirmation keeps Enter/Space.
- A Full screen button joins the video controls. Full screen is the window's full screen plus the seek mode; leaving restores the previous window state.
- Copy and Save defaults now use the platform modifier (`primary!`: Cmd on macOS, Ctrl elsewhere) instead of `cmd-`; zoom also accepts Cmd/Ctrl + `=` / `-` (tdesktop requires Ctrl) next to the bare keys.
- Not done: Space hold = 2x speed boost (needs key-release events), `,` `.` frame step, Alt+arrows chapters.

## B34 Caption custom emoji
- The viewer caption passes `custom_emoji_paths` (the message-text resolver) to `rich_text_line`: resolved stickers render inline, the span text until then. `open_chat_custom_emoji_ids` now also scans photo/video/GIF captions and Shared Media items, so the ids get resolved and downloaded. Still images only (no per-message animated frames).

## Verification
- Unit tests: key mapping, seek clamp, animation items, Shared Media position / merge / preload, state paging (append, dedupe, end, failure retry, stale generation), caption emoji ids. `gate.sh`: GATE OK.
- Demo captures (`ready-viewer-gif`, `ready-viewer-shared`) viewed: GIF frame with "GIF" header and custom-emoji caption; "Photo 27 of 30" over the Shared Media panel with prev/next arrows and caption emoji.
- Not verified live: AVPlayer GIF loop and the inactive pause (no live Telegram, static captures); Linux/Windows build not run here (only macOS).
