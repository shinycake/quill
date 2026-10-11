//! The tray switches of General settings, as tdesktop's "System
//! integration" block lists them: "Show tray icon", "Show taskbar icon" and
//! "Use monochrome icon". Which of them a platform offers and how they
//! constrain each other is `quill::tray::work_mode`; this file only draws
//! the switches and applies the result.

use super::QuillApp;
use gpui_kit::component::switch::Switch;
use gpui_kit::*;
use quill::tray::monochrome;
use quill::tray::work_mode::{self, TrayOptions, TrayPlatform};
use std::sync::atomic::{AtomicBool, Ordering};

/// Whether the main window's backend honors a skip-taskbar request. Set when
/// the window is first seen ([`QuillApp::apply_taskbar_icon`]); on Linux this
/// is what tells X11 from Wayland.
static WINDOW_SKIPS_TASKBAR: AtomicBool = AtomicBool::new(false);

pub(super) fn note_window_backend(skips_taskbar: bool) {
    WINDOW_SKIPS_TASKBAR.store(skips_taskbar, Ordering::Relaxed);
}

fn platform() -> TrayPlatform {
    TrayPlatform::current(WINDOW_SKIPS_TASKBAR.load(Ordering::Relaxed))
}

impl QuillApp {
    /// The `(tray, taskbar)` switch pair as saved.
    fn tray_switch_pair(&self) -> work_mode::Switches {
        (
            self.appearance.show_tray_icon,
            self.appearance.show_taskbar_icon,
        )
    }

    /// Which tray switches this platform and desktop offer right now.
    pub(crate) fn tray_options(&self, tray_available: bool) -> TrayOptions {
        work_mode::options(
            platform(),
            tray_available,
            self.appearance.show_tray_icon,
            monochrome::dark_taskbar().is_some(),
        )
    }

    /// "Show tray icon", "Show taskbar icon" and "Use monochrome icon",
    /// in tdesktop's order, limited to what the platform offers.
    pub(crate) fn tray_icon_sections(
        &self,
        tray_available: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let options = self.tray_options(tray_available);
        let mut body = Vec::new();
        if options.tray_icon {
            body.push(
                self.appearance_section(
                    cx,
                    "Show tray icon",
                    "Keep Quill in the system tray with the unread count.",
                    Switch::new("general-show-tray-icon")
                        .checked(self.appearance.show_tray_icon)
                        .accessibility_label("Show the Quill tray icon")
                        .on_click(cx.listener(|this, &on, window, cx| {
                            let (tray, taskbar) =
                                work_mode::toggle_tray(this.tray_switch_pair(), on);
                            this.set_appearance(cx, |a| {
                                a.show_tray_icon = tray;
                                a.show_taskbar_icon = taskbar;
                            });
                            this.apply_taskbar_icon(window);
                        }))
                        .into_any_element(),
                ),
            );
        }
        if options.taskbar_icon {
            body.push(
                self.appearance_section(
                    cx,
                    "Show taskbar icon",
                    "Off keeps Quill out of the taskbar; the tray icon opens it.",
                    Switch::new("general-show-taskbar-icon")
                        .checked(self.appearance.show_taskbar_icon)
                        .accessibility_label("Show Quill in the taskbar")
                        .on_click(cx.listener(|this, &on, window, cx| {
                            let (tray, taskbar) =
                                work_mode::toggle_taskbar(this.tray_switch_pair(), on);
                            this.set_appearance(cx, |a| {
                                a.show_tray_icon = tray;
                                a.show_taskbar_icon = taskbar;
                            });
                            this.apply_taskbar_icon(window);
                        }))
                        .into_any_element(),
                ),
            );
        }
        if options.monochrome_icon {
            body.push(
                self.appearance_section(
                    cx,
                    "Use monochrome icon",
                    "Draw the tray icon in one color to match the taskbar.",
                    Switch::new("general-tray-monochrome")
                        .checked(self.appearance.tray_icon_monochrome)
                        .accessibility_label("Use a monochrome tray icon")
                        .on_click(cx.listener(|this, &on, _, cx| {
                            this.set_appearance(cx, |a| a.tray_icon_monochrome = on)
                        }))
                        .into_any_element(),
                ),
            );
        }
        body
    }

    /// The main window's taskbar entry follows "Show taskbar icon"
    /// (Windows, X11; the tray icon must be on, see `work_mode::skip_taskbar`).
    pub(crate) fn apply_taskbar_icon(&self, window: &Window) {
        let supported = super::window_control::taskbar_toggle_supported(window);
        note_window_backend(supported);
        if !supported {
            return;
        }
        super::window_control::set_skip_taskbar(
            window,
            work_mode::skip_taskbar(self.tray_switch_pair()),
        );
    }
}
