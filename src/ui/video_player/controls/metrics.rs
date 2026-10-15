//! The viewer controller's geometry, from tdesktop's
//! `Telegram/SourceFiles/media/view/media_view.style` and the layout in
//! `media_view_playback_controls.cpp` (`PlaybackControls::resizeEvent`).
//! Colors are not here: they come from kit tokens and the viewer's dark
//! overlay (`controls::panel_background`).
//!
//! The rows are the design's arrangement (`designs/video-player.md`): the
//! scrubber with its time labels on top, the buttons under it. tdesktop
//! puts the buttons on top (`mediaviewPlayButtonTop` 2) and the scrubber
//! at `mediaviewPlaybackTop` 49; the sizes and insets are its own.

use gpui_kit::{Pixels, px};

/// `mediaviewControllerSize` width.
pub(crate) const PANEL_WIDTH: Pixels = px(480.);
/// `mediaviewControllerSize` height.
pub(crate) const PANEL_HEIGHT: Pixels = px(72.);
/// Opacity of the panel's black (`mediaviewSaveMsgBg` = `toastBg`, black at
/// 0.75 over the viewer backdrop).
pub(crate) const PANEL_OPACITY: f32 = 0.75;

/// Top of the scrubber row (tdesktop: `mediaviewPlaybackTop` 49 from the
/// top, in the bottom row).
pub(crate) const SCRUBBER_ROW_TOP: Pixels = px(8.);
/// Height of the scrubber row: the thumb (`mediaviewPlayback.seekSize`
/// 12) plus room for the drag size.
pub(crate) const SCRUBBER_ROW: Pixels = px(16.);
/// Top of the button row: under the scrubber row with a 4 px gap.
pub(crate) const BUTTON_ROW_TOP: Pixels = px(28.);

/// `mediaviewPlayButton` width and height, centered.
pub(crate) const PLAY_BUTTON: Pixels = px(40.);
/// `mediaviewPipButton` / `mediaviewVolumeToggle` / `mediaviewSpeedButton`
/// size: every side button is this square.
pub(crate) const SIDE_BUTTON: Pixels = px(32.);
/// `mediaviewButtonsRight`: the right group's inset.
pub(crate) const BUTTONS_RIGHT: Pixels = px(8.);
/// `mediaviewPipButtonSkip` / `mediaviewFullScreenButtonSkip`: the gap
/// between the right-hand buttons.
pub(crate) const BUTTONS_SKIP: Pixels = px(4.);
/// `mediaviewVolumeToggleLeft`.
pub(crate) const VOLUME_LEFT: Pixels = px(6.);
/// `mediaviewVolumeSkip`: between the volume toggle and its slider.
pub(crate) const VOLUME_SKIP: Pixels = px(3.);
/// `mediaviewVolumeWidth`.
pub(crate) const VOLUME_WIDTH: Pixels = px(75.);

/// `mediaviewPlayProgressLeft` + `mediaviewPlayProgressSkip`: the time
/// labels' inset from the panel edge (4 + 10).
pub(crate) const PROGRESS_INSET: Pixels = px(14.);
/// `mediaviewPlayProgressSkip`: between a time label and the scrubber.
pub(crate) const PROGRESS_SKIP: Pixels = px(10.);

/// `mediaviewPlayback.width`: the scrubber line at rest.
pub(crate) const TRACK: Pixels = px(3.);
/// The line while the pointer is over it (Quill: tdesktop only brightens
/// it, `activeFgOver`).
pub(crate) const TRACK_HOVER: Pixels = px(5.);
/// `mediaviewPlayback.seekSize`: the thumb.
pub(crate) const THUMB: Pixels = px(12.);
/// The thumb while it is dragged (Quill: one step larger than at hover).
pub(crate) const THUMB_DRAG: Pixels = px(14.);
/// `mediaviewOverDuration` is 150; the design asks the thumb to appear in
/// 120 ms so it is up before a quick click lands.
pub(crate) const THUMB_FADE_MS: u64 = 120;
/// The time bubble's gap above the scrubber line.
pub(crate) const BUBBLE_GAP: Pixels = px(8.);

/// The waiting spinner (kit `Spinner` at `size_8`, 2 rem).
pub(crate) const SPINNER: Pixels = px(32.);
/// The download ring over the picture.
pub(crate) const PROGRESS_RING: Pixels = px(48.);
