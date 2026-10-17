//! Screenshot demos: the status toast (`QUILL_DEMO_SHOW_TOAST=1` keeps it up).
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::status_note::StatusNote;
use crate::ui::user_error::user_error;
use gpui_kit::*;
use quill::connect::ConnectSendError;
use quill::lang::{keys, tr};

register_demos![
    // A confirmation toast (Info): copied text.
    DemoSpec::chats(
        "ready-status-toast-info",
        "screenshot demo — status toast: confirmation"
    )
    .setup(QuillApp::demo_status_toast_info),
    // A failure toast (Error): a request that could not reach the service,
    // worded by `user_error`, never a Debug dump.
    DemoSpec::chats(
        "ready-status-toast-error",
        "screenshot demo — status toast: failure"
    )
    .setup(QuillApp::demo_status_toast_error),
];

impl QuillApp {
    fn demo_status_toast_info(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.connection
            .set_note(StatusNote::info(tr(keys::TEXT_COPIED)));
    }

    fn demo_status_toast_error(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.connection
            .set_note(StatusNote::error(user_error(&ConnectSendError::Native)));
    }
}
