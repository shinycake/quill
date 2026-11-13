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
#![deny(clippy::indexing_slicing)]

use std::path::PathBuf;
use std::sync::Arc;

use super::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::Textarea;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with};
use quill::spell_catalog::{
    ManagerRow, RowDownloadView, RowInputs, RowView, Transfer, build_rows, installed_codes,
    managed_dir, next_chosen, percent, remove,
};
use quill::spell_dict::EngineKind;
use quill::spell_download::{DownloadError, Progress, download_dictionary, run_guarded};
use std::collections::BTreeSet;

/// The one download in flight (tdesktop also loads a single dictionary at
/// a time).
pub(super) struct ActiveDownload {
    code: String,
    progress: Arc<Progress>,
    total: u64,
    percent: u8,
}

/// The one download: none, running (with the task that follows it, so
/// dropping it ends the loop), cancelled but still being let go of, or the
/// last one's failure.
#[derive(Default)]
enum DictDownload {
    #[default]
    Idle,
    Running {
        download: ActiveDownload,
        /// Dropped with the stage: dropping it ends the progress loop.
        _task: Task<()>,
    },
    /// The user switched the download off: the row is a plain off switch
    /// again at once, but the worker may be past its last cancel check, so
    /// the task goes on to see it end and read the dictionary folder once
    /// more. A new download waits for this worker to end (it still has
    /// `.X.*.part` and `.X.aff.old` open), and this stage leaves when that
    /// task ends.
    Cancelling { _task: Task<()> },
    /// The last download of `code` failed with `message`; starting another
    /// (or any download) clears it.
    Failed { code: String, message: String },
}

/// Manager state on the app.
#[derive(Default)]
pub(super) struct DictManager {
    pub open: bool,
    download: DictDownload,
    /// The codes of the dictionaries on disk, read when the list opens and
    /// after a download or a removal, not on every frame.
    installed: BTreeSet<String>,
    /// Screenshot fixture: rows to show instead of the real ones.
    pub demo_rows: Option<Vec<ManagerRow>>,
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

    /// Reads the dictionary folder into [`DictManager::installed`].
    fn refresh_installed_dictionaries(&mut self) {
        self.spell.dict_manager.installed = Self::dictionaries_dir()
            .map(|dir| installed_codes(&dir))
            .unwrap_or_default();
    }

    fn dictionary_rows(&self, query: &str) -> Vec<ManagerRow> {
        if let Some(rows) = &self.spell.dict_manager.demo_rows {
            return rows
                .iter()
                .filter(|r| quill::spell_catalog::matches_query(&r.name, &r.code, query))
                .cloned()
                .collect();
        }
        let managed = &self.spell.dict_manager.installed;
        let download = match &self.spell.dict_manager.download {
            DictDownload::Idle | DictDownload::Cancelling { .. } => RowDownloadView::None,
            DictDownload::Running { download, .. } => RowDownloadView::Running(Transfer {
                code: &download.code,
                percent: download.percent,
            }),
            DictDownload::Failed { code, .. } => RowDownloadView::Failed(code),
        };
        build_rows(
            &RowInputs {
                managed,
                available: &self.spell.info.available,
                active: &self.spell.info.active,
                download,
            },
            query,
        )
    }

    /// A row's switch.
    fn toggle_dictionary(&mut self, code: &str, enable: bool, cx: &mut Context<Self>) {
        if self.spell.dict_manager.demo_rows.is_some() {
            return;
        }
        let downloading = matches!(
            &self.spell.dict_manager.download,
            DictDownload::Running { download, .. } if download.code == code
        );
        if downloading {
            if !enable {
                self.cancel_dictionary_download(cx);
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

    /// Switching a downloading dictionary off ends it at once, as
    /// tdesktop's destroyed loader does: the row is a plain off switch
    /// again and the progress loop and the worker's answer are dropped. The
    /// blocking read the worker may be stuck in cannot be interrupted; the
    /// flag ends it at the next chunk and nothing it produces is used, so a
    /// stalled server holds nothing but its own thread.
    fn cancel_dictionary_download(&mut self, cx: &mut Context<Self>) {
        if let DictDownload::Running { download, _task } =
            std::mem::take(&mut self.spell.dict_manager.download)
        {
            download.progress.cancel();
            self.spell.dict_manager.download = DictDownload::Cancelling { _task };
        }
        cx.notify();
    }

    fn start_dictionary_download(&mut self, code: &str, cx: &mut Context<Self>) {
        if matches!(
            self.spell.dict_manager.download,
            DictDownload::Running { .. } | DictDownload::Cancelling { .. }
        ) {
            // One download at a time (a cancelled one still has its worker
            // on the dictionary files): the switch snaps back, so say why.
            self.set_note(StatusNote::info(tr(
                keys::Q_MISC_WAIT_FOR_THE_DICTIONARY_DOWNLOAD,
            )));
            return;
        }
        let (Some(dir), Some(entry)) =
            (Self::dictionaries_dir(), quill::spell_catalog::entry(code))
        else {
            return;
        };
        let (progress, ended) = Progress::start();
        let download = ActiveDownload {
            code: code.to_string(),
            progress: progress.clone(),
            total: entry.bytes,
            percent: 0,
        };
        let code = code.to_string();
        let worker_progress = progress.clone();
        let worker_code = code.clone();
        let worker = cx.background_spawn(async move {
            run_guarded(ended, || {
                download_dictionary(&dir, &worker_code, &worker_progress)
            })
        });
        // screen-scope: the spell checker and its dictionaries belong to the
        // app, not to an account; the download finishes on whichever screen
        let task = Self::spawn_app(cx, async move |this, cx| {
            // Woken by the worker's chunks and its end, not by a timer.
            let mut seen = progress.changed.epoch();
            while !progress.is_finished() {
                progress.changed.wait_since(seen).await;
                seen = progress.changed.epoch();
                let received = progress.received();
                let _ = this.update(cx, |this, cx| {
                    if let DictDownload::Running { download, .. } =
                        &mut this.spell.dict_manager.download
                    {
                        let now = percent(received, download.total);
                        if now != download.percent {
                            download.percent = now;
                            cx.notify();
                        }
                    }
                });
            }
            let result = worker.await;
            let _ = this.update(cx, |this, cx| {
                // This task is the running stage's: it is let go of after
                // the answer is applied, not under it.
                let finished = std::mem::take(&mut this.spell.dict_manager.download);
                // A cancel that came too late to stop the install still
                // leaves the dictionary on disk: the list shows it.
                this.refresh_installed_dictionaries();
                let cancelled = matches!(finished, DictDownload::Cancelling { .. });
                match result {
                    _ if cancelled => {}
                    Ok(()) => {
                        let next = next_chosen(
                            &this.spell.info.chosen,
                            &this.spell.info.active,
                            &code,
                            true,
                        );
                        this.set_spell_languages(next, cx);
                    }
                    Err(DownloadError::Cancelled) => {}
                    Err(DownloadError::Message(message)) => {
                        this.spell.dict_manager.download = DictDownload::Failed {
                            code: code.clone(),
                            message,
                        };
                    }
                }
                cx.notify();
            });
        });
        self.spell.dict_manager.download = DictDownload::Running {
            download,
            _task: task,
        };
        cx.notify();
    }

    /// tdesktop "Remove dictionary".
    fn remove_dictionary(&mut self, code: &str, cx: &mut Context<Self>) {
        let Some(dir) = Self::dictionaries_dir() else {
            return;
        };
        let removed = remove(&dir, code);
        // A half-removed dictionary still counts as it is on disk.
        self.refresh_installed_dictionaries();
        if let Err(err) = removed {
            self.set_note(StatusNote::error(tr_with(
                keys::Q_MISC_COULDNT_REMOVE_THE_DICTIONARY,
                &[("err", &err.to_string())],
            )));
            return;
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
        let on = matches!(row.state, RowView::Enabled | RowView::Downloading { .. });
        let failed = matches!(row.state, RowView::Failed);
        let code = row.code.clone();
        let toggle_code = row.code.clone();
        let remove_code = row.code.clone();
        let removable = row.removable && !matches!(row.state, RowView::Downloading { .. });
        let status_color = if failed {
            cx.theme().danger
        } else if matches!(row.state, RowView::Enabled) {
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
                        .label(tr(keys::BOX_REMOVE))
                        .ghost()
                        .small()
                        .accessibility_label(tr_with(
                            keys::Q_MISC_REMOVE_THE_DICTIONARY,
                            &[("name", &row.name.to_string())],
                        ))
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
                    tr(keys::Q_MISC_HIDE_DICTIONARIES)
                } else {
                    tr(keys::SETTINGS_MANAGE_DICTIONARIES)
                })
                .outline()
                .small()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.spell.dict_manager.open = !this.spell.dict_manager.open;
                    if this.spell.dict_manager.open {
                        this.refresh_installed_dictionaries();
                    }
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
                        .child(tr(keys::Q_MISC_NO_DICTIONARIES_MATCH)),
                );
            }
            for row in &rows {
                list = list.child(self.dictionary_row(row, cx));
            }
            if let DictDownload::Failed { message, .. } = &self.spell.dict_manager.download {
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
                        .aria_label(tr(keys::Q_MISC_FILTER_DICTIONARIES))
                        .w_full(),
                )
                .child(list)
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(keys::Q_MISC_DICTIONARIES_COME_FROM_THE_OPEN)),
                );
        }
        section.into_any_element()
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod tests {
    use super::{ActiveDownload, DictDownload};
    use crate::ui::app::QuillApp;
    use crate::ui::screenshot_demo::ScreenshotDemo;
    use crate::ui::shell::QuillShell;
    use gpui_kit::component::Root;
    use gpui_kit::{AppContext, TestAppContext, VisualTestContext, px, size};
    use quill::spell_download::Progress;

    /// Switching a running download off ends the row's download at once,
    /// but the stage keeps the task that sees the worker out, so a cancel
    /// that came too late to stop the install still refreshes the list.
    #[gpui_kit::test]
    fn cancelling_keeps_the_task_that_sees_the_worker_out(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
        let slot_in = slot.clone();
        let handle = cx.open_window(size(px(900.), px(700.)), move |window, cx| {
            let view = cx.new(|cx| {
                QuillApp::new_with_demo(
                    window,
                    cx,
                    None,
                    Some(ScreenshotDemo::named("ready-privacy")),
                )
            });
            let shell = cx.new(|_| QuillShell::new(view.clone()));
            *slot_in.borrow_mut() = Some(view);
            Root::new(shell, window, cx)
        });
        let app = slot.borrow().clone().expect("the app view");
        let mut vcx = VisualTestContext::from_window(handle.into(), cx);
        let (progress, _worker) = Progress::start();
        app.update(&mut vcx, |app, cx| {
            app.spell.dict_manager.download = DictDownload::Running {
                download: ActiveDownload {
                    code: "fr".into(),
                    progress: progress.clone(),
                    total: 10,
                    percent: 0,
                },
                _task: cx.spawn(async move |_, _| {}),
            };
            app.cancel_dictionary_download(cx);
            assert!(matches!(
                app.spell.dict_manager.download,
                DictDownload::Cancelling { .. }
            ));
            assert!(progress.is_cancelled(), "the worker is told to stop");
            // A new download waits: it does not replace the stage and drop
            // the task that sees the worker out.
            app.start_dictionary_download("de", cx);
            assert!(matches!(
                app.spell.dict_manager.download,
                DictDownload::Cancelling { .. }
            ));
        });
    }
}
