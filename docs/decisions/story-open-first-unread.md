# Stories tray opens the first unread story

## What Telegram Desktop does

- `data/data_stories.cpp:178-184` `StoriesSource::toOpen()`: the first story
  with an id after `readTill`, or the first story when all are read.
- `window/window_session_controller.cpp:3927` `openPeerStories` (the tray and
  avatar ring entry point) opens `source->toOpen()`. The viewer's sibling
  navigation (`media_stories_sibling.cpp:45`) uses the same rule.

## What Quill did

`StoryTile.latest_story` was the highest story id, so a tap skipped every
unread story before the newest one.

## What changed

- `story_viewer::story_to_open` implements the rule over
  `ChatActiveStoriesView` (`max_read_story_id`), with unit tests.
- The tray tile stores that id and opens it.
- Other entry points open one specific story (forwarded story card, reply
  header, deep link), which tdesktop does too (`openPeerStory` with the
  given id), so they are unchanged. The chat list avatar ring is drawn but
  not clickable in Quill, so it has no entry point to change.
