//! The one-time "Quill closed unexpectedly last time" notice, and the
//! clean-exit marker that keeps it from showing after a normal quit
//! (tdesktop's `tdata/working` file and its "Last time Telegram Desktop
//! crashed" window, `core/crash_report_window.cpp`). Quill shows a
//! notification instead of a separate window: the report stays on this
//! computer, and "Show log" opens the folder it is in.

use gpui_kit::component::WindowExt;
use gpui_kit::component::button::Button;
use gpui_kit::component::notification::Notification;
use gpui_kit::{App, Window};

/// Call once while the main window is built: arms the clean-exit marker and
/// shows the notice after this update, once the kit root is in place.
pub(crate) fn install(window: &Window, cx: &mut App) {
    watch_clean_exit(cx);
    window.defer(cx, show_if_crashed);
}

/// Write the clean-exit marker when the app quits normally. A panic ends the
/// process without running quit observers, so the marker stays older than
/// the crash report.
fn watch_clean_exit(cx: &mut App) {
    cx.on_app_quit(|_| {
        if let Some(root) = quill::settings::safe_app_root() {
            quill::crash_log::mark_clean_exit(&root);
        }
        async {}
    })
    .detach();
}

/// Show the notice when a crash report is newer than the last clean exit.
/// Shown once: the crash is acknowledged as soon as the notice is pushed.
/// Runs outside any update of the window's root view (the kit's
/// notification layer is read by the root).
fn show_if_crashed(window: &mut Window, cx: &mut App) {
    if let Some(root) = quill::settings::safe_app_root() {
        show_for_root(&root, window, cx);
    }
}

fn show_for_root(root: &std::path::Path, window: &mut Window, cx: &mut App) {
    let Some(crash) = quill::crash_log::pending_crash(root) else {
        return;
    };
    quill::crash_log::acknowledge(root, &crash);
    window.push_notification(notice(crash.path), cx);
}

fn notice(report: std::path::PathBuf) -> Notification {
    Notification::new()
        .id::<CrashNotice>()
        .title("Quill closed unexpectedly last time")
        .message("A crash report was saved on this computer.")
        .action(move |_, _, cx| {
            let report = report.clone();
            Button::new("crash-notice-show-log")
                .label("Show log")
                .on_click(cx.listener(move |note, _, window, cx| {
                    let _ = quill::platform::reveal_in_file_manager(&report);
                    note.dismiss(window, cx);
                }))
        })
}

/// Identifies the notice so a second push replaces it.
struct CrashNotice;

#[cfg(all(test, feature = "demo-capture"))]
mod tests {
    use super::show_for_root;
    use gpui_kit::component::Root;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        AppContext as _, Context, IntoElement, Render, TestAppContext, VisualTestContext, Window,
        div, px, size,
    };

    struct Blank;

    impl Render for Blank {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    #[gpui_kit::test]
    fn a_pending_crash_shows_the_notice_once(cx: &mut TestAppContext) {
        let root = std::env::temp_dir().join(format!("quill-crash-notice-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let logs = quill::crash_log::log_dir(&root);
        quill::crash_log::write_crash_file(&logs, 3_000, "report", 3).unwrap();

        cx.update(gpui_kit::init);
        let handle = cx.open_window(size(px(800.), px(500.)), |window, cx| {
            Root::new(cx.new(|_| Blank), window, cx)
        });
        let mut vcx = VisualTestContext::from_window(handle.into(), cx);
        vcx.update(|window, cx| show_for_root(&root, window, cx));
        vcx.run_until_parked();
        vcx.update(|window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("crash-notice-show-log").is_some());
        });
        assert_eq!(quill::crash_log::pending_crash(&root), None, "shown once");
        let _ = std::fs::remove_dir_all(&root);
    }
}
