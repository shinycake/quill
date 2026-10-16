//! Where Picture-in-Picture's controls sit and what a press on them
//! does: tdesktop's `Pip::setupButtons`, `computeState` and the press
//! rules of `handleMousePress` / `handleMouseRelease` / `handleDoubleClick`
//! (`media_view_pip.cpp`), sizes from `media_view.style` (`pip*`).

use super::panel::{PipPoint, PipRect};

/// `pipControlSkip`: the padding around each button's icon.
pub const CONTROL_SKIP: i32 = 6;
/// `pipCloseIcon` / `pipEnlargeIcon` / `pipVolumeIcon*`: 24 × 24.
pub const ICON: i32 = 24;
/// `pipPlayIcon` / `pipPauseIcon`: 30 × 30, centered.
pub const PLAY_ICON: i32 = 30;
/// `pipPlaybackWidth`: the progress line at rest.
pub const PLAYBACK_WIDTH: f32 = 2.0;
/// `pipPlaybackWide`: the line under the pointer.
pub const PLAYBACK_WIDE: i32 = 4;
/// `pipPlaybackSkip`: the padding around the line's hit area.
pub const PLAYBACK_SKIP: i32 = 4;
/// `pipPlaybackTextSkip`: the time labels' inset.
pub const PLAYBACK_TEXT_SKIP: i32 = 6;
/// `pipPlaybackFont`: 11 px.
pub const PLAYBACK_FONT: f32 = 11.0;
/// `mediaviewVolumeWidth`: the volume line, when there is room.
pub const VOLUME_WIDTH: i32 = 75;
/// `st::fadeWrapDuration`: controls and button highlights fade this long.
pub const FADE_MS: u64 = 200;

/// What the pointer is over (tdesktop `Pip::OverState`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverState {
    /// Outside the picture.
    None,
    Close,
    Enlarge,
    /// The progress line at the bottom.
    Playback,
    VolumeToggle,
    VolumeController,
    /// The picture itself: a click pauses or resumes.
    Other,
}

impl OverState {
    /// `ResolveShownOver`: the volume line highlights with its toggle.
    pub fn shown(self) -> OverState {
        if self == OverState::VolumeController {
            OverState::VolumeToggle
        } else {
            self
        }
    }
}

/// A control's hit area and the icon (or line) inside it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Control {
    pub area: PipRect,
    pub icon: PipRect,
}

/// Every control's place in an `inner` picture rectangle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlsLayout {
    pub inner: PipRect,
    pub close: Control,
    pub enlarge: Control,
    pub volume_toggle: Control,
    /// Empty `icon` when the picture is too narrow for it.
    pub volume_controller: Control,
    pub play: PipRect,
    pub playback: Control,
}

impl ControlsLayout {
    /// `setupButtons` for `inner`. `controls_on_left` mirrors the window
    /// title controls: close and enlarge sit on that side's top corner
    /// (left on macOS, the system's side elsewhere) and the volume on the
    /// other.
    pub fn new(inner: PipRect, controls_on_left: bool) -> Self {
        let skip = CONTROL_SKIP;
        let button = ICON + 2 * skip;
        let mut close = PipRect::new(inner.x, inner.y, button, button);
        let mut enlarge = PipRect::new(close.right(), inner.y, button, button);

        let volume_skip = PLAYBACK_SKIP;
        let volume_height = 2 * volume_skip + PLAYBACK_WIDE;
        let toggle_width = ICON + 2 * skip;
        let toggle_height = ICON + 2 * skip;
        let volume_width = if VOLUME_WIDTH + 2 * skip + close.w + enlarge.w + toggle_width < inner.w
        {
            VOLUME_WIDTH
        } else {
            0
        };
        let mut controller = PipRect::new(
            inner.right() - volume_width - 2 * volume_skip,
            inner.y + (toggle_height - volume_height) / 2,
            volume_width,
            volume_height,
        );
        let mut toggle = PipRect::new(
            controller.x - ICON - skip,
            inner.y,
            toggle_width,
            toggle_height,
        );
        if !controls_on_left {
            close.x = inner.right() - (close.x - inner.x) - close.w;
            enlarge.x = inner.right() - (enlarge.x - inner.x) - enlarge.w;
            toggle.x = inner.x;
            controller.x = toggle.right();
        }
        let controller_icon = if controller.w > 2 * volume_skip && controller.h > 2 * volume_skip {
            controller.inset(volume_skip)
        } else {
            PipRect::default()
        };
        let playback_skip = PLAYBACK_SKIP;
        let playback_height = 2 * playback_skip + PLAYBACK_WIDE;
        let playback = PipRect::new(
            inner.x,
            inner.bottom() - playback_height,
            inner.w,
            playback_height,
        );
        Self {
            inner,
            close: Control {
                area: close,
                icon: close.inset(skip),
            },
            enlarge: Control {
                area: enlarge,
                icon: enlarge.inset(skip),
            },
            volume_toggle: Control {
                area: toggle,
                icon: toggle.inset(skip),
            },
            volume_controller: Control {
                area: controller,
                icon: controller_icon,
            },
            play: PipRect::new(
                inner.x + (inner.w - PLAY_ICON) / 2,
                inner.y + (inner.h - PLAY_ICON) / 2,
                PLAY_ICON,
                PLAY_ICON,
            ),
            playback: Control {
                area: playback,
                icon: playback.inset(playback_skip),
            },
        }
    }

    /// A live picture's layout: no volume line and no playback line, so
    /// the mute button takes the volume side's top corner, as close does
    /// its own.
    pub fn live(mut self) -> Self {
        let toggle = &mut self.volume_toggle;
        let on_right = self.close.area.x == self.inner.x;
        let x = if on_right {
            self.inner.right() - toggle.area.w
        } else {
            self.inner.x
        };
        let shift = x - toggle.area.x;
        toggle.area.x += shift;
        toggle.icon.x += shift;
        self.volume_controller = Control::default();
        self.playback = Control::default();
        self
    }

    /// `computeState`: what is under `point`.
    pub fn state_at(&self, point: PipPoint) -> OverState {
        if !self.inner.contains(point) {
            OverState::None
        } else if self.close.area.contains(point) {
            OverState::Close
        } else if self.enlarge.area.contains(point) {
            OverState::Enlarge
        } else if self.playback.area.contains(point) {
            OverState::Playback
        } else if self.volume_toggle.area.contains(point) {
            OverState::VolumeToggle
        } else if self.volume_controller.area.contains(point) {
            OverState::VolumeController
        } else {
            OverState::Other
        }
    }
}

/// `seekUpdate` / `volumeControllerUpdate`: the pointer's 0–1 position
/// along a line.
pub fn line_fraction(x: f32, line: PipRect) -> f32 {
    if line.w <= 0 || !x.is_finite() {
        return 0.0;
    }
    ((x - line.x as f32) / line.w as f32).clamp(0.0, 1.0)
}

/// `paintPlaybackProgress`: the line grows from [`PLAYBACK_WIDTH`] to its
/// full height as it becomes active (0–1).
pub fn line_height(active: f32, full: i32) -> f32 {
    let active = if active.is_finite() {
        active.clamp(0.0, 1.0)
    } else {
        0.0
    };
    PLAYBACK_WIDTH + (full as f32 - PLAYBACK_WIDTH) * active
}

/// What a finished press asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PressAction {
    Close,
    Enlarge,
    ToggleVolume,
    TogglePause,
    /// A double click on the picture: undo the first click's toggle and
    /// go back to the viewer.
    UndoToggleAndEnlarge,
}

/// The press bookkeeping of `handleMousePress` / `handleMouseRelease` /
/// `handleDoubleClick`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Presses {
    pressed: Option<OverState>,
    last_handled: Option<OverState>,
}

impl Presses {
    /// The left button went down over `over`.
    pub fn press(&mut self, over: OverState) {
        self.pressed = Some(over);
    }

    /// What is held, if a press is in progress.
    pub fn pressed(&self) -> Option<OverState> {
        self.pressed
    }

    /// The press took the window drag or the lines: no window drag starts
    /// from them (`setDragDisabled`).
    pub fn drag_disabled(&self) -> bool {
        matches!(
            self.pressed,
            Some(OverState::Playback | OverState::VolumeController)
        )
    }

    /// The button came up over `over`; `dragged` when the press moved the
    /// window.
    pub fn release(&mut self, over: OverState, dragged: bool) -> Option<PressAction> {
        let pressed = self.pressed.take();
        match pressed {
            Some(OverState::Playback | OverState::VolumeController) => None,
            Some(held) if !dragged && held == over => {
                self.last_handled = Some(over);
                match over {
                    OverState::Close => Some(PressAction::Close),
                    OverState::Enlarge => Some(PressAction::Enlarge),
                    OverState::VolumeToggle => Some(PressAction::ToggleVolume),
                    OverState::Other => Some(PressAction::TogglePause),
                    _ => None,
                }
            }
            _ => {
                self.last_handled = None;
                None
            }
        }
    }

    /// A double click came over `over`.
    pub fn double_click(&mut self, over: OverState) -> Option<PressAction> {
        (over == OverState::Other && self.last_handled == Some(OverState::Other))
            .then_some(PressAction::UndoToggleAndEnlarge)
    }
}

/// The keys Picture-in-Picture answers (Quill: tdesktop's window never
/// takes focus).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PipKey {
    Close,
    TogglePause,
}

/// The key `key` (GPUI's key name) asks for.
pub fn pip_key(key: &str) -> Option<PipKey> {
    match key {
        "escape" => Some(PipKey::Close),
        "space" => Some(PipKey::TogglePause),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ControlsLayout, OverState, PipKey, PressAction, Presses, VOLUME_WIDTH, line_fraction,
        line_height, pip_key,
    };
    use crate::media_viewer::pip::panel::{PipPoint, PipRect};

    #[test]
    fn controls_sit_in_the_title_controls_corner() {
        let inner = PipRect::new(0, 0, 320, 180);
        let mac = ControlsLayout::new(inner, true);
        assert_eq!(mac.close.area, PipRect::new(0, 0, 36, 36));
        assert_eq!(mac.close.icon, PipRect::new(6, 6, 24, 24));
        assert_eq!(mac.enlarge.area, PipRect::new(36, 0, 36, 36));
        assert_eq!(mac.volume_controller.area, PipRect::new(237, 12, 75, 12));
        assert_eq!(mac.volume_controller.icon, PipRect::new(241, 16, 67, 4));
        assert_eq!(mac.volume_toggle.area.x, 237 - 30);
        let other = ControlsLayout::new(inner, false);
        assert_eq!(other.close.area, PipRect::new(284, 0, 36, 36));
        assert_eq!(other.enlarge.area, PipRect::new(248, 0, 36, 36));
        assert_eq!(other.volume_toggle.area, PipRect::new(0, 0, 36, 36));
        assert_eq!(other.volume_controller.area.x, 36);
        // Play in the middle, the line along the bottom.
        assert_eq!(mac.play, PipRect::new(145, 75, 30, 30));
        assert_eq!(mac.playback.area, PipRect::new(0, 168, 320, 12));
        assert_eq!(mac.playback.icon, PipRect::new(4, 172, 312, 4));
    }

    #[test]
    fn a_live_picture_has_mute_in_the_corner_and_no_lines() {
        let inner = PipRect::new(0, 0, 320, 180);
        let mac = ControlsLayout::new(inner, true).live();
        assert_eq!(mac.volume_toggle.area, PipRect::new(284, 0, 36, 36));
        assert_eq!(mac.volume_toggle.icon, PipRect::new(290, 6, 24, 24));
        assert_eq!(mac.close, ControlsLayout::new(inner, true).close);
        let other = ControlsLayout::new(inner, false).live();
        assert_eq!(other.volume_toggle.area, PipRect::new(0, 0, 36, 36));
        for layout in [mac, other] {
            assert!(layout.volume_controller.area.is_empty());
            assert!(layout.playback.area.is_empty());
            let at = |x, y| layout.state_at(PipPoint::new(x, y));
            assert_eq!(at(160, 175), OverState::Other, "the picture, not a line");
        }
        assert_eq!(
            mac.state_at(PipPoint::new(300, 10)),
            OverState::VolumeToggle
        );
    }

    #[test]
    fn a_narrow_picture_has_no_volume_line() {
        let narrow = ControlsLayout::new(PipRect::new(0, 0, 180, 320), true);
        assert_eq!(narrow.volume_controller.area.w, 0);
        assert!(narrow.volume_controller.icon.is_empty());
        let wide = ControlsLayout::new(PipRect::new(0, 0, 214, 120), true);
        assert_eq!(wide.volume_controller.area.w, VOLUME_WIDTH);
    }

    #[test]
    fn the_state_under_the_pointer() {
        let layout = ControlsLayout::new(PipRect::new(0, 0, 320, 180), true);
        let at = |x, y| layout.state_at(PipPoint::new(x, y));
        assert_eq!(at(10, 10), OverState::Close);
        assert_eq!(at(50, 10), OverState::Enlarge);
        assert_eq!(at(220, 10), OverState::VolumeToggle);
        assert_eq!(at(260, 18), OverState::VolumeController);
        assert_eq!(at(160, 175), OverState::Playback);
        assert_eq!(at(160, 90), OverState::Other);
        assert_eq!(at(-1, 90), OverState::None);
        assert_eq!(at(320, 90), OverState::None);
        assert_eq!(OverState::VolumeController.shown(), OverState::VolumeToggle);
    }

    #[test]
    fn lines_map_the_pointer_and_grow_when_active() {
        let line = PipRect::new(4, 172, 312, 4);
        assert_eq!(line_fraction(4.0, line), 0.0);
        assert_eq!(line_fraction(160.0, line), 0.5);
        assert_eq!(line_fraction(400.0, line), 1.0);
        assert_eq!(line_fraction(f32::NAN, line), 0.0);
        assert_eq!(line_fraction(10.0, PipRect::default()), 0.0);
        assert_eq!(line_height(0.0, 4), 2.0);
        assert_eq!(line_height(1.0, 4), 4.0);
        assert_eq!(line_height(0.5, 4), 3.0);
    }

    #[test]
    fn presses_act_on_release_over_the_same_control_and_never_after_a_drag() {
        let mut presses = Presses::default();
        presses.press(OverState::Other);
        assert_eq!(
            presses.release(OverState::Other, false),
            Some(PressAction::TogglePause)
        );
        // The double click undoes that and goes back to the viewer.
        assert_eq!(
            presses.double_click(OverState::Other),
            Some(PressAction::UndoToggleAndEnlarge)
        );
        presses.press(OverState::Close);
        assert_eq!(presses.release(OverState::Enlarge, false), None);
        assert_eq!(presses.double_click(OverState::Other), None);
        presses.press(OverState::Close);
        assert_eq!(presses.release(OverState::Close, true), None, "dragged");
        presses.press(OverState::Enlarge);
        assert_eq!(
            presses.release(OverState::Enlarge, false),
            Some(PressAction::Enlarge)
        );
        presses.press(OverState::VolumeToggle);
        assert_eq!(
            presses.release(OverState::VolumeToggle, false),
            Some(PressAction::ToggleVolume)
        );
        presses.press(OverState::Playback);
        assert!(presses.drag_disabled());
        assert_eq!(presses.release(OverState::Playback, false), None);
        assert!(!presses.drag_disabled());
    }

    #[test]
    fn escape_closes_and_space_toggles() {
        assert_eq!(pip_key("escape"), Some(PipKey::Close));
        assert_eq!(pip_key("space"), Some(PipKey::TogglePause));
        assert_eq!(pip_key("k"), None);
    }
}
