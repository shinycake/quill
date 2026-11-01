//! Links, activation and quit requests from outside the window.
#![deny(clippy::indexing_slicing)]

/// Drains links that arrived from the OS or a second launch: raises the
/// window and hands the next link to the deep-link pump (one flow at a time).
pub fn install_link_inbox(
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::App,
    view: &gpui_kit::Entity<super::QuillApp>,
) {
    note_main_window(window, cx);
    let activated = view.downgrade();
    cx.on_app_lifecycle(move |phase, cx| {
        if phase == gpui_kit::AppLifecyclePhase::Active {
            main_window_on_activation(&activated, cx);
        }
    })
    .detach();
    cx.spawn({
        let view = view.downgrade();
        let window_handle = window.window_handle();
        async move |cx| {
            loop {
                // Asleep until a link, activation or quit request arrives
                // (the OS callback and the single-instance listener poke
                // `wake::LINKS`); links waiting behind another flow are
                // retried at the old cadence.
                let seen = quill::wake::LINKS.epoch();
                let wait = if quill::deep_link_inbox::has_pending_links() {
                    std::time::Duration::from_millis(250)
                } else {
                    std::time::Duration::from_secs(60)
                };
                super::wake_wait::sleep_or_wake(
                    cx.background_executor(),
                    &quill::wake::LINKS,
                    seen,
                    wait,
                )
                .await;
                let alive = view
                    .update(cx, |this, _| {
                        if this.pending_deep_link.is_none()
                            && let Some(link) = quill::deep_link_inbox::pop_link()
                        {
                            this.queue_deep_link(link);
                        }
                    })
                    .is_ok();
                if !alive {
                    break;
                }
                if quill::deep_link_inbox::take_activation() {
                    let _ = window_handle.update(cx, |_, window, cx| raise_main_window(window, cx));
                }
            }
        }
    })
    .detach();
}

/// Brings the main window to the front, restoring it from the tray, the
/// Dock (minimized) or a hidden app.
pub fn raise_main_window(window: &mut gpui_kit::Window, cx: &mut gpui_kit::App) {
    // Hidden to the tray (Windows, X11) or minimized (everywhere): show it
    // again, then bring it to the front.
    super::window_control::set_visible(window, true);
    cx.activate(true);
    window.activate_window();
}

/// The main window, for bringing it back on macOS ([`reopen_main_window`],
/// [`main_window_on_activation`]).
#[derive(Clone, Copy)]
struct MainWindow {
    handle: gpui_kit::AnyWindowHandle,
    /// The user closed it at some point. A window that starts in the tray
    /// is hidden too, and the activation of a launch must not show it.
    closed: bool,
}

impl gpui_kit::Global for MainWindow {}

/// Note the main window for [`reopen_main_window`].
pub fn note_main_window(window: &gpui_kit::Window, cx: &mut gpui_kit::App) {
    cx.set_global(MainWindow {
        handle: window.window_handle(),
        closed: false,
    });
}

/// macOS calls this for a Dock click, with windows showing or not: a main
/// window hidden by [`close_main_window`] comes back. AppKit restores a
/// minimized one itself.
pub fn reopen_main_window(cx: &mut gpui_kit::App) {
    raise_main_window_if(super::window_control::is_hidden, cx);
}

fn raise_main_window_if(hidden: impl FnOnce(&gpui_kit::Window) -> bool, cx: &mut gpui_kit::App) {
    if let Some(main) = cx.try_global::<MainWindow>().copied() {
        let _ = main.handle.update(cx, |_, window, cx| {
            if hidden(window) {
                raise_main_window(window, cx);
            }
        });
    }
}

/// Quill came forward by Cmd-Tab or a click on one of its windows, as in
/// tdesktop's `applicationDidBecomeActive`: a main window the user closed
/// comes back, unless a call or chat window is what came forward.
fn main_window_on_activation(view: &gpui_kit::WeakEntity<super::QuillApp>, cx: &mut gpui_kit::App) {
    let Some(main) = cx.try_global::<MainWindow>().copied() else {
        return;
    };
    let closed = main.closed
        && main
            .handle
            .update(cx, |_, window, _| super::window_control::is_hidden(window))
            .unwrap_or(false);
    if closed
        && view
            .update(cx, |app, cx| app.main_window_wanted(cx))
            .unwrap_or(false)
    {
        reopen_main_window(cx);
    }
}

/// How often, and how many times, a window closed in full screen is asked
/// whether it left: 3 s in all, tdesktop's `kHideAfterFullscreenTimeoutMs`.
const LEFT_FULLSCREEN_POLL: std::time::Duration = std::time::Duration::from_millis(100);
const LEFT_FULLSCREEN_POLLS: u32 = 30;

/// Hide the main window, tdesktop's `closeWithoutDestroy`: on a Mac it
/// leaves full screen first, because hiding it there leaves a black Space.
fn hide_main_window(window: &mut gpui_kit::Window, cx: &mut gpui_kit::App) {
    if cx.has_global::<MainWindow>() {
        cx.global_mut::<MainWindow>().closed = true;
    }
    if !(cfg!(target_os = "macos") && window.is_fullscreen()) {
        hide_or_minimize(window);
        return;
    }
    window.toggle_fullscreen();
    let main = window.window_handle();
    cx.spawn(async move |cx| {
        for _ in 0..LEFT_FULLSCREEN_POLLS {
            cx.background_executor().timer(LEFT_FULLSCREEN_POLL).await;
            let left = main.update(cx, |_, window, _| !window.is_fullscreen());
            if left.unwrap_or(true) {
                break;
            }
        }
        let _ = main.update(cx, |_, window, _| hide_or_minimize(window));
    })
    .detach();
}

fn hide_or_minimize(window: &mut gpui_kit::Window) {
    if !super::window_control::set_visible(window, false) {
        window.minimize_window();
    }
}

/// What the main window's close button and Close Window do
/// (`quill::tray::close_outcome`). Returns whether the window closes.
pub fn close_main_window(
    run_in_background: bool,
    signed_in: bool,
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::App,
) -> bool {
    use quill::tray::{CloseOutcome, close_outcome};
    match close_outcome(
        run_in_background,
        quill::tray::tray_available(),
        cfg!(target_os = "macos"),
        super::window_control::hide_supported(window),
        signed_in,
    ) {
        CloseOutcome::Quit => {
            // No account to keep running for; a Mac app would stay alive
            // with no window otherwise.
            #[cfg(target_os = "macos")]
            cx.quit();
            true
        }
        CloseOutcome::Hide => {
            hide_main_window(window, cx);
            false
        }
        CloseOutcome::Minimize => {
            window.minimize_window();
            false
        }
    }
}

impl super::QuillApp {
    /// Whether Quill coming forward should show the closed main window: not
    /// over a call, which takes the focus instead (tdesktop's
    /// `activateCurrentCall`), or over a chat window, which is the last
    /// active window then.
    fn main_window_wanted(&self, cx: &mut gpui_kit::App) -> bool {
        if let Some(call) = self.calls.window.or(self.group_call.window) {
            let _ = call.update(cx, |_, window, _| window.activate_window());
            return false;
        }
        !self.windows.any_open()
    }

    /// An account is signed in, on screen or in the background: closing
    /// the window on a Mac hides it ([`close_main_window`]).
    pub fn signed_in(&self) -> bool {
        self.session().is_some_and(|session| {
            matches!(
                session.auth,
                quill::telegram::envelope::AuthorizationState::Ready
            )
        }) || !self.background.clients.is_empty()
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "link_inbox_tests.rs"]
mod tests;
