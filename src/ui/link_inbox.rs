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
            main_window_on_activation(&activated, super::window_control::is_hidden_on_mac, cx);
        }
    })
    .detach();
    quill::single_instance::UI.start();
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
                // A second launch waits for this before it is acknowledged.
                quill::single_instance::UI.beat();
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
                    quill::single_instance::UI.stop();
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
    take_hide_after_full_screen(cx);
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
    closed_by_user: bool,
    hide_after_full_screen: bool,
}

impl gpui_kit::Global for MainWindow {}

/// Note the main window for [`reopen_main_window`].
pub fn note_main_window(window: &gpui_kit::Window, cx: &mut gpui_kit::App) {
    cx.set_global(MainWindow {
        handle: window.window_handle(),
        closed_by_user: false,
        hide_after_full_screen: false,
    });
}

/// The main window's id, once it opened.
#[cfg_attr(not(any(test, target_os = "windows")), allow(dead_code))]
pub(crate) fn main_window_id(cx: &gpui_kit::App) -> Option<gpui_kit::WindowId> {
    cx.try_global::<MainWindow>()
        .map(|main| main.handle.window_id())
}

fn take_hide_after_full_screen(cx: &mut gpui_kit::App) -> bool {
    cx.has_global::<MainWindow>()
        && std::mem::take(&mut cx.global_mut::<MainWindow>().hide_after_full_screen)
}

/// macOS calls this for a Dock click, with windows showing or not: a main
/// window hidden by [`super::QuillApp::close_main_window`] comes back.
/// AppKit restores a minimized one itself.
pub fn reopen_main_window(cx: &mut gpui_kit::App) {
    raise_main_window_if(super::window_control::is_hidden_on_mac, cx);
}

fn raise_main_window_if(hidden: impl FnOnce(&gpui_kit::Window) -> bool, cx: &mut gpui_kit::App) {
    if let Some(main) = cx.try_global::<MainWindow>().copied() {
        let _ = main.handle.update(cx, |_, window, cx| {
            if main.hide_after_full_screen || hidden(window) {
                raise_main_window(window, cx);
            }
        });
    }
}

/// Quill came forward by Cmd-Tab or a click on one of its windows, as in
/// tdesktop's `applicationDidBecomeActive`: a main window the user closed
/// comes back, unless a call takes the focus (`activateCurrentCall`) or a
/// chat window is the last active window.
fn main_window_on_activation(
    view: &gpui_kit::WeakEntity<super::QuillApp>,
    hidden: impl FnOnce(&gpui_kit::Window) -> bool,
    cx: &mut gpui_kit::App,
) {
    let Some(main) = cx.try_global::<MainWindow>().copied() else {
        return;
    };
    let closed = main.closed_by_user
        && (main.hide_after_full_screen
            || main
                .handle
                .update(cx, |_, window, _| hidden(window))
                .unwrap_or(false));
    if !closed {
        return;
    }
    let Ok((call, chat_window)) = view.read_with(cx, |app, _| {
        (
            app.calls.window.or(app.group_call.window),
            app.windows.any_open(),
        )
    }) else {
        return;
    };
    if let Some(call) = call {
        let _ = call.update(cx, |_, window, _| window.activate_window());
    } else if !chat_window {
        let _ = main
            .handle
            .update(cx, |_, window, cx| raise_main_window(window, cx));
    }
}

/// How often, and how many times, a window closed in full screen is asked
/// whether it left: 3 s in all, tdesktop's `kHideAfterFullscreenTimeoutMs`.
const LEFT_FULLSCREEN_POLL: std::time::Duration = std::time::Duration::from_millis(100);
const LEFT_FULLSCREEN_POLLS: u32 = 30;

/// Hide the main window, tdesktop's `closeWithoutDestroy`: on a Mac it
/// leaves full screen first, because hiding it there leaves a black Space.
/// Raising the window during that wait undoes the close, unlike tdesktop,
/// where the wait then hides the window the user just asked for.
pub(super) fn hide_main_window(window: &mut gpui_kit::Window, cx: &mut gpui_kit::App) {
    let waiting = cx
        .try_global::<MainWindow>()
        .is_some_and(|main| main.hide_after_full_screen);
    if waiting {
        return;
    }
    let leave_full_screen = cfg!(target_os = "macos") && window.is_fullscreen();
    if cx.has_global::<MainWindow>() {
        let main = cx.global_mut::<MainWindow>();
        main.closed_by_user = true;
        main.hide_after_full_screen = leave_full_screen;
    }
    if !leave_full_screen {
        hide_or_minimize(window);
        return;
    }
    window.toggle_fullscreen();
    let main = window.window_handle();
    cx.spawn(async move |cx| {
        for _ in 0..LEFT_FULLSCREEN_POLLS {
            cx.background_executor().timer(LEFT_FULLSCREEN_POLL).await;
            let left = main.update(cx, |_, window, _| {
                !window.is_fullscreen() && !super::window_control::is_exiting_fullscreen(window)
            });
            if left.unwrap_or(true) {
                break;
            }
        }
        let _ = main.update(cx, |_, window, cx| {
            if take_hide_after_full_screen(cx) {
                hide_or_minimize(window);
            }
        });
    })
    .detach();
}

fn hide_or_minimize(window: &mut gpui_kit::Window) {
    if !super::window_control::set_visible(window, false) {
        window.minimize_window();
    }
}

impl super::QuillApp {
    /// What the main window's close button and Close Window do
    /// (`quill::tray::close_outcome`). Returns whether the window closes.
    pub fn close_main_window(
        &mut self,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
    ) -> bool {
        use quill::tray::{CloseOutcome, close_outcome};
        match close_outcome(
            self.minimize_to_tray(),
            quill::tray::tray_available(),
            cfg!(target_os = "macos"),
            super::window_control::hide_supported(window),
            self.signed_in(),
        ) {
            CloseOutcome::Quit => {
                if self.exit_held_by_export(super::export_quit::quit_app, window, cx) {
                    return false;
                }
                // No account to keep running for: quit, as tdesktop does,
                // even with a PiP or a chat window still open.
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

    /// An account is signed in, on screen or in the background: closing
    /// the window on a Mac hides it ([`Self::close_main_window`]).
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
