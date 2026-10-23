# Story album picker and video cover strip

## What tdesktop does

**Albums.** `EditAlbumBox` (`info/stories/info_stories_inner_widget.cpp`) is a
wide box titled "Add Stories" over the user's story grid in the
`StoryAlbumEdit` wrap. The stories the album already holds start ticked,
tapping a tile toggles it, and the button reads "Add Stories" while the edit
only adds and "Save" when it also removes. Saving sends one `stories.updateAlbum`
with the added and removed ids. Creating an album is a separate name box
(`NewAlbumBox`, 12 characters); an empty album shows "Add Stories" to open the
same picker. The name box has no story picker.

**Cover.** tdesktop has no cover picker for stories. The nearest thing is the
video editor's `Editor::VideoTimeline` (frame strip with a cover playhead),
which Quill already has for sent videos (`video_editor_timeline.rs`). For a
posted story's cover (`editStoryCover`) the mobile apps show a preview of the
chosen frame over a strip of frames with a draggable handle; that is the model
used here, as the brief allowed.

## What changed

- `quill::story_album_picker`: selection rules (ticks, added/removed against
  the album, candidate list from archive + chat page + album, minus stories the
  server says cannot be added).
- `ui/story_albums/picker.rs`: the picker in the story page. A thumbnail grid
  (photo size or video thumbnail, duration badge on videos), a check on each
  tile, a button that counts: "Add N Stories" when only adding, "Save (N
  stories)" when removing too, "Create Album with N Stories" for a new album.
  The next archive page loads when the grid scrolls within 160 px of its end (a spinner row shows while it loads), with no "Load more" button. The old comma-separated id inputs and `parse_story_id_list` are gone. The
  count in the button is a Quill addition; tdesktop's button has no number.
- `quill::story_cover` and `ui/story_albums/cover_strip.rs`: the cover picker
  replaces the seconds input. A preview of the chosen frame over eight frames
  cut across the clip, a draggable handle (mouse, and Left/Right/Home/End with
  Shift for big steps), the time next to the title, Set / Cancel. Frames come
  from the same ffmpeg paths as the video editor (`render_strip_frame`,
  `render_frame`), so macOS, Windows and Linux behave alike. Duration comes
  from `probe_source`, falling back to `storyVideo.duration`.

## Verification

Demos `ready-story-album-picker` and `ready-story-cover-strip` (light and
dark, handle dragged by a scripted click). Unit tests for both rule modules and
GPUI tests for opening, ticking and closing. Not shown by captures: a live
`updateStoryAlbum`/`editStoryCover` round trip and ffmpeg on Windows/Linux.
