# Moving to the next poster at the end of their stories

## What tdesktop does

A viewer opened from the story tray walks the tray, not one poster
(`Media::Stories::Controller`, `media_stories_controller.cpp`):

- `subjumpFor` (:1375-1396): a step past the last story opens the next
  poster (`jumpFor(1)`), a step back from the first opens the previous one,
  and with no previous poster the first story restarts.
- `updatePlayback` (:1231-1240) and the Right key at the end close the viewer
  when there is no next poster.
- `rebuildCachedSourcesList` (:1628-1717) keeps the walk in the order the
  tray had when the viewer opened, so reading a poster (which moves it in the
  tray) does not move the next poster. Opened on an unread story, the walk
  keeps only posters with unread stories plus the current one
  (`_showingUnreadSources`).
- A poster opens on the story last shown for it in this viewer, else its
  first unread story, else its first (`LookupShownId`,
  `media_stories_sibling.cpp:37-46`; `StoriesSource::toOpen`).
- When fewer than `kPreloadPeersCount` (3) posters remain, it asks for more
  of the tray (`loadMore`, which is `loadActiveStories`).
- The neighbour posters show beside the story, shrunk (0.448) and dimmed,
  with their photo and name (`Sibling`, `media_stories_sibling.cpp`). A click
  opens that poster.

## What Quill did

The viewer held one poster's stories. Next on the last story did nothing,
autoplay stopped on the last story, and there was no way to reach the next
poster without closing the viewer.

## What changed

- `quill::story_tray_walk` is a pure port of the walk: `TrayWalk` (the
  cached sources list and the story shown per poster), `plan_step`
  (`subjumpFor`), `story_to_open` and `wants_more_tray`. Unit tests cover each.
- `StoryViewer` lists opened from the tray carry a `TrayWalk`.
  `ui/story_viewer/nav.rs` routes Prev / Next, the arrow keys and autoplay
  through `plan_step`: within the list, to a neighbour poster, a restart, or
  closing the viewer.
- `follow_story_tray` (each frame) merges tray changes into the walk,
  prefetches both neighbours' stories and pictures, and loads more of the
  tray when fewer than three posters remain. A 404 from
  `loadActiveStories` (everything loaded) marks the tray exhausted, so the
  load stops instead of repeating every frame.
- `ui/story_viewer/siblings.rs` draws the neighbour previews beside the
  story, centered in the space on each side. Prev / Next are disabled when
  they lead nowhere (`subjumpAvailable`).

## Deviations

- Any open of a story in a poster's tray uses the walk. tdesktop opens
  message cards, reply headers and links in a single-story context with no
  neighbours. The tray, card and reply callers share `open_story_viewer`, and
  the tray caller lives in a file this change does not own. Follow-up: give
  the non-tray callers a single-story open.
- A neighbour counts only when its story is cached. tdesktop jumps and closes
  the viewer if the story never loads. The prefetch makes this rare.
- tdesktop places a sibling a third of the way into the side space and
  scales it with the window. Quill centers it at a fixed size, and narrow
  windows clip it.
