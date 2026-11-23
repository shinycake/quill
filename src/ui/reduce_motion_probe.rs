//! The OS "reduce motion" preference, read after the first window is up.
//!
//! GPUI's animation and spring machinery settles instantly when
//! `App::set_reduce_motion(true)` is called, so kit `Skeleton` shimmer, switch
//! springs and spinners all go static with that one call.
//! `QUILL_REDUCED_MOTION=1` overrides the OS for testing.
//!
//! macOS (`defaults`) and Linux (`gsettings`) answer through a helper
//! program, which can hang on a wedged settings daemon, so the question never
//! runs on the thread that opens the first window: it runs on the background
//! executor with a time limit, and a late answer updates the flag once it
//! lands. Until then animations run as usual.
#![deny(clippy::indexing_slicing)]

use gpui_kit::*;

/// Hands Quill's answer (the OS reading, Power Saving and the battery) to
/// the kit as an explicit choice. A plain `App::set_reduce_motion` is a value
/// the kit's own OS reader may overwrite when it matches what the kit last
/// wrote, so a battery or Power Saving reduction could come undone on a live
/// OS change (Linux portal).
pub(crate) fn apply(cx: &mut App) {
    let preference = if quill::power_saving::reduce_motion_now() {
        gpui_kit::base::ReduceMotionPreference::Reduce
    } else {
        gpui_kit::base::ReduceMotionPreference::Allow
    };
    gpui_kit::base::set_reduce_motion_preference(preference, cx);
    // Every power, battery, heat and motion change passes here, and glass
    // pauses with them (docs/decisions/glass-effects.md).
    crate::ui::glass::refresh(cx);
}

/// Start the probe. The environment override applies at once; the OS answer
/// follows on the background executor.
pub(crate) fn apply_at_startup(cx: &mut App) {
    // Reduce Transparency and Increase Contrast, for glass effects.
    crate::ui::glass::probe::start(cx);
    if let Some(forced) = env_override() {
        quill::power_saving::set_os_reduce_motion(forced);
        apply(cx);
        return;
    }
    cx.spawn(async move |cx| {
        let prefers = cx
            .background_executor()
            .spawn(async { os_prefers_reduced_motion() })
            .await;
        cx.update(|cx| {
            quill::power_saving::set_os_reduce_motion(prefers);
            apply(cx);
        });
    })
    .detach();
}

fn env_override() -> Option<bool> {
    let value = std::env::var("QUILL_REDUCED_MOTION").ok()?;
    Some(value == "1" || value.eq_ignore_ascii_case("true"))
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn helper_says(program: &str, args: &[&str], on: &str) -> bool {
    quill::child_timeout::stdout_within(program, args, quill::child_timeout::DESKTOP_QUERY_LIMIT)
        .is_some_and(|out| out.trim() == on)
}

fn os_prefers_reduced_motion() -> bool {
    #[cfg(target_os = "macos")]
    {
        helper_says(
            "defaults",
            &["read", "com.apple.universalaccess", "reduceMotion"],
            "1",
        )
    }
    #[cfg(target_os = "linux")]
    {
        // GNOME exposes the preference as interface animations off.
        helper_says(
            "gsettings",
            &["get", "org.gnome.desktop.interface", "enable-animations"],
            "false",
        )
    }
    #[cfg(windows)]
    {
        // Settings > Accessibility > Visual effects > Animation effects.
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW,
        };
        let mut animations: i32 = 1;
        // SAFETY: SPI_GETCLIENTAREAANIMATION writes one BOOL to the pointer.
        let ok = unsafe {
            SystemParametersInfoW(
                SPI_GETCLIENTAREAANIMATION,
                0,
                (&mut animations as *mut i32).cast(),
                0,
            )
        };
        ok != 0 && animations == 0
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        false
    }
}
