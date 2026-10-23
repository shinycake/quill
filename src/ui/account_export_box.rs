//! The "Export Telegram data" box (tdesktop `export/view/export_view_settings`
//! for a full export): account information, contacts, stories, profile
//! music, the chat types with "Only my messages", media kinds and a size
//! limit, active sessions, then the folder and format. Once the export runs
//! the same dialog shows its progress (`account_export_progress.rs`).
//! Opened from Settings › Data & Storage.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::export_box::default_folder;
use super::folder_share::finish_dialog;
use super::shell::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::export::account::{AccountSettings, Section};
use quill::export::settings::{SIZE_STEPS, size_index_for, size_limit_by_index, size_limit_label};
use quill::export::{ExportFormat, MediaKind};
use quill::lang::{keys, tr, tr_with};

/// The box's choices while it is open.
#[derive(Clone)]
pub(super) struct AccountExportDraft {
    pub settings: AccountSettings,
    size_slider: Entity<SliderState>,
    error: Option<String>,
}

/// The sections above the chat types, each with its line of explanation.
const ACCOUNT_SECTIONS: [Section; 4] = [
    Section::PersonalInfo,
    Section::Contacts,
    Section::Stories,
    Section::ProfileMusic,
];

/// tdesktop's format names in this box.
fn format_label(format: ExportFormat) -> &'static str {
    match format {
        ExportFormat::Html => tr(keys::EXPORT_OPTION_HTML),
        ExportFormat::Json => tr(keys::EXPORT_OPTION_JSON),
        ExportFormat::HtmlAndJson => tr(keys::EXPORT_OPTION_HTML_AND_JSON),
    }
}

impl QuillApp {
    /// Open the box. While an export is running it shows that export.
    pub(super) fn open_account_export(&mut self, cx: &mut Context<Self>) {
        if self
            .session()
            .is_some_and(|s| s.settings.account_export.is_some())
        {
            self.dialogs.account_export_progress_hidden = false;
            cx.notify();
            return;
        }
        let settings = AccountSettings::new(default_folder());
        let slider = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max((SIZE_STEPS - 1) as f32)
                .step(1.)
                .default_value(size_index_for(settings.size_limit) as f32)
        });
        cx.subscribe(&slider, |this, _, event: &SliderEvent, cx| {
            let (SliderEvent::Change(value) | SliderEvent::Release(value)) = event;
            if let Some(draft) = this.dialogs.account_export_dialog.as_mut() {
                draft.settings.size_limit = size_limit_by_index(value.end().round() as usize);
            }
            cx.notify();
        })
        .detach();
        self.dialogs.account_export_dialog = Some(AccountExportDraft {
            settings,
            size_slider: slider,
            error: None,
        });
        cx.notify();
    }

    fn edit_account_export(
        &mut self,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut AccountSettings),
    ) {
        if let Some(draft) = self.dialogs.account_export_dialog.as_mut() {
            edit(&mut draft.settings);
            draft.error = None;
        }
        cx.notify();
    }

    /// Start the export with the box's choices; progress shows in this
    /// same dialog.
    fn run_account_export(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.dialogs.account_export_dialog.as_mut() else {
            return;
        };
        let started = match self.live.as_mut() {
            Some(live) => live.driver.start_account_export(draft.settings.clone()),
            None => Err(std::io::Error::other(tr(
                keys::Q_OPS_CONNECT_TO_EXPORT_YOUR_DATA,
            ))),
        };
        match started {
            Ok(()) => {
                self.dialogs.account_export_dialog = None;
                self.dialogs.account_export_progress_hidden = false;
            }
            Err(error) => draft.error = Some(error.to_string()),
        }
        cx.notify();
    }

    fn choose_account_export_folder(&mut self, cx: &mut Context<Self>) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(tr(keys::Q_OPS_EXPORT_HERE).into()),
        });
        cx.spawn(async move |this, cx| {
            let result = picker.await;
            let _ = this.update(cx, |this, cx| {
                if let Ok(Ok(Some(paths))) = result
                    && let Some(folder) = paths.first()
                    && let Some(draft) = this.dialogs.account_export_dialog.as_mut()
                {
                    draft.settings.dir = folder.clone();
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn build_account_export_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::AccountExport, |this, _, cx| {
                this.dialogs.account_export_dialog = None;
                // A running export keeps going; its result shows as a note.
                this.dialogs.account_export_progress_hidden = true;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            match this.dialogs.account_export_dialog.clone() {
                None => {
                    let (body, footer) = this.account_export_progress(cx);
                    finish_dialog(
                        dialog,
                        tr(keys::EXPORT_PROGRESS_TITLE).into(),
                        body,
                        Some(footer),
                        on_close,
                    )
                }
                Some(draft) => {
                    let (body, footer) = this.account_export_options(&draft, cx);
                    finish_dialog(
                        dialog,
                        tr(keys::EXPORT_TITLE).into(),
                        body,
                        Some(footer),
                        on_close,
                    )
                }
            }
        })
    }

    fn account_export_options(
        &self,
        draft: &AccountExportDraft,
        cx: &mut Context<Self>,
    ) -> (AnyElement, AnyElement) {
        let settings = &draft.settings;
        let muted = cx.theme().muted_foreground;
        let heading = |text: &'static str| {
            div()
                .id(SharedString::from(format!("account-export-heading-{text}")))
                .role(Role::Heading)
                .aria_label(text)
                .pt_2()
                .text_sm()
                .font_semibold()
                .child(text)
        };
        let about = |text: &'static str| div().pl_6().text_xs().text_color(muted).child(text);
        let option = |section: Section| {
            Checkbox::new(SharedString::from(format!("account-export-{section:?}")))
                .label(section.label())
                .checked(settings.is_on(section))
                .on_click(cx.listener(move |this, &on, _, cx| {
                    this.edit_account_export(cx, |s| s.set(section, on));
                }))
        };
        let mut body = div().id("account-export").flex().flex_col().gap_2();
        for section in ACCOUNT_SECTIONS {
            body = body.child(option(section));
            if let Some(text) = section.about() {
                body = body.child(about(text));
            }
        }
        body = body.child(heading(tr(keys::EXPORT_HEADER_CHATS)));
        for section in Section::CHATS {
            body = body.child(option(section));
            if AccountSettings::offers_only_my(section) && settings.is_on(section) {
                let locked = AccountSettings::only_my_locked(section);
                body = body.child(
                    div().pl_6().child(
                        Checkbox::new(SharedString::from(format!(
                            "account-export-only-my-{section:?}"
                        )))
                        .label(tr(keys::EXPORT_OPTION_ONLY_MY))
                        .accessibility_label(tr_with(
                            keys::Q_LEFTOVERS_ONLY_MY_MESSAGES_IN,
                            &[("section", &(section.label().to_lowercase()).to_string())],
                        ))
                        .checked(settings.only_my(section))
                        .disabled(locked)
                        .on_click(cx.listener(
                            move |this, &on, _, cx| {
                                this.edit_account_export(cx, |s| s.set_only_my(section, on));
                            },
                        )),
                    ),
                );
            }
        }
        if settings.shows_media() {
            body = body.child(heading(tr(keys::EXPORT_HEADER_MEDIA)));
            let mut media = div().flex().flex_wrap().gap_x_4().gap_y_2();
            for (i, kind) in MediaKind::ALL.into_iter().enumerate() {
                media = media.child(
                    Checkbox::new(("account-export-media", i))
                        .label(kind.label())
                        .checked(settings.media.contains(kind))
                        .on_click(cx.listener(move |this, &on, _, cx| {
                            this.edit_account_export(cx, |s| s.media = s.media.with(kind, on));
                        })),
                );
            }
            let size_label = tr_with(
                keys::EXPORT_OPTION_SIZE_LIMIT,
                &[("size", &(size_limit_label(settings.size_limit)).to_string())],
            );
            body = body.child(media).child(
                div()
                    .id("account-export-size")
                    .role(Role::Group)
                    .aria_label(size_label.clone())
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .flex_1()
                            .child(Slider::new(&draft.size_slider).bg(cx.theme().primary)),
                    )
                    .child(div().text_sm().child(size_label)),
            );
        }
        body = body
            .child(heading(tr(keys::EXPORT_HEADER_OTHER)))
            .child(option(Section::Sessions));
        if let Some(text) = Section::Sessions.about() {
            body = body.child(about(text));
        }
        let format = RadioGroup::vertical("account-export-format")
            .selected_index(ExportFormat::ALL.iter().position(|f| *f == settings.format))
            .children(ExportFormat::ALL.iter().enumerate().map(|(i, f)| {
                Radio::new(("account-export-format-item", i)).label(format_label(*f))
            }))
            .on_click(cx.listener(|this, &ix, _, cx| {
                if let Some(&format) = ExportFormat::ALL.get(ix) {
                    this.edit_account_export(cx, |s| s.format = format);
                }
            }));
        let folder = div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_sm()
                    .child(settings.dir.display().to_string()),
            )
            .child(
                Button::new("account-export-folder")
                    .label(tr(keys::Q_OPS_CHANGE))
                    .outline()
                    .small()
                    .accessibility_label(tr(keys::Q_OPS_CHOOSE_THE_EXPORT_FOLDER))
                    .on_click(cx.listener(|this, _, _, cx| this.choose_account_export_folder(cx))),
            );
        body = body
            .child(heading(tr(keys::EXPORT_HEADER_FORMAT)))
            .child(folder)
            .child(format);
        if let Some(error) = &draft.error {
            body = body.child(
                div()
                    .id("account-export-error")
                    .role(Role::Alert)
                    .aria_label(error.clone())
                    .text_xs()
                    .text_color(cx.theme().danger)
                    .child(error.clone()),
            );
        }
        let footer = div()
            .flex()
            .justify_end()
            .gap_2()
            .child(
                Button::new("account-export-cancel")
                    .label(tr(keys::CANCEL))
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.dialogs.account_export_dialog = None;
                        cx.notify();
                        this.close_kit_dialog_if_done(DialogKind::AccountExport, window, cx);
                    })),
            )
            .child(
                Button::new("account-export-start")
                    .label(tr(keys::EXPORT_START))
                    .primary()
                    .disabled(!settings.can_start())
                    .on_click(cx.listener(|this, _, _, cx| this.run_account_export(cx))),
            );
        (body.into_any_element(), footer.into_any_element())
    }
}

crate::ui::shell::register_dialogs! {
    /// The "Export Telegram data" box, and the running export's progress.
    AccountExport => DialogSpec::new(
        1510,
        |app| {
            app.dialogs.account_export_dialog.is_some()
                || (!app.dialogs.account_export_progress_hidden
                    && app.session().is_some_and(|s| s.settings.account_export.is_some()))
        },
        QuillApp::build_account_export_dialog,
    ),
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "account_export_box_tests.rs"]
mod tests;
