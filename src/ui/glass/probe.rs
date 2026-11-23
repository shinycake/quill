//! The system's Reduce Transparency and Increase Contrast, read after the
//! first window is up, as `reduce_motion_probe` reads Reduce Motion. The
//! helper programs (`defaults`, `gsettings`) can hang on a wedged settings
//! daemon, so the question runs on the background executor with a time
//! limit; until it answers, glass counts the system as asking for no
//! reduction. `QUILL_REDUCE_TRANSPARENCY=1` and `QUILL_INCREASE_CONTRAST=1`
//! override it for tests and demos.
#![deny(clippy::indexing_slicing)]

use gpui_kit::*;

/// What the system asks of glass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SystemGlass {
    pub(crate) reduce_transparency: bool,
    pub(crate) increase_contrast: bool,
}

/// Start the probe; its answer goes through the funnel (`super::set_system`).
pub(crate) fn start(cx: &mut App) {
    let overridden = env_override();
    if overridden != SystemGlass::default() {
        super::set_system(overridden, cx);
        return;
    }
    cx.spawn(async move |cx| {
        let answer = cx
            .background_executor()
            .spawn(async { read_system() })
            .await;
        cx.update(|cx| super::set_system(answer, cx));
    })
    .detach();
}

fn env_flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
}

fn env_override() -> SystemGlass {
    SystemGlass {
        reduce_transparency: env_flag("QUILL_REDUCE_TRANSPARENCY"),
        increase_contrast: env_flag("QUILL_INCREASE_CONTRAST"),
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn helper_says(program: &str, args: &[&str], on: &str) -> bool {
    quill::child_timeout::stdout_within(program, args, quill::child_timeout::DESKTOP_QUERY_LIMIT)
        .is_some_and(|out| out.trim() == on)
}

fn read_system() -> SystemGlass {
    #[cfg(target_os = "macos")]
    {
        let key = |name: &str| {
            helper_says(
                "defaults",
                &["read", "com.apple.universalaccess", name],
                "1",
            )
        };
        SystemGlass {
            reduce_transparency: key("reduceTransparency"),
            increase_contrast: key("increaseContrast"),
        }
    }
    #[cfg(target_os = "linux")]
    {
        // Linux has no standard Reduce Transparency; GNOME's high contrast
        // stands for Increase Contrast.
        SystemGlass {
            reduce_transparency: false,
            increase_contrast: helper_says(
                "gsettings",
                &["get", "org.gnome.desktop.a11y.interface", "high-contrast"],
                "true",
            ),
        }
    }
    #[cfg(windows)]
    {
        // Settings > Personalization > Colors > Transparency effects, and
        // a high-contrast theme (the HCF_HIGHCONTRASTON bit of Flags).
        let transparency = quill::winreg::get_dword(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",
            "EnableTransparency",
        )
        .ok()
        .flatten();
        let contrast_flags = quill::winreg::get_string(
            // A registry path, not interface text.
            concat!("Control Panel", "\\Accessibility\\HighContrast"),
            "Flags",
        )
        .ok()
        .flatten()
        .and_then(|flags| flags.trim().parse::<u32>().ok());
        SystemGlass {
            reduce_transparency: transparency == Some(0),
            increase_contrast: contrast_flags.is_some_and(|flags| flags & 1 != 0),
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        SystemGlass::default()
    }
}
