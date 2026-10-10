//! Appearance → Spelling → "Manage dictionaries" (parity:appearance-
//! dictionaries), after Telegram Desktop's `boxes/dictionaries_manager.cpp`:
//! a filterable list of spell-checker languages, each with a switch and a
//! state line ("Download size 4.8 MB", "Downloading 42%", "Enabled").
//! Switching an uninstalled language on downloads it; switching a
//! downloading one off cancels; "Remove" deletes a downloaded copy.
//!
//! Shown only where Quill owns the dictionaries (Hunspell on Linux, the
//! fallback on Windows). macOS and the Windows system checker keep their
//! languages in the OS, as in tdesktop.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use super::app::QuillApp;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::Textarea;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::spell_catalog::{
    ManagerRow, RowInputs, RowState, Transfer, build_rows, installed_codes, managed_dir,
    next_chosen, percent, remove,
};
use quill::spell_dict::EngineKind;
use quill::spell_download::{Progress, download_dictionary};

/// The one download in flight (tdesktop also loads a single dictionary at
/// a time).
pub(super) struct ActiveDownload {
    code: String,
    progress: Arc<Progress>,
    total: u64,
    percent: u8,
}

/// Manager state on the app.
#[derive(Default)]
pub(super) struct DictManager {
    pub open: bool,
    download: Option<ActiveDownload>,
    task: Option<Task<()>>,
    /// `(code, message)` of the last failed download.
    failed: Option<(String, String)>,
    /// Screenshot fixture: rows to show instead of the real ones.
    pub demo_rows: Option<Vec<ManagerRow>>,
}

impl DictManager {
    /// The screenshot fixture: an enabled and an installed dictionary, one
    /// mid-download and the rest available.
    pub(super) fn demo() -> Self {
        let mut rows = Vec::new();
        let transfer = Transfer {
            code: "de",
            percent: 42,
        };
        let available = vec!["en_US".to_string(), "fr".to_string()];
        let active = vec!["en_US".to_string()];
        let managed = std::collections::BTreeSet::from(["fr".to_string()]);
        rows.extend(build_rows(
            &RowInputs {
                managed: &managed,
                available: &available,
                active: &active,
                transfer: Some(transfer),
                failed: Some("it"),
            },
            "",
        ));
        // States worth seeing first, the plain downloads after them.
        rows.sort_by_key(|r| matches!(r.state, RowState::Available { .. }));
        DictManager {
            open: true,
            demo_rows: Some(rows),
            ..Default::default()
        }
    }
}

impl QuillApp {
    /// Where downloaded dictionaries live.
    fn dictionaries_dir() -> Option<PathBuf> {
        quill::settings::safe_app_root().map(|root| managed_dir(&root))
    }

    /// The manager is offered when Quill owns the dictionaries.
    pub(super) fn dictionary_manager_available(&self) -> bool {
        self.spell.dict_manager.demo_rows.is_some() || self.spell.info.kind != EngineKind::System
    }

    fn dictionary_rows(&self, query: &str) -> Vec<ManagerRow> {
        if let Some(rows) = &self.spell.dict_manager.demo_rows {
            return rows
                .iter()
                .filter(|r| quill::spell_catalog::matches_query(&r.name, &r.code, query))
                .cloned()
                .collect();
        }
        let managed = Self::dictionaries_dir()
            .map(|dir| installed_codes(&dir))
            .unwrap_or_default();
        let transfer = self.spell.dict_manager.download.as_ref().map(|d| Transfer {
            code: &d.code,
            percent: d.percent,
        });
        build_rows(
            &RowInputs {
                managed: &managed,
                available: &self.spell.info.available,
                active: &self.spell.info.active,
                transfer,
                failed: self
                    .spell
                    .dict_manager
                    .failed
                    .as_ref()
                    .map(|(c, _)| c.as_str()),
            },
            query,
        )
    }

    /// A row's switch.
    fn toggle_dictionary(&mut self, code: &str, enable: bool, cx: &mut Context<Self>) {
        if self.spell.dict_manager.demo_rows.is_some() {
            return;
        }
        let downloading = self
            .spell
            .dict_manager
            .download
            .as_ref()
            .is_some_and(|d| d.code == code);
        if downloading {
            if !enable && let Some(d) = &self.spell.dict_manager.download {
                d.progress.cancel();
            }
            return;
        }
        let installed = self.spell.info.available.iter().any(|c| c == code);
        if enable && !installed {
            self.start_dictionary_download(code, cx);
            return;
        }
        let next = next_chosen(
            &self.spell.info.chosen,
            &self.spell.info.active,
            code,
            enable,
        );
        // Switching off the last language falls back to "automatic".
        self.set_spell_languages(next, cx);
    }

    fn start_dictionary_download(&mut self, code: &str, cx: &mut Context<Self>) {
        if self.spell.dict_manager.download.is_some() {
            return;
        }
        let (Some(dir), Some(entry)) =
            (Self::dictionaries_dir(), quill::spell_catalog::entry(code))
        else {
            return;
        };
        let progress = Arc::new(Progress::default());
        self.spell.dict_manager.failed = None;
        self.spell.dict_manager.download = Some(ActiveDownload {
            code: code.to_string(),
            progress: progress.clone(),
            total: entry.bytes,
            percent: 0,
        });
        let code = code.to_string();
        let worker_progress = progress.clone();
        let worker_code = code.clone();
        let worker = cx.background_spawn(async move {
            let result = download_dictionary(&dir, &worker_code, &worker_progress);
            worker_progress.finish();
            result
        });
        self.spell.dict_manager.task = Some(cx.spawn(async move |this, cx| {
            while !progress.is_finished() {
                cx.background_executor()
                    .timer(Duration::from_millis(120))
                    .await;
                let received = progress.received();
                let _ = this.update(cx, |this, cx| {
                    if let Some(d) = this.spell.dict_manager.download.as_mut() {
                        d.percent = percent(received, d.total);
                    }
                    cx.notify();
                });
            }
            let result = worker.await;
            let _ = this.update(cx, |this, cx| {
                this.spell.dict_manager.download = None;
                match result {
                    Ok(()) => {
                        let next = next_chosen(
                            &this.spell.info.chosen,
                            &this.spell.info.active,
                            &code,
                            true,
                        );
                        this.set_spell_languages(next, cx);
                    }
                    Err(message) if message == "Cancelled." => {}
                    Err(message) => this.spell.dict_manager.failed = Some((code.clone(), message)),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// tdesktop "Remove dictionary".
    fn remove_dictionary(&mut self, code: &str, cx: &mut Context<Self>) {
        let Some(dir) = Self::dictionaries_dir() else {
            return;
        };
        if let Err(err) = remove(&dir, code) {
            self.connection.status_note = format!("Couldn't remove the dictionary: {err}");
        }
        let chosen: Vec<String> = self
            .spell
            .info
            .chosen
            .iter()
            .filter(|c| *c != code)
            .cloned()
            .collect();
        self.set_spell_languages(chosen, cx);
    }

    fn dictionary_row(&self, row: &ManagerRow, cx: &mut Context<Self>) -> AnyElement {
        let on = matches!(row.state, RowState::Enabled | RowState::Downloading { .. });
        let failed = matches!(row.state, RowState::Failed);
        let code = row.code.clone();
        let toggle_code = row.code.clone();
        let remove_code = row.code.clone();
        let removable = row.removable && !matches!(row.state, RowState::Downloading { .. });
        let status_color = if failed {
            cx.theme().danger
        } else if matches!(row.state, RowState::Enabled) {
            cx.theme().primary
        } else {
            cx.theme().muted_foreground
        };
        let title = format!("{}, {}", row.name, row.status());
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(div().text_sm().child(row.name.clone()))
                    .child(div().text_xs().text_color(status_color).child(row.status())),
            )
            .when(removable, |this| {
                this.child(
                    Button::new(SharedString::from(format!("dict-remove-{code}")))
                        .label("Remove")
                        .ghost()
                        .small()
                        .accessibility_label(format!("Remove the {} dictionary", row.name))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.remove_dictionary(&remove_code, cx);
                        })),
                )
            })
            .child(
                Switch::new(SharedString::from(format!("dict-switch-{code}")))
                    .checked(on)
                    .accessibility_label(title)
                    .on_click(cx.listener(move |this, &on, _, cx| {
                        this.toggle_dictionary(&toggle_code, on, cx);
                    })),
            )
            .into_any_element()
    }

    /// The "Manage dictionaries" button and, open, the filter and list.
    pub(super) fn dictionary_manager_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let open = self.spell.dict_manager.open;
        let mut section = div().flex().flex_col().gap_2().child(
            Button::new("spell-dictionaries-toggle")
                .label(if open {
                    "Hide dictionaries"
                } else {
                    "Manage dictionaries"
                })
                .outline()
                .small()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.spell.dict_manager.open = !this.spell.dict_manager.open;
                    cx.notify();
                })),
        );
        if open {
            let query = self.spell.dict_filter_input.read(cx).value().to_string();
            let rows = self.dictionary_rows(&query);
            let mut list = div()
                .id("spell-dictionaries-list")
                .max_h(px(260.))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_2()
                .pr_2();
            if rows.is_empty() {
                list = list.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("No dictionaries match."),
                );
            }
            for row in &rows {
                list = list.child(self.dictionary_row(row, cx));
            }
            if let Some((_, message)) = &self.spell.dict_manager.failed {
                list = list.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().danger)
                        .child(message.clone()),
                );
            }
            section = section
                .child(
                    Textarea::new(&self.spell.dict_filter_input)
                        .aria_label("Filter dictionaries")
                        .w_full(),
                )
                .child(list)
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(
                            "Dictionaries come from the open wooorm/dictionaries collection and keep their own licenses.",
                        ),
                );
        }
        section.into_any_element()
    }
}
