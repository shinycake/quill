//! Loading the FFmpeg shim off the UI thread.
//!
//! `quill::video_decode::available` dlopens the shim and its FFmpeg
//! libraries the first time it is asked (20 to 90 ms here), and the inline
//! players asked it while rendering. Now the render only reads whether the
//! load has finished; [`warm_decoder`], called when a signed-in session's
//! history first renders, loads it on the background executor and redraws
//! the windows once it has. Until then clips show their still, as they do
//! where the shim is missing. The sign-in screens never load it.
//!
//! Opening a player warms it too: a player (a chat list's userpic video)
//! can open before any history renders, and its decode thread would then
//! do the first load with nothing told to redraw once it ends.
#![deny(clippy::indexing_slicing)]

use gpui_kit::{App, Global, Task};

/// Where this app's load of the shim stands. The result itself is the
/// loader's own (`quill::video_decode::available_now`), read by
/// [`supported`]; this holds who is loading, so it is started once and
/// its redraw is not detached.
#[derive(Default)]
enum DecoderLoad {
    /// Not started.
    #[default]
    Idle,
    /// Loading on the background executor; the redraw follows.
    Loading { _task: Task<()> },
    /// The load ended (and the windows were told).
    Settled,
}

impl Global for DecoderLoad {}

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

/// Start loading the shim, once per app.
pub(crate) fn warm_decoder(cx: &mut App) {
    if cfg!(target_os = "macos") {
        return;
    }
    warm_with(quill::video_decode::available, App::refresh_windows, cx);
}

fn warm_with(load: fn() -> bool, loaded: fn(&mut App), cx: &mut App) {
    if !matches!(cx.default_global::<DecoderLoad>(), DecoderLoad::Idle) {
        return;
    }
    let task = cx.spawn(async move |cx| {
        let available = cx.background_executor().spawn(async move { load() }).await;
        cx.update(|cx| {
            // The load is over: the stage moves on, and this task is
            // dropped with the old value once it returns.
            *cx.default_global::<DecoderLoad>() = DecoderLoad::Settled;
            if available {
                loaded(cx);
            }
        });
    });
    *cx.default_global::<DecoderLoad>() = DecoderLoad::Loading { _task: task };
}

#[cfg(test)]
mod tests {
    use super::{supported_from, warm_with};
    use gpui_kit::{App, TestAppContext};
    use std::sync::atomic::{AtomicUsize, Ordering};

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
        let load = || {
            LOADS.fetch_add(1, Ordering::SeqCst);
            true
        };
        let loaded = |_: &mut App| {
            REFRESHES.fetch_add(1, Ordering::SeqCst);
        };
        cx.update(|cx| {
            warm_with(load, loaded, cx);
            warm_with(load, loaded, cx);
        });
        assert_eq!(
            REFRESHES.load(Ordering::SeqCst),
            0,
            "not before the load ends"
        );
        cx.run_until_parked();
        cx.update(|cx| warm_with(load, loaded, cx));
        cx.run_until_parked();
        assert_eq!(LOADS.load(Ordering::SeqCst), 1);
        assert_eq!(REFRESHES.load(Ordering::SeqCst), 1);
    }

    /// A player that opens before any history has rendered starts the load
    /// that redraws the windows, so its decode thread is not the one to load.
    #[cfg(not(target_os = "macos"))]
    #[gpui_kit::test]
    fn opening_a_player_starts_the_load(cx: &mut TestAppContext) {
        cx.update(|cx| {
            assert!(!cx.has_global::<super::DecoderLoad>());
            let _ = super::super::VideoPlayer::open(
                std::path::Path::new("/nonexistent/clip.mp4"),
                super::super::Purpose::Inline { max_edge: 64 },
                None,
                cx,
            );
            assert!(matches!(
                cx.global::<super::DecoderLoad>(),
                super::DecoderLoad::Loading { .. }
            ));
        });
    }
}
