//! Links, activation and quit requests from outside the window.
#![deny(clippy::indexing_slicing)]

/// Drains links that arrived from the OS or a second launch: raises the
/// window and hands the next link to the deep-link pump (one flow at a time).
pub fn install_link_inbox(
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::App,
    view: &gpui_kit::Entity<super::QuillApp>,
) {
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
                            // The poll loop starts the flow.
                            quill::wake::UPDATES.poke();
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
