//! The OS Quill's shortcuts are spelled for. GPUI 0.3.10 takes it as an
//! argument (or reads it from `cx.platform_info()`); Quill ships native
//! builds only, so it is the build target. For code without an `App`: a
//! keystroke parser, a demo script, a test.

use gpui_kit::OperatingSystem;

pub(crate) const HOST_OS: OperatingSystem = if cfg!(target_os = "macos") {
    OperatingSystem::Mac
} else if cfg!(target_os = "windows") {
    OperatingSystem::Windows
} else {
    OperatingSystem::Linux
};

/// Cmd on macOS, Ctrl elsewhere: `Modifiers::secondary_key` without an `App`.
#[cfg(feature = "demo-capture")]
pub(crate) fn secondary_key() -> gpui_kit::Modifiers {
    if cfg!(target_os = "macos") {
        gpui_kit::Modifiers::command()
    } else {
        gpui_kit::Modifiers::control()
    }
}
