//! A GPU lost mid-session on Windows. The platform destroys every window
//! once it cannot recreate the device (or its vsync thread panicked) and
//! reports itself headless. Quill then tries once to attach a new device and
//! reopen the main window; when that fails, or the device goes a second
//! time, it quits and says why ([`super::report_gpu_gone`]). The app is not
//! rebuilt: the new window takes over from the old one
//! ([`gpui_kit::Window::take_over`]), so the open chat, the dialogs and the
//! TDLib clients carry on.
//!
//! The platform can also give up on one window's renderer while the device
//! still works (Windows, and Linux when a window cannot adopt the recreated
//! device). When that window is the main one, Quill reopens it the same way
//! and leaves the others (a PiP) open. Linux has no headless step: a GPU that
//! is gone there quits from the renderer ([`super::watch_gpu_loss`]).
//!
//! GPUI's own quit on the last window closing would end the process before
//! any of this, so Quill quits explicitly instead, and does so itself when
//! the last window closes for any other reason.

use crate::ui::{QuillApp, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::{
    App, AsyncApp, Entity, Focusable as _, GraphicalEnvironment, KeptWindow, QuitMode,
    WindowOptions, WindowingRequest,
};
use quill::lang::{keys, tr};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// How long attaching a new device may take. A platform that never answers
/// the windowing request would leave the process alive with no window and
/// nothing to quit it.
const ATTACH_DEADLINE: std::time::Duration = std::time::Duration::from_secs(20);

/// The first of two futures to finish; the first one wins a tie.
async fn first_of<T>(
    first: impl std::future::Future<Output = T>,
    second: impl std::future::Future<Output = T>,
) -> T {
    use std::task::Poll;
    let mut first = std::pin::pin!(first);
    let mut second = std::pin::pin!(second);
    std::future::poll_fn(|cx| {
        if let Poll::Ready(value) = first.as_mut().poll(cx) {
            return Poll::Ready(value);
        }
        second.as_mut().poll(cx)
    })
    .await
}

/// The windowing `request`, or an error once `deadline` has passed first.
async fn attach_in_time(
    request: impl std::future::Future<Output = gpui_kit::Result<()>>,
    deadline: impl std::future::Future<Output = ()>,
) -> gpui_kit::Result<()> {
    first_of(request, async {
        deadline.await;
        Err(std::io::Error::other("the platform did not attach a new device in time").into())
    })
    .await
}

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

/// The kept root and the app under it, or `None` if it holds something else.
fn kept_app(kept: &KeptWindow, cx: &App) -> Option<(Entity<Root>, Entity<QuillApp>)> {
    let root = kept.root()?.clone().downcast::<Root>().ok()?;
    let shell = root.read(cx).view().clone().downcast::<QuillShell>().ok()?;
    let app = shell.read(cx).app.clone();
    Some((root, app))
}

/// Open a window around the app `kept` holds.
pub(crate) fn reopen_main_window(
    options: WindowOptions,
    kept: KeptWindow,
    cx: &mut AsyncApp,
) -> gpui_kit::Result<()> {
    let (root, app) = cx
        .update(|cx| kept_app(&kept, cx))
        .ok_or_else(|| std::io::Error::other("the closed main window kept no app"))?;
    // window-root-ok: `root` is the closed main window's Root, kept.
    cx.open_window(options, move |window, cx| {
        window.take_over(kept, cx);
        crate::ui::link_inbox::attach_main_window(window, cx, &app);
        if window.focused(cx).is_none() {
            window.focus(&app.focus_handle(cx), cx);
        }
        root
    })?;
    // What came in while there was no window can run now.
    quill::wake::LINKS.poke();
    quill::wake::CHROME.poke();
    Ok(())
}

/// Watch for the GPU going away. `reopen` opens the main window again around
/// what the old one kept.
pub(crate) fn recover_after_gpu_loss(
    cx: &mut App,
    reopen: impl Fn(&mut AsyncApp, KeptWindow) -> gpui_kit::Result<()> + 'static,
) {
    // Quill never asks the platform to go headless, so headless here means
    // the platform dropped a device it could not recreate.
    watch_closed_windows(|cx| cx.graphical_environment().is_none(), reopen, cx);
}

fn watch_closed_windows(
    gpu_gone: impl Fn(&App) -> bool + 'static,
    reopen: impl Fn(&mut AsyncApp, KeptWindow) -> gpui_kit::Result<()> + 'static,
    cx: &mut App,
) {
    cx.set_quit_mode(QuitMode::Explicit);
    let reopen = Rc::new(reopen);
    let recovered = Rc::new(Cell::new(false));
    let kept = RefCell::new(None);
    cx.on_window_closed(move |cx, window_id| {
        // Taken even when it will not be used: until dropped it holds the
        // app, and a quit must drop the app as it always did.
        if let Some(left) = cx.take_kept_window(window_id) {
            *kept.borrow_mut() = Some(left);
        }
        let closed = Closed {
            windows_left: cx.windows().len(),
            gpu_gone: gpu_gone(cx),
            main_lost: crate::ui::link_inbox::main_window_id(cx) == Some(window_id)
                && !super::quit_asked(),
            recovered_before: recovered.get(),
        };
        let step = after_window_closed(closed);
        if !matches!(step, Step::CloseTheRest | Step::Recover | Step::Reopen) {
            kept.borrow_mut().take();
        }
        match step {
            Step::Nothing => {}
            Step::Quit => cx.quit(),
            // A close nested in this callback never reaches the watcher, so
            // the last one would never act.
            Step::CloseTheRest => cx.defer(|cx| {
                for window in cx.windows() {
                    let _ = window.update(cx, |_, window, _| window.remove_window());
                }
            }),
            step @ (Step::Recover | Step::Reopen) => {
                recovered.set(true);
                let Some(left) = kept.borrow_mut().take() else {
                    give_up(tr(keys::Q_MISC_GPU_STOPPED_WORKING_AGAIN), cx);
                    return;
                };
                let reopen = reopen.clone();
                cx.spawn(async move |cx| {
                    let attached = if step == Step::Recover {
                        let request = cx.update(|cx| {
                            cx.request_windowing(WindowingRequest::Windowed(
                                GraphicalEnvironment::detect(),
                            ))
                        });
                        let deadline = cx.background_executor().timer(ATTACH_DEADLINE);
                        attach_in_time(request, deadline).await
                    } else {
                        Ok(())
                    };
                    let reopened = attached.and_then(|()| reopen(cx, left));
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
#[cfg(feature = "demo-capture")]
#[path = "device_loss_tests.rs"]
mod reset_tests;

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
