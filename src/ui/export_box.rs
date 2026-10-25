//! The "Export chat history" box (tdesktop `export/view/export_view_settings`
//! for one chat): media kinds, size limit, format, folder and a date range,
//! then Export. Once the export runs, the same dialog shows its progress
//! (`export_progress.rs`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::folder_share::finish_dialog;
use super::shell::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::export::settings::{
    SIZE_STEPS, date_label, size_index_for, size_limit_by_index, size_limit_label,
};
use quill::export::{ExportFormat, ExportSettings, MediaKind};
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use quill::local_time::now_unix;
use quill::search_filters::{YearMonth, day_number, local_day_number};
use std::path::PathBuf;

/// Which end of the date range the calendar is picking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DateField {
    From,
    Till,
}

/// The box's choices while it is open.
#[derive(Clone)]
pub(super) struct ChatExportDraft {
    pub settings: ExportSettings,
    size_slider: Entity<SliderState>,
    /// The calendar is open for this end of the range.
    pub(super) picking: Option<DateField>,
    month: YearMonth,
    error: Option<String>,
}

/// Telegram launched on 2013-08-14; nothing is older.
const EARLIEST: YearMonth = YearMonth {
    year: 2013,
    month: 8,
};
const CELL: f32 = 34.;

pub(super) fn default_folder() -> PathBuf {
    quill::file_prefs::configured_download_dir()
        .or_else(|| directories::UserDirs::new().and_then(|d| d.download_dir().map(PathBuf::from)))
        .unwrap_or_else(std::env::temp_dir)
}

impl QuillApp {
    /// Open the options box for `chat_id`; the export starts from its button.
    /// While an export is running the box shows that export instead.
    pub(super) fn start_chat_export(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        // No bulk export for an agent driving the window.
        if quill::agent::ui_driver::guard::refuse_if_injecting("export a chat") {
            return;
        }
        if self
            .session()
            .is_some_and(|s| s.settings.chat_export.is_some())
        {
            self.dialogs.chat_export_progress_hidden = false;
            cx.notify();
            return;
        }
        let title = self
            .session()
            .and_then(|s| s.chats.get(&chat_id.0))
            .map(|c| c.title.clone())
            .unwrap_or_else(|| "chat".to_string());
        let settings = ExportSettings::new(chat_id.0, title, default_folder());
        let slider = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max((SIZE_STEPS - 1) as f32)
                .step(1.)
                .default_value(size_index_for(settings.size_limit) as f32)
        });
        cx.subscribe(&slider, |this, _, event: &SliderEvent, cx| {
            let (SliderEvent::Change(value) | SliderEvent::Release(value)) = event;
            if let Some(draft) = this.dialogs.chat_export_dialog.as_mut() {
                draft.settings.size_limit = size_limit_by_index(value.end().round() as usize);
            }
            cx.notify();
        })
        .detach();
        self.dialogs.chat_export_dialog = Some(ChatExportDraft {
            settings,
            size_slider: slider,
            picking: None,
            month: YearMonth::of(now_unix()),
            error: None,
        });
        cx.notify();
    }

    /// Start the export with the box's choices. The worker thread pages
    /// history and copies media; progress shows in this same dialog.
    fn run_chat_export(&mut self, cx: &mut Context<Self>) {
        if quill::agent::ui_driver::guard::refuse_if_injecting("export a chat") {
            return;
        }
        let Some(draft) = self.dialogs.chat_export_dialog.as_mut() else {
            return;
        };
        let settings = draft.settings.clone();
        let started = match self.live.as_mut() {
            Some(live) => live.driver.start_chat_export(settings),
            None => Err(std::io::Error::other(tr(
                keys::Q_OPS_CONNECT_TO_EXPORT_A_CHAT,
            ))),
        };
        match started {
            Ok(()) => {
                self.dialogs.chat_export_dialog = None;
                self.dialogs.chat_export_progress_hidden = false;
            }
            Err(error) => draft.error = Some(error.to_string()),
        }
        cx.notify();
    }

    fn choose_chat_export_folder(&mut self, cx: &mut Context<Self>) {
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
                    && let Some(draft) = this.dialogs.chat_export_dialog.as_mut()
                {
                    draft.settings.dir = folder.clone();
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn build_chat_export_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::ChatExport, |this, _, cx| {
                this.dialogs.chat_export_dialog = None;
                // A running export keeps going; its result shows as a note.
                this.dialogs.chat_export_progress_hidden = true;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let Some(draft) = this.dialogs.chat_export_dialog.clone() else {
                let (body, footer) = this.chat_export_progress(cx);
                return finish_dialog(
                    dialog,
                    tr(keys::PROFILE_EXPORT_CHAT).into(),
                    body,
                    Some(footer),
                    on_close,
                );
            };
            let (title, body, footer) = match draft.picking {
                Some(field) => this.chat_export_calendar(&draft, field, cx),
                None => this.chat_export_options(&draft, cx),
            };
            finish_dialog(dialog, title, body, Some(footer), on_close)
        })
    }

    fn chat_export_options(
        &self,
        draft: &ChatExportDraft,
        cx: &mut Context<Self>,
    ) -> (String, AnyElement, AnyElement) {
        let settings = &draft.settings;
        let muted = cx.theme().muted_foreground;
        let heading = |text: &'static str| {
            div()
                .id(format!("chat-export-heading-{text}"))
                .role(Role::Heading)
                .aria_label(text)
                .text_sm()
                .font_semibold()
                .child(text)
        };
        let mut media = div().flex().flex_wrap().gap_x_4().gap_y_1();
        for (i, kind) in MediaKind::ALL.into_iter().enumerate() {
            media = media.child(
                Checkbox::new(("chat-export-media", i))
                    .label(kind.label())
                    .checked(settings.media.contains(kind))
                    .on_click(cx.listener(move |this, &on, _, cx| {
                        if let Some(draft) = this.dialogs.chat_export_dialog.as_mut() {
                            draft.settings.media = draft.settings.media.with(kind, on);
                        }
                        cx.notify();
                    })),
            );
        }
        let size = div()
            .id("chat-export-size")
            .role(Role::Group)
            .aria_label(tr(keys::Q_OPS_MEDIA_SIZE_LIMIT))
            .flex()
            .items_center()
            .gap_3()
            .child(
                div()
                    .flex_1()
                    .child(Slider::new(&draft.size_slider).bg(cx.theme().primary)),
            )
            .child(
                div()
                    .w(px(64.))
                    .text_sm()
                    .text_right()
                    .child(size_limit_label(settings.size_limit)),
            );
        let format = RadioGroup::horizontal("chat-export-format")
            .selected_index(ExportFormat::ALL.iter().position(|f| *f == settings.format))
            .children(
                ExportFormat::ALL
                    .iter()
                    .enumerate()
                    .map(|(i, f)| Radio::new(("chat-export-format-item", i)).label(f.label())),
            )
            .on_click(cx.listener(|this, &ix, _, cx| {
                if let Some(draft) = this.dialogs.chat_export_dialog.as_mut()
                    && let Some(&format) = ExportFormat::ALL.get(ix)
                {
                    draft.settings.format = format;
                }
                cx.notify();
            }));
        let date_row = |field: DateField, label: &'static str, value: i64, empty: &'static str| {
            let text = if value > 0 {
                date_label(if field == DateField::Till {
                    value - 1
                } else {
                    value
                })
            } else {
                empty.to_string()
            };
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().w(px(48.)).text_sm().text_color(muted).child(label))
                .child(
                    Button::new(("chat-export-date", field as usize))
                        .label(text)
                        .outline()
                        .small()
                        .accessibility_label(tr_with(
                            keys::Q_OPS_CHOOSE_THE_DATE,
                            &[("to_lowercase", &(label.to_lowercase()).to_string())],
                        ))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(draft) = this.dialogs.chat_export_dialog.as_mut() {
                                let shown = match field {
                                    DateField::From => draft.settings.from,
                                    DateField::Till => draft.settings.till - 1,
                                };
                                draft.month =
                                    YearMonth::of(if shown > 0 { shown } else { now_unix() });
                                draft.picking = Some(field);
                            }
                            cx.notify();
                        })),
                )
                .when(value > 0, |row| {
                    row.child(
                        Button::new(("chat-export-date-clear", field as usize))
                            .label(tr(keys::Q_OPS_CLEAR))
                            .ghost()
                            .small()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(draft) = this.dialogs.chat_export_dialog.as_mut() {
                                    match field {
                                        DateField::From => draft.settings.from = 0,
                                        DateField::Till => draft.settings.till = 0,
                                    }
                                }
                                cx.notify();
                            })),
                    )
                })
        };
        let only_mine = div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                Switch::new("chat-export-only-mine")
                    .checked(settings.only_mine)
                    .accessibility_label(tr(keys::EXPORT_OPTION_ONLY_MY))
                    .on_click(cx.listener(|this, &on, _, cx| {
                        if let Some(draft) = this.dialogs.chat_export_dialog.as_mut() {
                            draft.settings.only_mine = on;
                        }
                        cx.notify();
                    })),
            )
            .child(div().text_sm().child(tr(keys::EXPORT_OPTION_ONLY_MY)));
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
                Button::new("chat-export-folder")
                    .label(tr(keys::Q_OPS_CHANGE))
                    .outline()
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| this.choose_chat_export_folder(cx))),
            );
        let mut body = div()
            .id("chat-export")
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_sm().text_color(muted).child(tr_with(
                keys::Q_OPS_SAVE_THE_HISTORY_OF_AS,
                &[("title", &settings.title.to_string())],
            )))
            .child(heading(tr(keys::Q_OPS_MEDIA)))
            .child(media)
            .child(size)
            .child(heading(tr(keys::Q_OPS_FORMAT)))
            .child(format)
            .child(heading(tr(keys::Q_OPS_FOLDER)))
            .child(folder)
            .child(heading(tr(keys::Q_OPS_PERIOD)))
            .child(date_row(
                DateField::From,
                tr(keys::Q_OPS_FROM),
                settings.from,
                tr(keys::Q_OPS_BEGINNING),
            ))
            .child(date_row(
                DateField::Till,
                tr(keys::Q_OPS_TO),
                settings.till,
                tr(keys::Q_OPS_NOW),
            ))
            .child(only_mine);
        if let Some(error) = &draft.error {
            body = body.child(
                div()
                    .id("chat-export-error")
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
                Button::new("chat-export-cancel")
                    .label(tr(keys::CANCEL))
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.dialogs.chat_export_dialog = None;
                        cx.notify();
                        this.close_kit_dialog_if_done(DialogKind::ChatExport, window, cx);
                    })),
            )
            .child(
                Button::new("chat-export-start")
                    .label(tr(keys::EXPORT_START))
                    .on_click(cx.listener(|this, _, _, cx| this.run_chat_export(cx))),
            );
        (
            tr(keys::PROFILE_EXPORT_CHAT).into(),
            body.into_any_element(),
            footer.into_any_element(),
        )
    }

    /// The month grid for choosing one end of the date range.
    fn chat_export_calendar(
        &self,
        draft: &ChatExportDraft,
        field: DateField,
        cx: &mut Context<Self>,
    ) -> (String, AnyElement, AnyElement) {
        let theme = cx.theme().clone();
        let today = local_day_number(now_unix());
        let month = draft.month;
        let current = YearMonth::of(now_unix());
        let selected = match field {
            DateField::From => draft.settings.from,
            DateField::Till => draft.settings.till - 1,
        };
        let selected_day = (selected > 0).then(|| local_day_number(selected));
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .child(
                Button::new("chat-export-prev")
                    .icon(gpui_kit::assets::IconName::ChevronLeft)
                    .ghost()
                    .disabled(month <= EARLIEST)
                    .tooltip(tr(keys::Q_OPS_PREVIOUS_MONTH))
                    .accessibility_label(tr(keys::Q_OPS_PREVIOUS_MONTH))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(draft) = this.dialogs.chat_export_dialog.as_mut() {
                            draft.month = month.prev();
                        }
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id("chat-export-month")
                    .role(Role::Heading)
                    .aria_label(month.title())
                    .font_semibold()
                    .child(month.title()),
            )
            .child(
                Button::new("chat-export-next")
                    .icon(gpui_kit::assets::IconName::ChevronRight)
                    .ghost()
                    .disabled(month >= current)
                    .tooltip(tr(keys::Q_OPS_NEXT_MONTH))
                    .accessibility_label(tr(keys::Q_OPS_NEXT_MONTH))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(draft) = this.dialogs.chat_export_dialog.as_mut() {
                            draft.month = month.next();
                        }
                        cx.notify();
                    })),
            );
        let mut weekdays = div().flex();
        for weekday in 0..7u8 {
            weekdays = weekdays.child(
                div()
                    .w(px(CELL))
                    .flex()
                    .justify_center()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(
                        quill::local_time::weekday_name(weekday)
                            .get(..2)
                            .unwrap_or_default()
                            .to_string(),
                    ),
            );
        }
        let mut grid = div().flex().flex_col().gap_0p5();
        for week in month.grid() {
            let mut row = div().flex();
            for cell in week {
                let Some(day) = cell else {
                    row = row.child(div().size(px(CELL)));
                    continue;
                };
                let dn = day_number(month.year, month.month, day);
                let mut cell = div()
                    .id(("chat-export-day", dn as u64))
                    .size(px(CELL))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .text_sm()
                    .role(Role::Button)
                    .aria_label(day.to_string())
                    .child(day.to_string());
                if dn == today {
                    cell = cell.border_1().border_color(theme.primary);
                }
                if dn > today {
                    cell = cell.text_color(theme.muted_foreground.opacity(0.4));
                } else {
                    cell = cell
                        .cursor_pointer()
                        .tab_index(0)
                        .hover(|s| s.bg(theme.secondary_hover))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(draft) = this.dialogs.chat_export_dialog.as_mut() {
                                match field {
                                    DateField::From => draft.settings.set_from_day(dn),
                                    DateField::Till => draft.settings.set_till_day(dn),
                                }
                                draft.picking = None;
                            }
                            cx.notify();
                        }));
                    if selected_day == Some(dn) {
                        cell = cell
                            .bg(theme.primary)
                            .text_color(theme.primary_foreground)
                            .font_semibold();
                    }
                }
                row = row.child(cell);
            }
            grid = grid.child(row);
        }
        let body = div()
            .id("chat-export-calendar")
            .mx_auto()
            .flex()
            .flex_col()
            .gap_2()
            .w(px(CELL * 7.))
            .child(header)
            .child(weekdays)
            .child(grid);
        let footer = div().flex().justify_end().child(
            Button::new("chat-export-calendar-back")
                .label(tr(keys::Q_OPS_BACK))
                .ghost()
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(draft) = this.dialogs.chat_export_dialog.as_mut() {
                        draft.picking = None;
                    }
                    cx.notify();
                })),
        );
        let title = match field {
            DateField::From => tr(keys::Q_OPS_EXPORT_FROM),
            DateField::Till => tr(keys::Q_OPS_EXPORT_UNTIL),
        };
        (
            title.into(),
            body.into_any_element(),
            footer.into_any_element(),
        )
    }
}

crate::ui::shell::register_dialogs! {
    /// The "Export chat history" options box, and the running export's
    /// progress.
    ChatExport => DialogSpec::new(
        1500,
        |app| {
            app.dialogs.chat_export_dialog.is_some()
                || (!app.dialogs.chat_export_progress_hidden
                    && app.session().is_some_and(|s| s.settings.chat_export.is_some()))
        },
        QuillApp::build_chat_export_dialog,
    )
    .closes(|app, _, cx| { app.dialogs.chat_export_dialog = None; app.dialogs.chat_export_progress_hidden = true; cx.notify(); }),
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "export_box_tests.rs"]
mod tests;
