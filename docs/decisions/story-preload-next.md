# Story viewer: preload the neighbouring stories

## What Telegram Desktop does

- `media/stories/media_stories_controller.cpp:828-844` `Controller::preloadNext`
  runs on every `show`. With `kPreloadNextMediaCount = 3` and
  `kPreloadPreviousMediaCount = 1` (`:81-82`) it hands
  `Stories::setPreloadingInViewer` the ids `index + 1 ..< min(index + 3, count)`
  and then `index - 1`. The next loop is exclusive, so it covers two stories
  ahead, not three. Quill matches the loop, not the constant's name.
- `data/data_stories.cpp:2571-2620` preloads one story at a time, in that
  order, and remembers finished ones in `_preloaded`.
- `data/data_story.cpp:935-957` `StoryPreload`: a photo is preloaded only if
  the auto-download settings allow it for that peer (`PhotoPreload::Should`).
  A video is preloaded when it can be streamed, with no auto-download check.
- `data/data_media_preload.cpp:24-31, 120-142`: a video preloads only a
  prefix: `videoPreloadPrefix` (the story's `preload_prefix_size`, ignored at
  6 MiB or more), else `min(4 MiB, size)`.

## What Quill did

`ensure_story_download` fetched only the current story, so every tap to the
next story waited for the network.

## What changed

- `src/story_preload.rs` (core): `preload_indices` gives tdesktop's order;
  `plan` turns the neighbouring items into `downloadFile` requests. Photos
  follow the photo auto-download flag. A video gets its thumbnail (Quill shows
  that while the clip loads) and a clip prefix of `preload_prefix_size`, else
  4 MiB, never more than the file. Data saver pauses it all, like every other
  automatic download.
- Priorities 7, 6, 5 for the first, second and previous story
  (`STORY_PRELOAD_PRIORITIES`): above chat auto-downloads (4), below the
  current story (32), so the story on screen is never starved.
- `ConnectDriver::sync_story_preloads` sends the plan as `downloadFile` with
  `limit`, recorded with its own origin (`DownloadOrigin::Preload`). A
  preload ends with its prefix, and `is_stalled_auto` then keeps it from
  being requested again, which plays the role of `_preloaded`.
- A preloaded story that becomes current asks for the whole file. A preload
  marks the file in flight, so the usual dedupe would swallow the current
  story's request and leave the clip stuck at the prefix.
  `download_user_file` therefore re-sends `downloadFile` (limit 0, priority
  32) whenever the file's last request was a preload, running or ended.
  TDLib replaces the limit and priority of an active download with the new
  request's.
- Stale preloads are cancelled. tdesktop holds one `StoryPreload`, and its
  destructor cancels the download when it is replaced or the viewer closes
  (`data_stories.cpp:2571-2620`, `data_media_preload.cpp:54-56, 147-149`).
  Each re-plan sends `cancelDownloadFile` (`only_if_pending` false) for the
  running preloads the new plan dropped, except the current story's files.
  Closing the viewer cancels all of them.
- `StoryContentView::Video` keeps `preload_prefix_size`.

Not matched: tdesktop preloads one story at a time. TDLib orders concurrent
downloads by priority, so Quill sends all of them and lets the priorities
decide.
