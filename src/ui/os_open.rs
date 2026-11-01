//! The way back from an open handed to the OS: no note says "Opened" until
//! the OS answered.

use super::app::QuillApp;
use gpui_kit::Context;
use quill::platform::OsOpen;

impl QuillApp {
    pub(super) fn when_os_answers(
        &mut self,
        open: OsOpen,
        cx: &mut Context<Self>,
        answered: impl FnOnce(&mut Self, bool, &mut Context<Self>) + 'static,
    ) {
        match open {
            OsOpen::Answered(opened) => answered(self, opened, cx),
            OsOpen::Pending(answer) => Self::spawn_main(cx, async move |app, cx| {
                let opened = answer.await.unwrap_or(false);
                let _ = app.update(cx, |this, cx| answered(this, opened, cx));
            })
            .detach(),
        }
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "os_open_tests.rs"]
mod tests;
