# Stories that load after the viewer opened

## What tdesktop does

The viewer's list for a poster is the poster's `StoriesSource::ids`, every
active story whether loaded or not. The controller rebuilds it whenever the
source changes (`Stories::sourceChanged`, `media_stories_controller.cpp:868-874`),
and a jump to a story that is not loaded yet waits for it (`subjumpTo` and
`checkWaitingFor`, :1398-1441).

## What Quill did

`rebuild_story_viewer` built the item list once, from the stories already in
the cache (`collect_story_items` skips the rest). `open_story_viewer` sends
`getStory` for the missing ones, but when they landed the list never changed:
a poster with three stories, one of them cached, showed "1 of 1" for good.

## What changed

- `StoryViewer` now records what its list follows. A list opened from a
  poster's tray (`open_poster`) follows that poster; albums, the profile,
  search hits and single stories stay fixed.
- Every frame, `follow_story_tray` rebuilds the followed poster's list from
  the tray and the cache and hands it to `StoryViewer::refresh`, which keeps
  the current story current. So the list is a function of session state, with
  no rebuild trigger. Stories still loading are fetched (the driver drops
  duplicate `getStory` requests), and new stories the poster posts while the
  viewer is open join too.
- A rebuilt list without the current story is ignored: a deleted story closes
  the viewer through the existing check, and one that only expired stays on
  screen until the user moves on.

## Remaining difference

tdesktop counts unloaded stories in the progress bar and waits on them. Quill
counts only loaded ones, so the segment count can grow by one while the
`getStory` answers land, usually within the first frames.
