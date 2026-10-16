# Viewer: chrome above the picture, measured caption, true full screen

## What Telegram Desktop does

- `media/view/media_view_overlay_widget.cpp` draws the picture first and
  the controls, arrows, top bar and caption over it. Nothing it draws for
  the viewer is ever under the video.
- `refreshCaptionGeometry` lays the caption out at the window's width
  before it places the picture. The caption takes at most
  `_maxUsedHeight / 4`, in whole lines, and the picture fits in what is
  left.
- `_fullScreenVideo` makes the picture fill the whole screen
  (`resizeContentByScreenSize`). The caption and top bar go, and the
  controller and arrows sit over the picture and fade on idle. While they
  are hidden the cursor hides too (`updateCursor`). `displayPhoto` and
  `displayDocument` leave full screen when another item shows.
- In full screen `updateOver` treats the whole window as `Over::Video`, so
  a click anywhere outside the controls plays or pauses. Nothing closes
  the viewer on a click.

## What was wrong

- The viewer panel was one cached slice under the playing video. A caption
  under a video was covered by the picture except for its last line.
- The caption kept a fixed 32 px under the picture, so a wrapped caption
  overlapped it. Its real height was read from the text box's
  `ScrollHandle` one frame late.
- The full screen button flipped its icon and asked the window to go full
  screen, but the layout did not change. The picture kept its margins and
  the top bar stayed.
- The backdrop was 92% black, so the chat list showed through.

## What changed

- `app_slice.rs`: the viewer is two cached slices. `SliceKind::Viewer`
  holds the media box (thumbnail or photo with its zoom, pan and menu
  handlers) and draws under the video layer. `SliceKind::ViewerChrome`
  holds the top bar, arrows, caption and controls and draws above it.
  `media_viewer_panel` takes a `ViewerPart` (`Inline`, `Media`, `Chrome`).
  Chat windows still draw everything inline. The chrome moved from
  `overlay.rs` to the new `chrome.rs`.
- The backdrop is opaque black.
- `geometry.rs` `fit_area` is a pure function of the window size, video or
  photo, the caption plate's height and full screen. In full screen the
  fit area is the whole window.
- `caption.rs` shapes the caption at the lane width before anything draws
  (`CaptionLayout`). `message_text/measure.rs` lays it out with the runs
  `rich_text_line` draws, which `paragraph.rs` builds for both: bold,
  italic, code in the monospace font, and custom emoji. Quotes and `pre`
  blocks add the insets their blocks draw. The picture makes room for the
  plate's real height. What shows under the picture is capped at a
  quarter of the window in whole lines. Text past that scrolls inside the
  plate. Long captions still collapse to two lines with "Show more".
- `fullscreen.rs` `VideoFullscreen` is the one flag the layout, the exit
  icon, the keys and the Touch Bar read. It tracks the window: `Entering`
  becomes `Active` once the window is full screen, and leaving OS full
  screen some other way (the title-bar button, a system shortcut) ends the
  mode. A window that was already full screen stays so when the mode ends.
  Leaving while the window is still entering moves to `Cancelling`: once
  the window reports full screen it is toggled back, and if it never does
  within a second the mode just ends. Entering again while leaving keeps
  the window Quill made full screen. Each window follows its own state,
  so a viewer in a chat window tracks that window.
  Showing another item leaves the mode. Changing the video quality keeps
  it.
- In full screen the top bar and caption are hidden. The arrows and the
  controller sit over the picture, the controller on a black gradient
  scrim so it reads on bright and dark frames. Both fade with the idle
  timer, and the cursor hides with them.
- In full screen a click on the picture or the black bars plays or
  pauses, as in tdesktop. Outside full screen a click on the backdrop
  still closes the viewer.
- Only the controller, caption and error rows take the pointer. The scrim
  and the band around the rows take none, and once the fade ends the
  controls are not drawn, so a click lands on the picture under them.
  The seek bar's time bubble shows only while the controls are shown.

## Deviations

- The scrim is only behind the bottom controls. In full screen the top
  bar is hidden, and outside full screen it sits above the picture, not
  over it.
- The cursor hides only in full screen. The platform call hides it system
  wide, which is only safe when the window covers the screen.
- The caption plate wraps at the lane width, as in tdesktop, not at the
  picture's width.

## Verified

- Unit tests: `geometry.rs` (a wrapped caption keeps its height under the
  picture, no caption keeps no room, full screen fills the window),
  `caption.rs` (shown lines, the collapsed toggle, the quarter-window cap),
  `fullscreen.rs` (enter and leave from a windowed or full screen window,
  leaving OS full screen another way, leaving while entering, entering
  while leaving) and `measure.rs` (bold and code runs are measured as
  drawn). The GPUI test in `caption_ui_tests.rs` checks the shaped layout
  of a long caption.
- GPUI tests in `video_player/tests/fullscreen.rs`: full screen fits the
  picture to the window with no top bar or caption, and Escape leaves
  full screen before it closes the viewer. A click on the bars in full
  screen plays or pauses and keeps the viewer open, and a click on the
  backdrop outside full screen closes it.
- Demo captures at 1600x1000 before and after: a two-line caption under a
  video, in light and dark; full screen with controls shown and faded on a
  bright and a dark 16:9 clip; a vertical clip pillarboxed in full screen;
  the photo viewer with a caption. Letterbox and pillarbox pixels sample
  as #000000. Not committed.

## Not verified

- Real OS full screen. The captures never enter it, so they show the
  `Entering` state. The transition, and leaving it from the title bar,
  were not run.
- The test text system shapes every run alike, so the GPUI tests cannot
  see bold or code change a caption's width; `measure.rs` tests the runs
  instead. Quote and `pre` insets are read off their blocks' styles, not
  measured. If they are off, the plate scrolls instead of hiding text.
- `Cancelling` against a real OS animation. The test window enters full
  screen at once.
- Cursor hiding, live playback and Linux and Windows.
