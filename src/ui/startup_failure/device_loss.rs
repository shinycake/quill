//! A GPU lost mid-session on Windows. The platform destroys every window
//! once it cannot recreate the device (or its vsync thread panicked) and
//! reports itself headless. Quill then tries once to attach a new device and
//! reopen the main window; when that fails, or the device goes a second
//! time, it quits and says why ([`super::report_gpu_gone`]).
//!
//! GPUI's own quit on the last window closing would end the process before
//! any of this, so Quill quits explicitly instead, and does so itself when
//! the last window closes for any other reason.

use gpui_kit::{App, AsyncApp, GraphicalEnvironment, QuitMode, WindowingRequest};
use quill::lang::{keys, tr};
use std::cell::Cell;
use std::rc::Rc;

/// What to do when a window has closed.
#[derive(Debug, PartialEq, Eq)]
enum Step {
    Nothing,
    /// The last window closed and the GPU still works: the user is done.
    Quit,
    /// The GPU is gone: close what the platform left open (a PiP window),
    /// and act once the last one has closed.
    CloseTheRest,
    Recover,
    GiveUp,
}

fn after_window_closed(windows_left: usize, gpu_gone: bool, recovered_before: bool) -> Step {
    match (windows_left, gpu_gone, recovered_before) {
        (0, false, _) => Step::Quit,
        (_, false, _) => Step::Nothing,
        (1.., true, _) => Step::CloseTheRest,
        (0, true, false) => Step::Recover,
        (0, true, true) => Step::GiveUp,
    }
}

/// Watch for the GPU going away. `reopen` opens the main window again.
pub(crate) fn recover_after_gpu_loss(
    cx: &mut App,
    reopen: impl Fn(&mut AsyncApp) -> gpui_kit::Result<()> + 'static,
) {
    cx.set_quit_mode(QuitMode::Explicit);
    let reopen = Rc::new(reopen);
    let recovered = Rc::new(Cell::new(false));
    // Quill never asks the platform to go headless, so headless here means
    // the platform dropped a device it could not recreate.
    cx.on_window_closed(move |cx, _| {
        let gpu_gone = cx.graphical_environment().is_none();
        match after_window_closed(cx.windows().len(), gpu_gone, recovered.get()) {
            Step::Nothing => {}
            Step::Quit => cx.quit(),
            Step::CloseTheRest => {
                for window in cx.windows() {
                    let _ = window.update(cx, |_, window, _| window.remove_window());
                }
            }
            Step::Recover => {
                recovered.set(true);
                let reopen = reopen.clone();
                cx.spawn(async move |cx| {
                    let request = cx.update(|cx| {
                        cx.request_windowing(WindowingRequest::Windowed(
                            GraphicalEnvironment::detect(),
                        ))
                    });
                    let reopened = match request.await {
                        Ok(()) => reopen(cx),
                        Err(error) => Err(error),
                    };
                    if let Err(error) = reopened {
                        log::error!("could not draw again after the GPU was lost: {error:#}");
                        cx.update(|cx| give_up(&format!("{error:#}"), cx));
                    }
                })
                .detach();
            }
            Step::GiveUp => give_up(tr(keys::Q_MISC_GPU_STOPPED_WORKING_AGAIN), cx),
        }
    })
    .detach();
}

/// Quit the normal way; the alert follows once the quit has finished.
fn give_up(detail: &str, cx: &mut App) {
    let _ = super::remember_gpu_gone(detail);
    cx.quit();
}

#[cfg(test)]
mod tests {
    use super::{Step, after_window_closed};

    #[test]
    fn closing_the_last_window_quits_while_the_gpu_works() {
        assert_eq!(after_window_closed(0, false, false), Step::Quit);
        assert_eq!(after_window_closed(0, false, true), Step::Quit);
        assert_eq!(after_window_closed(1, false, false), Step::Nothing);
    }

    #[test]
    fn a_lost_gpu_closes_every_window_then_recovers_once_then_gives_up() {
        assert_eq!(after_window_closed(1, true, false), Step::CloseTheRest);
        assert_eq!(after_window_closed(0, true, false), Step::Recover);
        assert_eq!(after_window_closed(2, true, true), Step::CloseTheRest);
        assert_eq!(after_window_closed(0, true, true), Step::GiveUp);
    }
}
