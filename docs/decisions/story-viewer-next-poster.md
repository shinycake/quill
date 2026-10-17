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

## How each entry point scopes the viewer

`Controller::rebuildFromContext` (`media_stories_controller.cpp:720-777`)
builds the walk, the siblings and `loadMore` only for a `StorySourcesList`
context. `Single`, `Peer` and `Album` call `hideSiblings()`.

| Entry point | tdesktop context | Quill `StoryOpenScope` |
|---|---|---|
| Tray tile (`dialogs_widget.cpp:1792`) | `StorySourcesList` | `Tray` |
| Message story card, story reply header (`JumpToStoryClickHandler`, `history_item_helpers.cpp:889-906`) | `StoriesContextSingle` | `Single` |
| `t.me/<user>/s/<id>` (`window_session_controller.cpp:693-705`) | `StoriesContextSingle`, toast `lng_stories_link_invalid` when gone | `Link` |

## What changed

- `quill::story_tray_walk` is a pure port of the walk: `TrayWalk` (the
  cached sources list of one story list, main or archive, and the story
  shown per poster), `plan_step` (`subjumpFor`), `story_to_open`
  (`StoriesSource::toOpen`, the one function the tray tile and the walk
  share), `shown_story` (`LookupShownId`) and `wants_more_tray`. Unit tests
  cover each.
- `open_story_viewer` takes a `StoryOpenScope`, carried through
  `pending_story_open`. A `Tray` open fetches the poster's active stories
  and opens inside the walk over the list the poster is in. A `Single` or
  `Link` open fetches only its story and shows it alone: no walk, no
  siblings, and Next or autoplay at its end closes the viewer.
- `ui/story_viewer/nav.rs` routes Prev / Next, the arrow keys and autoplay
  through `plan_step`: within the list, to a neighbour poster, a restart, or
  closing the viewer. A neighbour counts only when its story can be shown
  now (`story_sibling_item`), the same test the jump uses. A neighbour whose
  story is gone is passed over for the next poster in the walk. The jump
  updates the walk first and switches the list and the walk together, so
  they cannot disagree about the current poster.
- `follow_story_tray` (each frame) merges tray changes into the walk,
  prefetches both neighbours' stories and pictures, and loads more of the
  main tray when fewer than three posters remain.
- Tray paging: a 404 from `loadActiveStories` (everything loaded) ends it.
  Any other failure waits 5 s, doubling per failure up to 5 minutes, before
  the next page. A new Ready clears both.
- `quill::story_sibling_layout` ports tdesktop's sibling geometry
  (`media_stories_controller.cpp:482-546`): the multiplier is
  `max(0.448, 200 / story width)`, capped at 0.72 and at the side space
  over `2 * (1 - 0.24)`; x is `max(min(left / 3, left - 64), -0.24 * w)`.
  The siblings are children of the story frame, so they centre on it.

## Folded in from the reviews of #944 and #948

- #944 M1: only a 400 or 404 from `getStory` marks a story gone. A
  timeout, FLOOD_WAIT or 500 leaves it to be asked for again.
- #944 L2: only a `Link` open toasts that the link is broken. A card or a
  tray tap of a gone story drops the open quietly, like tdesktop's
  `openPeerStory`; the card already shows the story as expired.
- #948 M1: when a rebuild of the followed poster's list lacks the current
  story (it left the tray but is still cached), the current story is kept
  in its place by id and the rest of the list stays live, instead of the
  whole list freezing.

## Deviations

- The viewer's panel is now exactly the story column (the 360 px frame
  plus its 16 px padding), so the side space is known from the window
  width alone. Before, a wide action row or caption could widen it to 480
  px. tdesktop keeps its controls within the story width too
  (`storiesControlsExtend`).
- The side space is measured from that column rather than from the bare
  frame, so a sibling never overlaps the panel's padding.
- tdesktop centres the siblings on the window, where its story frame is
  centred too. Quill's frame sits under the header inside the panel, so
  the siblings centre on the frame instead.
- Only the main list pages (`loadActiveStories(storyListMain)`). No Quill
  caller opens the archive in a `Tray` scope yet, so the archive walk
  covers the posters already loaded.
- A neighbour whose story is still loading does not count, so Next at the
  end of a poster closes the viewer if the next story has not arrived.
  tdesktop would jump and then close if it never loaded. The prefetch makes
  this rare.
