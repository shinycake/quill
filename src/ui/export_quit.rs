//! macOS's Dock menu and an OS logout terminate the app without passing
//! through here: GPUI offers no way to refuse them.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::link_inbox::raise_main_window;
use gpui_kit::component::button::ButtonVariant;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::state::Session;
use std::rc::Rc;

fn exporting(session: &Session) -> bool {
    let settings = &session.settings;
    let chat = settings.chat_export.as_ref();
    let account = settings.account_export.as_ref();
    chat.is_some_and(|export| !export.progress().is_settled())
        || account.is_some_and(|export| !export.progress().is_settled())
}

impl QuillApp {
    fn all_sessions(&self) -> impl Iterator<Item = &Session> {
        let background = self.background.clients.iter();
        self.session().into_iter().chain(
            background.filter_map(|client| client.live.as_ref().map(|live| &live.driver.session)),
        )
    }

    fn stop_exports(&self) {
        for session in self.all_sessions() {
            if let Some(export) = &session.settings.chat_export {
                export.cancel();
            }
            if let Some(export) = &session.settings.account_export {
                export.cancel();
            }
        }
    }

    pub(crate) fn exit_held_by_export(
        &mut self,
        exit: impl Fn(&mut Window, &mut App) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.all_sessions().any(exporting) {
            return false;
        }
        raise_main_window(window, cx);
        if self.dialogs.export_stop_open {
            return true;
        }
        self.dialogs.export_stop_open = true;
        let app = cx.entity().downgrade();
        let exit = Rc::new(exit);
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (stop, closed, exit) = (app.clone(), app.clone(), exit.clone());
            alert
                .description(tr(keys::EXPORT_SURE_STOP))
                .ok_text(tr(keys::EXPORT_STOP))
                .ok_variant(ButtonVariant::Danger)
                .cancel_text(tr(keys::CANCEL))
                .show_cancel(true)
                .on_ok(move |_, window, cx| {
                    let _ = stop.update(cx, |this, _| this.stop_exports());
                    exit(window, cx);
                    true
                })
                .on_close(move |_, _, cx| {
                    let _ = closed.update(cx, |this, _| this.dialogs.export_stop_open = false);
                })
        });
        true
    }

    pub(crate) fn exit_after_exports(
        &mut self,
        exit: impl Fn(&mut Window, &mut App) + Clone + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.exit_held_by_export(exit.clone(), window, cx) {
            exit(window, cx);
        }
    }
}

pub(crate) fn quit_app(window: &mut Window, cx: &mut App) {
    window.remove_window();
    cx.quit();
}

/// Closing the main window, where that ends the app: macOS keeps a
/// windowless app alive for its menu bar; elsewhere closing the only
/// window quits.
pub(crate) fn close_main_window(window: &mut Window, cx: &mut App) {
    window.remove_window();
    #[cfg(not(target_os = "macos"))]
    cx.quit();
    #[cfg(target_os = "macos")]
    let _ = cx;
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "export_quit_tests.rs"]
mod tests;
