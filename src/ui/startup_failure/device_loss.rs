//! A GPU lost mid-session on Windows. The platform destroys every window
//! once it cannot recreate the device (or its vsync thread panicked) and
//! reports itself headless. Quill then tries once to attach a new device and
//! reopen the main window; when that fails, or the device goes a second
//! time, it quits and says why ([`super::report_gpu_gone`]).
//!
//! The platform can also give up on one window's renderer while the device
//! still works. When that window is the main one, Quill reopens it the same
//! way and leaves the others (a PiP) open.
//!
//! GPUI's own quit on the last window closing would end the process before
//! any of this, so Quill quits explicitly instead, and does so itself when
//! the last window closes for any other reason.

use gpui_kit::{App, AsyncApp, GraphicalEnvironment, QuitMode, WindowingRequest};
use quill::lang::{keys, tr};
use std::cell::Cell;
use std::rc::Rc;

/// A window has closed.
#[derive(Debug, Clone, Copy)]
struct Closed {
    windows_left: usize,
    gpu_gone: bool,
    /// It was the main window, and Quill had not asked to quit.
    main_lost: bool,
    recovered_before: bool,
}

/// What to do when a window has closed.
#[derive(Debug, PartialEq, Eq)]
enum Step {
    Nothing,
    /// The last window closed and the GPU still works: the user is done.
    Quit,
    /// The GPU is gone: close what the platform left open (a PiP window),
    /// and act once the last one has closed.
    CloseTheRest,
    /// Attach a new device, then open the main window again.
    Recover,
    /// The device works: open the main window again.
    Reopen,
    GiveUp,
}

fn after_window_closed(closed: Closed) -> Step {
    let Closed {
        windows_left,
        gpu_gone,
        main_lost,
        recovered_before,
    } = closed;
    match (gpu_gone, windows_left, main_lost, recovered_before) {
        (true, 1.., _, _) => Step::CloseTheRest,
        (true, 0, _, false) => Step::Recover,
        (false, _, true, false) => Step::Reopen,
        (_, _, true, true) | (true, 0, false, true) => Step::GiveUp,
        (false, 0, false, _) => Step::Quit,
        (false, 1.., false, _) => Step::Nothing,
    }
}

/// Watch for the GPU going away. `reopen` opens the main window again,
/// with the link the old one had not opened yet.
pub(crate) fn recover_after_gpu_loss(
    cx: &mut App,
    reopen: impl Fn(&mut AsyncApp, Option<String>) -> gpui_kit::Result<()> + 'static,
) {
    cx.set_quit_mode(QuitMode::Explicit);
    let reopen = Rc::new(reopen);
    let recovered = Rc::new(Cell::new(false));
    cx.on_window_closed(move |cx, window_id| {
        // Quill never asks the platform to go headless, so headless here
        // means the platform dropped a device it could not recreate.
        let closed = Closed {
            windows_left: cx.windows().len(),
            gpu_gone: cx.graphical_environment().is_none(),
            main_lost: crate::ui::link_inbox::main_window_id(cx) == Some(window_id)
                && !super::quit_asked(),
            recovered_before: recovered.get(),
        };
        match after_window_closed(closed) {
            Step::Nothing => {}
            Step::Quit => cx.quit(),
            Step::CloseTheRest => {
                for window in cx.windows() {
                    let _ = window.update(cx, |_, window, _| window.remove_window());
                }
            }
            step @ (Step::Recover | Step::Reopen) => {
                recovered.set(true);
                let reopen = reopen.clone();
                cx.spawn(async move |cx| {
                    let attached = if step == Step::Recover {
                        let request = cx.update(|cx| {
                            cx.request_windowing(WindowingRequest::Windowed(
                                GraphicalEnvironment::detect(),
                            ))
                        });
                        request.await
                    } else {
                        Ok(())
                    };
                    let reopened = attached.and_then(|()| {
                        reopen(cx, crate::ui::deep_links::take_left_by_dropped_app())
                    });
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
    use super::{Closed, Step, after_window_closed};

    fn step(windows_left: usize, gpu_gone: bool, main_lost: bool, recovered_before: bool) -> Step {
        after_window_closed(Closed {
            windows_left,
            gpu_gone,
            main_lost,
            recovered_before,
        })
    }

    #[test]
    fn closing_the_last_window_quits_while_the_gpu_works() {
        assert_eq!(step(0, false, false, false), Step::Quit);
        assert_eq!(step(0, false, false, true), Step::Quit);
        assert_eq!(step(1, false, false, false), Step::Nothing);
    }

    #[test]
    fn a_lost_gpu_closes_every_window_then_recovers_once_then_gives_up() {
        assert_eq!(step(1, true, true, false), Step::CloseTheRest);
        assert_eq!(step(0, true, false, false), Step::Recover);
        assert_eq!(step(2, true, false, true), Step::CloseTheRest);
        assert_eq!(step(0, true, false, true), Step::GiveUp);
    }

    #[test]
    fn a_main_window_the_renderer_gave_up_on_reopens_once_beside_the_pip() {
        assert_eq!(step(1, false, true, false), Step::Reopen);
        assert_eq!(step(0, false, true, false), Step::Reopen);
        assert_eq!(step(1, false, true, true), Step::GiveUp);
    }
}
