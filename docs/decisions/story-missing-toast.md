# Opening a story that no longer exists

## What tdesktop does

A story link resolves the story first. If `Stories::lookup` still has no
story after `resolve`, the window shows the toast
`lng_stories_link_invalid` ("This link is broken or has expired.") instead
of opening the viewer (`window/window_session_controller.cpp:690-705`). Inside
the viewer, a story the controller was waiting for that comes back as
`NoStory::Deleted` stops the wait (`media_stories_controller.cpp:1425-1441`).

## What Quill did

`open_story_viewer` fires `getStory` for the clicked story and parks the
open in `pending_story_open` until the story is cached. A failed `getStory`
already adds the story to `stories.missing` (`state/domains/stories/error.rs`),
but the pending open only waited for the cache. A deleted or expired story
never arrived, so the click did nothing and the open stayed pending forever.

## What changed

- The pending open now follows the same three states the forwarded-story card
  uses (`Session::story_card_state`): ready opens the viewer, expired drops
  the open and shows the tdesktop toast, loading keeps waiting.
- A story `getStory` already failed for is not requested again on the next
  click, so the toast shows on the next frame, as tdesktop does for a story
  it already knows is deleted.
- The opening code moved from `ui/story_viewer/mod.rs` to
  `ui/story_viewer/open.rs` to keep the module under 1,000 lines.
- New `ready-story-viewer` screenshot demo: the showcase tray with a photo
  behind every story, opened like a tray tap. `QUILL_DEMO_STORY=gone` opens a
  story the server no longer has.
