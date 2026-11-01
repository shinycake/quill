#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::link_inbox::raise_main_window;
use gpui_kit::component::button::ButtonVariant;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::state::Session;
use std::time::{Duration, Instant};

pub(crate) type ExitAfterStop = Box<dyn Fn(&mut Window, &mut App)>;

fn exporting(session: &Session) -> bool {
    let settings = &session.settings;
    let chat = settings.chat_export.as_ref();
    let account = settings.account_export.as_ref();
    chat.is_some_and(|export| !export.progress().is_settled())
        || account.is_some_and(|export| !export.progress().is_settled())
        || (settings.rich_html_exports.iter()).any(|export| export.outcome().is_none())
}

impl QuillApp {
    fn all_sessions(&self) -> impl Iterator<Item = &Session> {
        let background = self.background.clients.iter();
        self.session().into_iter().chain(
            background.filter_map(|client| client.live.as_ref().map(|live| &live.driver.session)),
        )
    }

    fn stop_exports(&self) {
        self.all_sessions()
            .flat_map(|session| session.settings.export_links())
            .for_each(|link| link.cancel());
    }

    pub(in crate::ui) fn stop_exports_and_wait(&self, until: Instant) {
        self.stop_exports();
        while self.all_sessions().any(exporting) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
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
        if self.dialogs.export_stop.replace(Box::new(exit)).is_some() {
            return true;
        }
        let app = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (stop, closed) = (app.clone(), app.clone());
            alert
                .description(tr(keys::EXPORT_SURE_STOP))
                .ok_text(tr(keys::EXPORT_STOP))
                .ok_variant(ButtonVariant::Danger)
                .cancel_text(tr(keys::CANCEL))
                .show_cancel(true)
                .on_ok(move |_, window, cx| {
                    let exit = stop.update(cx, |this, _| {
                        this.stop_exports();
                        this.dialogs.export_stop.take()
                    });
                    if let Ok(Some(exit)) = exit {
                        exit(window, cx);
                    }
                    true
                })
                .on_close(move |_, _, cx| {
                    let _ = closed.update(cx, |this, _| this.dialogs.export_stop = None);
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

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "export_quit_tests.rs"]
mod tests;
