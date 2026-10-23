//! "Upload Sound" in the sound picker (tdesktop `RingtonesBox`): the button
//! opens the system file dialog, the chosen file shows as a row while it
//! uploads, and the new sound is selected when it lands. A saved sound's
//! context menu deletes it.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use quill::lang::{keys, tr};
use quill::tone_upload::{ToneProblem, ToneUpload, check_file, display_name, problem_message};

impl QuillApp {
    /// The picker's "Upload Sound" button, and the row of the file in
    /// flight above it.
    pub(super) fn tone_upload_footer(
        &self,
        cx: &mut Context<Self>,
        target: SoundPickerTarget,
        list_id: &str,
    ) -> Vec<AnyElement> {
        let uploading = match self.session().map(|s| &s.settings.tone_upload) {
            Some(ToneUpload::Uploading { name }) => Some(name.clone()),
            _ => None,
        };
        let mut footer = Vec::new();
        if let Some(name) = &uploading {
            footer.push(
                div()
                    .id(format!("{list_id}-pending"))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(Radio::new(format!("{list_id}-pending-radio")).disabled(true))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .child(div().text_sm().child(name.clone()))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(tr(keys::Q_TONE_UPLOADING)),
                                    ),
                            ),
                    )
                    .child(gpui_kit::component::spinner::Spinner::new().small())
                    .into_any_element(),
            );
        }
        footer.push(
            div()
                .id(format!("{list_id}-upload-row"))
                .px_2()
                .py_1()
                .child(
                    Button::new(format!("{list_id}-upload"))
                        .small()
                        .outline()
                        .icon(gpui_kit::assets::IconName::Plus)
                        .label(tr(keys::RINGTONES_BOX_UPLOAD_BUTTON))
                        .disabled(uploading.is_some())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.pick_notification_tone(target, cx);
                        })),
                )
                .into_any_element(),
        );
        footer
    }

    /// The system file dialog; the chosen file goes up for `target`.
    pub(in crate::ui) fn pick_notification_tone(
        &mut self,
        target: SoundPickerTarget,
        cx: &mut Context<Self>,
    ) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(tr(keys::RINGTONES_BOX_UPLOAD_CHOOSE).into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picker.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update(cx, |this, cx| this.begin_tone_upload(target, &path, cx));
            }
        })
        .detach();
    }

    /// Check the file and send it; a refusal is told at once, as
    /// tdesktop's inform box does.
    pub(in crate::ui) fn begin_tone_upload(
        &mut self,
        target: SoundPickerTarget,
        path: &std::path::Path,
        cx: &mut Context<Self>,
    ) {
        let started = if let Some(live) = self.live.as_mut() {
            live.driver.upload_notification_tone(path).map(|_| ())
        } else if let Some(session) = self.demo_session.as_mut() {
            let size = std::fs::metadata(path).map_or(0, |meta| meta.len());
            check_file(path, size, session.messages.tone_limits).map(|()| {
                session.settings.tone_upload = ToneUpload::Uploading {
                    name: display_name(path),
                };
            })
        } else {
            Err(ToneProblem::Unreadable)
        };
        match started {
            Ok(()) => self.notify.tone_upload_target = Some(target),
            Err(problem) => self.tell_tone_problem(problem),
        }
        cx.notify();
    }

    /// React to an upload that ended: select the new sound where it was
    /// asked for, or tell why it was refused.
    pub(in crate::ui) fn settle_tone_upload(&mut self, cx: &mut Context<Self>) {
        let outcome = match self.live.as_mut() {
            Some(live) => live.driver.take_tone_upload_outcome(),
            None => self
                .demo_session
                .as_mut()
                .filter(|s| s.settings.tone_upload.is_settled())
                .map(|s| std::mem::take(&mut s.settings.tone_upload)),
        };
        let Some(outcome) = outcome else {
            return;
        };
        let target = self.notify.tone_upload_target.take();
        match (outcome, target) {
            (ToneUpload::Added { sound_id }, Some(target)) => {
                self.apply_sound_choice(target, SoundChoice::Custom(sound_id), cx);
                self.connection
                    .set_note(StatusNote::info(tr(keys::RINGTONES_TOAST_ADDED)));
            }
            (ToneUpload::Added { .. }, None) => {
                self.connection
                    .set_note(StatusNote::info(tr(keys::RINGTONES_TOAST_ADDED)));
            }
            (ToneUpload::Failed(problem), _) => self.tell_tone_problem(problem),
            (ToneUpload::Idle | ToneUpload::Uploading { .. }, _) => {}
        }
        cx.notify();
    }

    fn tell_tone_problem(&mut self, problem: ToneProblem) {
        let limits = self
            .session()
            .map(|s| s.messages.tone_limits)
            .unwrap_or_default();
        self.connection
            .set_note(StatusNote::error(problem_message(problem, limits)));
    }

    /// A saved sound's row menu: tdesktop's right click offers "Delete".
    pub(super) fn with_tone_menu(
        &self,
        cx: &mut Context<Self>,
        row: Stateful<Div>,
        sound_id: i64,
    ) -> AnyElement {
        let owner = cx.entity().downgrade();
        row.context_menu(move |menu, _, _| {
            let owner = owner.clone();
            menu.item(
                PopupMenuItem::new(tr(keys::BOX_DELETE)).on_click(move |_, _, cx| {
                    let _ = owner.update(cx, |this, cx| {
                        if let Some(live) = this.live.as_mut() {
                            let _ = live.driver.remove_notification_tone(sound_id);
                        }
                        cx.notify();
                    });
                }),
            )
        })
        .into_any_element()
    }
}
