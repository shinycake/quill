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
- `caption.rs` shapes the caption with `shape_text` at the lane width
  before anything draws (`CaptionLayout`). The picture makes room for the
  plate's real height. What shows under the picture is capped at a
  quarter of the window in whole lines. Text past that scrolls inside the
  plate. Long captions still collapse to two lines with "Show more".
- `fullscreen.rs` `VideoFullscreen` is the one flag the layout, the exit
  icon, the keys and the Touch Bar read. It tracks the window: `Entering`
  becomes `Active` once the window is full screen, and leaving OS full
  screen some other way (the title-bar button, a system shortcut) ends the
  mode. A window that was already full screen stays so when the mode ends.
  Showing another item leaves the mode. Changing the video quality keeps
  it.
- In full screen the top bar and caption are hidden. The arrows and the
  controller sit over the picture, the controller on a black gradient
  scrim so it reads on bright and dark frames. Both fade with the idle
  timer, and the cursor hides with them.

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
  `caption.rs` (shown lines, the collapsed toggle, the quarter-window cap)
  and `fullscreen.rs` (enter and leave from a windowed or full screen
  window, leaving OS full screen another way). The GPUI test in
  `caption_ui_tests.rs` checks the shaped layout of a long caption.
- Demo captures at 1600x1000 before and after: a two-line caption under a
  video, in light and dark; full screen with controls shown and faded on a
  bright and a dark 16:9 clip; a vertical clip pillarboxed in full screen;
  the photo viewer with a caption. Letterbox and pillarbox pixels sample
  as #000000. Not committed.

## Not verified

- Real OS full screen. The captures never enter it, so they show the
  `Entering` state. The transition, and leaving it from the title bar,
  were not run.
- The caption is shaped as plain text in one font. Quotes, code blocks
  and custom emoji can lay out taller; the plate then scrolls instead of
  hiding text.
- Leaving full screen while the OS is still animating into it.
- Cursor hiding, live playback and Linux and Windows.
