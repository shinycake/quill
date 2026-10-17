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

- While a repost is the current story, the viewer sends `getStory` for its
  origin when it is not cached (`fetch_story_repost_origin`), as
  `recountDimensions` resolves it on show. A press then usually knows the
  answer already.
- `quill::story_repost::press` picks the action from the origin, the
  story's cache state (`story_card_state`) and whether the poster has a
  profile:
  - cached or still loading: open the story alone (`StoryOpenScope::Single`,
    tdesktop's `StoriesContextSingle`). A story still loading opens when the
    `getStory` answer arrives (`pending_story_open`). The pending open
    carries `if_gone`, so if the answer says the story is gone, or the ask
    fails, the first press opens the poster's profile; with no profile it
    shows the expired note. Playback holds while that press waits, so the
    repost cannot end under it. A step drops the press and playback runs
    again, and a newer open replaces one still waiting, so a late answer
    never moves the viewer after the user moved on;
  - known to be gone: the poster's profile, after the viewer closes, like
    the "Stories" button, or the expired note when the poster has no
    profile (the same note as a press that waited for the answer);
  - no poster or no story id: the viewer note "This story has expired."
    (`lng_forwarded_story_expired`), the only case tdesktop toasts.
- Opening another story from the viewer sends `closeStory` for the story it
  replaces before `openStory`, as stepping does, so TDLib's calls stay
  paired (`quill::story_viewer::story_open_switch`). Every close of the
  viewer, including the passcode lock and a deleted current story, goes
  through `release_story_viewer`: `closeStory`, and no open left waiting
  for an answer that could open the viewer or a profile under the lock.
- The label is `story-viewer-repost`, in `story_viewer/repost.rs`.
  "edited" is in the header (PR #947), not here.
- Unloaded chat: the "Chat 123" fallback is gone. tdesktop has no
  placeholder to copy, so the label reads "Reposted story" without a name.
