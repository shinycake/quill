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

/// The main window, for a Dock click while no window shows
/// ([`reopen_main_window`]).
#[derive(Clone, Copy)]
struct MainWindow(gpui_kit::AnyWindowHandle);

impl gpui_kit::Global for MainWindow {}

/// Note the main window for [`reopen_main_window`].
pub fn note_main_window(window: &gpui_kit::Window, cx: &mut gpui_kit::App) {
    cx.set_global(MainWindow(window.window_handle()));
}

/// macOS calls this for a Dock click while the app shows no window: the
/// closed main window was only hidden ([`close_main_window`]).
pub fn reopen_main_window(cx: &mut gpui_kit::App) {
    if let Some(MainWindow(main)) = cx.try_global::<MainWindow>().copied() {
        let _ = main.update(cx, |_, window, cx| raise_main_window(window, cx));
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
        CloseOutcome::HideApp => {
            cx.hide();
            false
        }
        CloseOutcome::Hide => {
            if !super::window_control::set_visible(window, false) {
                window.minimize_window();
            }
            false
        }
        CloseOutcome::Minimize => {
            window.minimize_window();
            false
        }
    }
}

impl super::QuillApp {
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
