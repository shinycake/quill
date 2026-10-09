# B14 stories: video playback, close friends, hide, share, save, profile

## What tdesktop does

Read from `Telegram/SourceFiles/media/stories/` (`media_stories_controller.cpp`,
`media_stories_view.cpp`, `media_stories_share.cpp`) and the close-friends box:

- A video story plays inside the viewer; the progress segment follows the
  video position, and the story advances when the clip ends.
- Pressing and holding the media pauses the story until release; Space toggles
  pause; a mute button toggles the sound; arrow keys move between stories.
- Share opens the chat chooser and sends the story as a message; the story
  link is `https://t.me/<username>/s/<id>`; saving is allowed only when the
  story is not protected.
- Hide stories moves the peer's stories to the hidden list
  (`setChatActiveStoriesList`); the close friends box edits the whole list.
- Own stories can be posted to / removed from the profile.

## What changed in Quill

- Video playback reuses `ui/native_video.rs`: AVPlayer on macOS, the bundled
  FFmpeg (`video_decode`) on Linux and Windows. The player drives the progress
  bar and auto-advance (`story_extras::video_progress` / `video_finished`);
  the duration clock stays frozen while a clip is native-driven, and takes over
  (thumbnail + TDLib duration) if the clip cannot open or never downloads
  (6 s grace, no download running). Frames are redrawn through the frame clock
  (`request_media_tick`, 30 fps) only while playing and unpaused.
- Mute toggle, press-and-hold pause, Space (`StoryTogglePause`, rebindable
  `story-pause`), Left/Right step stories. These yield to text inputs
  (reply, share search, close-friends search, privacy, report, cover).
- New requests (all in `schema/td_api.tl`): `getCloseFriends`,
  `setCloseFriends`, `setChatActiveStoriesList`,
  `toggleStoryIsPostedToChatPage`, and `inputMessageStory` through
  `sendMessage`. They ride the existing story-page op status line.
- Close-friends editor: contact checkboxes with search, whole-list save,
  reachable from the viewer ("Close friends...") and from the privacy panel
  when "Close friends" is selected.
- Share sends only stories with `can_be_forwarded`; Save is gated the same
  way and copies the clip or photo to Downloads.
- `ParsedStory` gains `is_posted_to_chat_page`,
  `can_toggle_is_posted_to_chat_page`, `can_get_statistics`.

## Skipped

- Story statistics (`getStoryStatistics`): needs graph parsing and a panel;
  not small. `parity:stories-statistics` stays open.
- Story link via TDLib: there is no `getStoryLink` in the schema; the link is
  built locally from the public username and is unavailable without one.
- Close-friends entry from the story composer overlay (the editor panel lives
  in the viewer).
- Tap left/right thirds of the media to navigate (Prev/Next buttons and
  arrow keys exist).

## Verification

- Unit tests: `story_extras` (progress, finish rules, pause state, edit
  selection, link, labels), request shapes, reducer round trips for the new
  purposes, driver gating for hide / profile / share.
- Demo capture `ready-story-video` (the generated 12 s fixture clip plays in
  the viewer) and `ready-story-more` (close-friends editor): rendered
  in-process on macOS.
- Linux / Windows video playback uses the same `NativeVideo` FFmpeg backend as
  the media viewer; not run on those OSes here (macOS can try it with
  `QUILL_VIDEO_BACKEND=ffmpeg`).
