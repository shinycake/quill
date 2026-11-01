//! Loading the FFmpeg shim off the UI thread.
//!
//! `quill::video_decode::available` dlopens the shim and its FFmpeg
//! libraries the first time it is asked (20 to 90 ms here), and the inline
//! players asked it while rendering. Now the render only reads whether the
//! load has finished; [`warm_decoder`], called when a signed-in session's
//! history first renders, loads it on the background executor and redraws
//! the windows once it has. Until then clips show their still, as they do
//! where the shim is missing. The sign-in screens never load it.
#![deny(clippy::indexing_slicing)]

use gpui_kit::App;
use std::sync::atomic::{AtomicBool, Ordering};

/// Whether clips can play in this process: always on macOS (AVPlayer);
/// elsewhere once the bundled FFmpeg shim has loaded. In tests, also once
/// the test scripted a backend (CI has no shim).
pub(crate) fn supported() -> bool {
    #[cfg(test)]
    if super::tests::scripted_backends() {
        return true;
    }
    cfg!(target_os = "macos") || supported_from(quill::video_decode::available_now())
}

/// `None`: the load has not finished, so the still shows for now.
fn supported_from(loaded: Option<bool>) -> bool {
    loaded == Some(true)
}

/// Start loading the shim, once per process.
pub(crate) fn warm_decoder(cx: &mut App) {
    static STARTED: AtomicBool = AtomicBool::new(false);
    if cfg!(target_os = "macos") {
        return;
    }
    warm_with(
        &STARTED,
        quill::video_decode::available,
        App::refresh_windows,
        cx,
    );
}

fn warm_with(started: &'static AtomicBool, load: fn() -> bool, loaded: fn(&mut App), cx: &mut App) {
    if started.swap(true, Ordering::AcqRel) {
        return;
    }
    cx.spawn(async move |cx| {
        let available = cx.background_executor().spawn(async move { load() }).await;
        if available {
            cx.update(loaded);
        }
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::TestAppContext;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn a_load_still_running_shows_the_still() {
        assert!(!supported_from(None));
        assert!(!supported_from(Some(false)));
        assert!(supported_from(Some(true)));
    }

    static LOADS: AtomicUsize = AtomicUsize::new(0);
    static REFRESHES: AtomicUsize = AtomicUsize::new(0);

    #[gpui_kit::test]
    fn the_finished_load_redraws_the_windows_once(cx: &mut TestAppContext) {
        static STARTED: AtomicBool = AtomicBool::new(false);
        let load = || {
            LOADS.fetch_add(1, Ordering::SeqCst);
            true
        };
        let loaded = |_: &mut App| {
            REFRESHES.fetch_add(1, Ordering::SeqCst);
        };
        cx.update(|cx| {
            warm_with(&STARTED, load, loaded, cx);
            warm_with(&STARTED, load, loaded, cx);
        });
        assert_eq!(
            REFRESHES.load(Ordering::SeqCst),
            0,
            "not before the load ends"
        );
        cx.run_until_parked();
        cx.update(|cx| warm_with(&STARTED, load, loaded, cx));
        cx.run_until_parked();
        assert_eq!(LOADS.load(Ordering::SeqCst), 1);
        assert_eq!(REFRESHES.load(Ordering::SeqCst), 1);
    }
}
