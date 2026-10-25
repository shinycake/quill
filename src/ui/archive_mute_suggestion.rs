//! "Hide new chats?" (tdesktop `SessionController::suggestArchiveAndMute`):
//! when the server suggests archiving and muting new chats from
//! non-contacts and the setting is off, a box offers the Privacy and
//! Security settings. Closing it in any way dismisses the suggestion.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::folder_share::finish_dialog;
use super::navigation::NavigationAction;
use super::screen_chooser::bold_marked;
use super::shell::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::*;
use quill::archive_mute::{ACTION, suggestion_due};
use quill::lang::{keys, tr};

impl QuillApp {
    pub(super) fn archive_mute_suggested(&self) -> bool {
        self.session().is_some_and(|s| {
            suggestion_due(
                &s.chat_list.suggestions.actions,
                s.chat_list.archive_chat_list_settings.as_ref(),
            )
        })
    }

    /// `hideSuggestedAction`, as tdesktop's `boxClosing` does.
    pub(super) fn dismiss_archive_mute_suggestion(&mut self, cx: &mut Context<Self>) {
        match self.live.as_mut() {
            Some(live) => {
                if live.driver.hide_suggested_action(ACTION).is_err() {
                    self.connection.set_note(StatusNote::error(tr(
                        keys::Q_MISC_COULD_NOT_HIDE_THE_SUGGESTION,
                    )));
                }
            }
            None => {
                if let Some(demo) = self.demo_session.as_mut() {
                    demo.chat_list.suggestions.actions.remove(ACTION);
                }
            }
        }
        cx.notify();
    }

    fn build_archive_mute_suggestion_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close = QuillShell::on_close_kind(
            app,
            shell,
            DialogKind::ArchiveMuteSuggestion,
            |this, _, cx| this.dismiss_archive_mute_suggestion(cx),
        );
        app.update(cx, |_, cx| {
            let body = div()
                .id("archive-mute-suggestion")
                .debug_selector(|| "archive-mute-suggestion".into())
                .child(bold_marked(tr(keys::SUGGEST_HIDE_NEW_ABOUT)));
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("archive-mute-suggestion-cancel")
                        .debug_selector(|| "archive-mute-suggestion-cancel".into())
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.dismiss_archive_mute_suggestion(cx);
                            this.close_kit_dialog_if_done(
                                DialogKind::ArchiveMuteSuggestion,
                                window,
                                cx,
                            );
                        })),
                )
                .child(
                    Button::new("archive-mute-suggestion-settings")
                        .debug_selector(|| "archive-mute-suggestion-settings".into())
                        .label(tr(keys::SUGGEST_HIDE_NEW_TO_SETTINGS))
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.dismiss_archive_mute_suggestion(cx);
                            this.close_kit_dialog_if_done(
                                DialogKind::ArchiveMuteSuggestion,
                                window,
                                cx,
                            );
                            this.navigate(NavigationAction::Privacy, window, cx);
                        })),
                );
            finish_dialog(
                dialog,
                tr(keys::SUGGEST_HIDE_NEW_TITLE).to_string(),
                body.into_any_element(),
                Some(footer.into_any_element()),
                on_close,
            )
            .overlay_closable(false)
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// The server's "archive and mute new chats" suggestion.
    ArchiveMuteSuggestion => DialogSpec::new(
        1420,
        QuillApp::archive_mute_suggested,
        QuillApp::build_archive_mute_suggestion_dialog,
    ),
}

crate::ui::screenshot_demo::register_demos![
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-archive-mute-suggestion").setup(
        |app, _, _| {
            if let Some(demo) = app.demo_session.as_mut() {
                demo.chat_list
                    .suggestions
                    .actions
                    .insert(ACTION.to_string());
                demo.chat_list.archive_chat_list_settings = Some(Default::default());
            }
        }
    ),
];

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "archive_mute_suggestion_tests.rs"]
mod tests;
