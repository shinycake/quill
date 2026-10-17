# Story viewer: the "Reposted from" line opens the origin

## What Telegram Desktop does

`media/stories/media_stories_repost_view.cpp:172-191` `RepostView::lookupHandler`
(the click handler of the repost quote):

- peer and original story id known, story loaded: jump to that story
  (`StoriesContextSingle`);
- story not loaded or deleted: open the peer's short info box;
- no peer or no id (a hidden poster): toast `lng_forwarded_story_expired`.

`recountDimensions` (`:226-246`) resolves the original story on show so the
click can open it. The view always has a peer, so tdesktop has no placeholder
like "Chat 123".

## What Quill did

`story_viewer_meta_line` was one plain string, "Reposted from X . edited",
falling back to "Chat 123" for a chat that is not loaded.

## What changed

- `quill::story_repost::press` picks the action from the origin and the
  story's cache state (`story_card_state`):
  - cached or still loading: open the story. `open_story_viewer` sends
    `getStory` for an uncached story and opens the viewer on it when the
    answer arrives (`pending_story_open`), which stands in for tdesktop's
    resolve on show;
  - gone (`getStory` said so): the poster's profile, after the viewer
    closes, like the "Stories" button. A poster with no profile target
    gets nothing;
  - no poster or no story id: the viewer note "This story has expired."
    (`lng_forwarded_story_expired`), the only case tdesktop toasts.
- Opening another story from the viewer sends `closeStory` for the story it
  replaces before `openStory`, as stepping does, so TDLib's calls stay paired.
- The label is a button (`story-viewer-repost`). "edited" is in the header
  (PR #947), not here.
- Unloaded chat: the "Chat 123" fallback is gone. tdesktop has no
  placeholder to copy, so the label reads "Reposted story" without a name.
