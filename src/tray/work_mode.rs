//! tdesktop `Core::Settings::WorkMode` and the per-platform tray options
//! (`settings/sections/settings_advanced.cpp`, `BuildSystemIntegrationSection`).
//!
//! tdesktop stores one tri-state: window and tray, tray only, window only.
//! The settings page shows it as two switches, "Show tray icon" and "Show
//! taskbar icon", which are never both off: turning off one forces the other
//! back on. Quill stores the two switches (`show_tray_icon`,
//! `show_taskbar_icon`) and derives the mode, so older prefs files keep
//! working. Which switches exist depends on the platform:
//!
//! | platform | tray icon | taskbar icon | monochrome icon |
//! |----------|-----------|--------------|-----------------|
//! | macOS    | yes       | no (no per-window taskbar entry) | no (the menu bar icon is always a template) |
//! | Windows  | yes       | yes          | yes, when the taskbar theme can be read |
//! | Linux    | if a StatusNotifier host exists | X11 only (`_NET_WM_STATE_SKIP_TASKBAR`) | only with a `-symbolic` theme icon, which Quill does not ship |

/// tdesktop `WorkMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkMode {
    WindowAndTray,
    TrayOnly,
    WindowOnly,
}

impl WorkMode {
    /// The mode the two switches describe. Both off cannot happen in
    /// tdesktop; it reads as window only here (see [`toggle_taskbar`]).
    pub fn from_switches(tray: bool, taskbar: bool) -> Self {
        match (tray, taskbar) {
            (true, true) => Self::WindowAndTray,
            (true, false) => Self::TrayOnly,
            (false, _) => Self::WindowOnly,
        }
    }

    pub fn has_tray(self) -> bool {
        self != Self::WindowOnly
    }

    pub fn has_taskbar(self) -> bool {
        self != Self::TrayOnly
    }
}

/// The platforms whose option lists differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayPlatform {
    MacOs,
    Windows,
    LinuxX11,
    LinuxWayland,
}

impl TrayPlatform {
    /// The build's platform; on Linux `x11` says whether the window runs on
    /// X11 (the only backend with a skip-taskbar hint).
    pub fn current(x11: bool) -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else if x11 {
            Self::LinuxX11
        } else {
            Self::LinuxWayland
        }
    }

    /// tdesktop `Platform::SkipTaskbarSupported`.
    pub fn skip_taskbar_supported(self) -> bool {
        matches!(self, Self::Windows | Self::LinuxX11)
    }

    /// tdesktop `Platform::HasMonochromeSetting`: Windows when the taskbar
    /// theme is readable (`taskbar_theme_known`); Linux when the icon theme
    /// has a symbolic Quill icon, which it never does today.
    pub fn monochrome_supported(self, taskbar_theme_known: bool) -> bool {
        matches!(self, Self::Windows) && taskbar_theme_known
    }
}

/// Which of the three switches the page lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrayOptions {
    pub tray_icon: bool,
    pub taskbar_icon: bool,
    pub monochrome_icon: bool,
}

/// The option list for a platform. `tray_available` is whether the desktop
/// has a tray at all (tdesktop `TrayIconSupported`); `tray_on` is the saved
/// "Show tray icon" value, since the monochrome switch follows it.
pub fn options(
    platform: TrayPlatform,
    tray_available: bool,
    tray_on: bool,
    taskbar_theme_known: bool,
) -> TrayOptions {
    TrayOptions {
        // Without a tray host there is nothing to configure unless the user
        // turned the icon off earlier, which must stay reversible.
        tray_icon: tray_available || !tray_on,
        taskbar_icon: tray_available && platform.skip_taskbar_supported(),
        monochrome_icon: tray_available
            && tray_on
            && platform.monochrome_supported(taskbar_theme_known),
    }
}

/// The `(tray, taskbar)` pair after a switch change.
pub type Switches = (bool, bool);

/// "Show tray icon" changed. Turning it off forces the taskbar icon on.
pub fn toggle_tray((_, taskbar): Switches, on: bool) -> Switches {
    (on, taskbar || !on)
}

/// "Show taskbar icon" changed. Turning it off forces the tray icon on.
pub fn toggle_taskbar((tray, _): Switches, on: bool) -> Switches {
    (tray || !on, on)
}

/// Whether the main window leaves the taskbar: only when the user asked and
/// the tray icon can bring it back.
pub fn skip_taskbar((tray, taskbar): Switches) -> bool {
    tray && !taskbar
}

#[cfg(test)]
mod tests {
    use super::{TrayPlatform as P, WorkMode as W, *};

    #[test]
    fn mode_follows_the_switches() {
        assert_eq!(W::from_switches(true, true), W::WindowAndTray);
        assert_eq!(W::from_switches(true, false), W::TrayOnly);
        assert_eq!(W::from_switches(false, true), W::WindowOnly);
        assert_eq!(W::from_switches(false, false), W::WindowOnly);
        assert!(W::TrayOnly.has_tray() && !W::TrayOnly.has_taskbar());
        assert!(!W::WindowOnly.has_tray() && W::WindowOnly.has_taskbar());
    }

    #[test]
    fn never_both_off() {
        assert_eq!(toggle_tray((true, true), false), (false, true));
        assert_eq!(toggle_tray((true, false), false), (false, true));
        assert_eq!(toggle_tray((false, true), true), (true, true));
        assert_eq!(toggle_taskbar((true, true), false), (true, false));
        assert_eq!(toggle_taskbar((false, true), false), (true, false));
        assert_eq!(toggle_taskbar((true, false), true), (true, true));
    }

    #[test]
    fn work_mode_transitions() {
        let mut s = (true, true);
        s = toggle_taskbar(s, false);
        assert_eq!(W::from_switches(s.0, s.1), W::TrayOnly);
        s = toggle_tray(s, false);
        assert_eq!(W::from_switches(s.0, s.1), W::WindowOnly);
        s = toggle_tray(s, true);
        assert_eq!(W::from_switches(s.0, s.1), W::WindowAndTray);
    }

    #[test]
    fn taskbar_is_skipped_only_with_a_tray() {
        assert!(skip_taskbar((true, false)));
        assert!(!skip_taskbar((false, false)));
        assert!(!skip_taskbar((true, true)));
    }

    #[test]
    fn option_lists_per_platform() {
        // macOS: tray only.
        assert_eq!(
            options(P::MacOs, true, true, true),
            TrayOptions {
                tray_icon: true,
                taskbar_icon: false,
                monochrome_icon: false
            }
        );
        // Windows: all three.
        assert_eq!(
            options(P::Windows, true, true, true),
            TrayOptions {
                tray_icon: true,
                taskbar_icon: true,
                monochrome_icon: true
            }
        );
        // Windows with an unreadable taskbar theme: no monochrome switch.
        assert!(!options(P::Windows, true, true, false).monochrome_icon);
        // Monochrome follows the tray switch.
        assert!(!options(P::Windows, true, false, true).monochrome_icon);
        // Linux: taskbar switch on X11 only, never monochrome.
        assert!(options(P::LinuxX11, true, true, true).taskbar_icon);
        assert!(!options(P::LinuxWayland, true, true, true).taskbar_icon);
        assert!(!options(P::LinuxX11, true, true, true).monochrome_icon);
        // No tray host: hidden, unless the icon is off and must come back.
        assert!(!options(P::LinuxX11, false, true, true).tray_icon);
        assert!(options(P::LinuxX11, false, false, true).tray_icon);
        assert!(!options(P::LinuxX11, false, false, true).taskbar_icon);
    }
}
