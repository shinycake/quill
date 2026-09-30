//! Slice S4: Settings → Data & Storage dialog (TGX
//! `SettingsDataStorageController`): per-network automatic-download
//! settings (`getAutoDownloadSettingsPresets` /
//! `setAutoDownloadSettings`), the "use less data for calls" toggle
//! (`autoDownloadSettings.use_less_data_for_calls`), the storage-usage
//! breakdown with per-chat rows (`getStorageStatistics` with
//! `chat_limit` 50), and "Clear cache" (`removeAllFilesFromDownloads`).
//!
//! The current per-network values are the local source of truth —
//! TDLib exposes the presets and the setter but no getter for the
//! current values — so they load from `data_storage.json` and are
//! seeded once from the presets (Wi-Fi ← high, mobile ← medium,
//! roaming ← low).

use super::super::app::QuillApp;
use super::super::shell::{DialogKind, QuillShell};
use super::super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::table::{Table, TableBody, TableRow};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::data_settings::{
    AutoDownloadNetSettings, DataStoragePrefs, NetworkKind, size_cap_label, top_chats_by_size,
};
use std::cell::RefCell;
use std::rc::Rc;
/// Slice S4: bytes formatter that handles 0 and GB — the shared
/// `format_bytes` returns "" for 0 and tops out at MB.
fn format_storage_bytes(n: i64) -> String {
    if n <= 0 {
        "0 B".to_string()
    } else if n < 1024 {
        format!("{n} B")
    } else if n < 1024 * 1024 {
        format!("{} KB", n / 1024)
    } else if n < 1024 * 1024 * 1024 {
        format!("{:.1} MB", n as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", n as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

fn section_header(title: &'static str) -> Div {
    div().text_xs().font_semibold().child(title)
}

impl QuillApp {
    /// Slice S4: open the Data & Storage dialog — reset the transient
    /// view state, clear the last "Cache cleared" note, and fire the
    /// storage + presets fetches (both guarded: once per session).
    pub(crate) fn open_data_storage(&mut self, cx: &mut Context<Self>) {
        self.storage_usage_open = true;
        self.data_storage_editor = None;
        self.data_storage_confirm_clear = false;
        if let Some(live) = self.live.as_mut() {
            live.driver.session.cache_cleared = false;
            let _ = live.driver.maybe_fetch_storage_statistics();
            let _ = live.driver.fetch_auto_download_presets();
        }
        if let Some(demo) = self.demo_session.as_mut() {
            demo.cache_cleared = false;
        }
        cx.notify();
    }

    /// Slice S4: the Data & Storage dialog — the per-network editor when
    /// one is open, otherwise the overview (auto-download networks,
    /// less-data-for-calls, the storage breakdown, clear cache).
    pub(crate) fn build_storage_usage_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::StorageUsage, |this, _, cx| {
                this.storage_usage_open = false;
                this.data_storage_editor = None;
                this.data_storage_confirm_clear = false;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let editing = this.data_storage_editor.clone();
            let (title, body) = match &editing {
                Some((network, draft)) => (
                    format!("{} downloads", network.label()),
                    this.data_storage_editor_body(cx, *network, draft),
                ),
                None => ("Data & Storage".to_string(), this.data_storage_body(cx)),
            };
            let mut footer = div().flex().justify_end().gap_2();
            if editing.is_some() {
                footer =
                    footer
                        .child(
                            Button::new("data-storage-back")
                                .label("Back")
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.data_storage_editor = None;
                                    cx.notify();
                                })),
                        )
                        .child(Button::new("data-storage-save").label("Save").on_click(
                            cx.listener(|this, _, _, cx| {
                                if let Some((network, draft)) = this.data_storage_editor.take() {
                                    if let Some(live) = this.live.as_mut() {
                                        // Applied on the confirmed `ok`
                                        // (never optimistically); a failed
                                        // send restores the draft so
                                        // nothing is lost.
                                        if live
                                            .driver
                                            .set_auto_download_settings(network, draft.clone())
                                            .is_err()
                                        {
                                            this.data_storage_editor = Some((network, draft));
                                        }
                                    } else {
                                        // Demo mode: apply locally.
                                        if let Some(demo) = this.demo_session.as_mut() {
                                            *demo.data_storage.for_network_mut(network) = draft;
                                            demo.data_storage.seeded = true;
                                        }
                                    }
                                    cx.notify();
                                }
                            }),
                        ));
            } else {
                footer = footer
                    .child(
                        Button::new("storage-refresh")
                            .label("Refresh")
                            .ghost()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.refresh_storage_usage(cx);
                                this.close_kit_dialog_if_done(DialogKind::StorageUsage, window, cx);
                            })),
                    )
                    .child(
                        Button::new("close-storage-usage")
                            .label("Close")
                            .ghost()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.storage_usage_open = false;
                                cx.notify();
                                this.close_kit_dialog_if_done(DialogKind::StorageUsage, window, cx);
                            })),
                    );
            }
            dialog
                .overlay(true)
                .title(title)
                .content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                })
                .footer(footer)
                .on_close(on_close)
        })
    }

    /// Slice S4: the overview — error line, the three auto-download
    /// network rows, the less-data-for-calls toggle, the storage
    /// breakdown (totals, per-type, per-chat), and clear cache.
    fn data_storage_body(&self, cx: &mut Context<Self>) -> Div {
        let session = self.session();
        let mut body = div().flex().flex_col().gap_3();
        // Failures surface here, never as toasts (the S3 pattern).
        if let Some(err) = session.as_ref().and_then(|s| s.data_storage_error.clone()) {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(danger())
                    .child(format!("Error: {err}")),
            );
        }
        // --- Automatic downloads ---
        body = body.child(section_header("Automatic downloads"));
        let prefs = session.as_ref().map(|s| &s.data_storage);
        if !prefs.is_some_and(|p| p.seeded) {
            let loading = session.is_some_and(|s| s.auto_download_presets_loading);
            body = body.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(if loading {
                        "Loading download settings…"
                    } else {
                        "Download settings unavailable."
                    }),
            );
        } else if let Some(prefs) = prefs {
            for network in NetworkKind::ALL {
                body = body.child(self.data_storage_network_row(
                    cx,
                    network,
                    &prefs.network_summary(network),
                ));
            }
            // One toggle drives all three networks' `use_less_data_for_calls`
            // (TGX keeps a single switch); each network keeps its own caps.
            let all_on = NetworkKind::ALL
                .iter()
                .all(|n| prefs.for_network(*n).use_less_data_for_calls);
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .py_1()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(div().text_sm().child("Use less data for calls"))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Lower data usage during Telegram calls"),
                            ),
                    )
                    .child(
                        Switch::new("data-storage-less-data-calls")
                            .checked(all_on)
                            .accessibility_label("Use less data for calls")
                            .on_click(cx.listener(move |this, &on, _, cx| {
                                if let Some(live) = this.live.as_mut() {
                                    // Send failures land on
                                    // `data_storage_error`, shown on
                                    // this dialog (the S3 pattern).
                                    let _ = live.driver.set_less_data_for_calls(on);
                                } else if let Some(demo) = this.demo_session.as_mut() {
                                    for n in NetworkKind::ALL {
                                        demo.data_storage
                                            .for_network_mut(n)
                                            .use_less_data_for_calls = on;
                                    }
                                }
                                cx.notify();
                            })),
                    ),
            );
        }
        // --- Storage usage ---
        body = body.child(section_header("Storage usage"));
        let stats = session.as_ref().and_then(|s| s.storage_stats.clone());
        let loading = session.is_some_and(|s| s.storage_stats_loading);
        match stats {
            None => {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(if loading {
                            "Loading…"
                        } else {
                            "No storage data yet."
                        }),
                );
            }
            Some(stats) => {
                // Phase 6: a static kit Table (was: hand-rolled
                // justify-between rows).
                let mut table_body = TableBody::new().child(
                    TableRow::new()
                        .child(Self::table_cell(
                            div().font_semibold().text_sm().child("Total"),
                        ))
                        .child(
                            Self::table_cell(
                                div()
                                    .text_sm()
                                    .child(format_storage_bytes(stats.total_size)),
                            )
                            .text_right(),
                        ),
                );
                for (label, size, count) in quill::telegram::envelope::storage_category_rows(&stats)
                {
                    table_body = table_body.child(
                        TableRow::new()
                            .child(Self::table_cell(div().text_sm().child(label)))
                            .child(
                                Self::table_cell(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(format!(
                                            "{count} files · {}",
                                            format_storage_bytes(size)
                                        )),
                                )
                                .text_right(),
                            ),
                    );
                }
                body = body.child(Table::new().w_full().child(table_body));
                // Per-chat breakdown (largest first).
                let chat_rows: Vec<(i64, i64, i32)> = stats
                    .by_chat
                    .iter()
                    .map(|row| (row.chat_id, row.size, row.count))
                    .collect();
                let top: Vec<(String, i64, i32)> = top_chats_by_size(&chat_rows, 10)
                    .into_iter()
                    .map(|(chat_id, size, count)| {
                        // Slice S4 fix-up: chat_id 0 is the schema's
                        // "all other chats grouped" bucket, not a chat.
                        let title = if chat_id == 0 {
                            "Other chats".to_string()
                        } else {
                            session
                                .as_ref()
                                .and_then(|s| s.chats.get(&chat_id))
                                .map(|c| c.title.clone())
                                .unwrap_or_else(|| format!("Chat {chat_id}"))
                        };
                        (title, size, count)
                    })
                    .collect();
                if !top.is_empty() {
                    body = body.child(section_header("Chats"));
                    let mut chat_body = TableBody::new();
                    for (title, size, count) in top {
                        chat_body = chat_body.child(
                            TableRow::new()
                                .child(Self::table_cell(div().text_sm().child(title)))
                                .child(
                                    Self::table_cell(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(format!(
                                                "{count} files · {}",
                                                format_storage_bytes(size)
                                            )),
                                    )
                                    .text_right(),
                                ),
                        );
                    }
                    body = body.child(Table::new().w_full().child(chat_body));
                }
                body = body.child(self.data_storage_clear_cache_row(cx));
            }
        }
        body
    }

    /// Slice S4: one auto-download network row — label, current summary,
    /// chevron; opens the per-network editor.
    fn data_storage_network_row(
        &self,
        cx: &mut Context<Self>,
        network: NetworkKind,
        summary: &str,
    ) -> impl IntoElement {
        div()
            .id(format!(
                "data-storage-net-{}",
                network.label().replace(' ', "-")
            ))
            .cursor_pointer()
            .flex()
            .items_center()
            .justify_between()
            .py_1()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(div().text_sm().child(network.label()))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(summary.to_string()),
                    ),
            )
            .child(Icon::new(IconName::ChevronRight).size_4())
            .on_click(cx.listener(move |this, _, _, cx| {
                let draft = this
                    .session()
                    .map(|s| s.data_storage.for_network(network).clone());
                if let Some(draft) = draft {
                    this.data_storage_editor = Some((network, draft));
                    cx.notify();
                }
            }))
    }

    /// Slice S4: the per-network editor — the enable switch, the three
    /// size-cap cyclers, and this network's less-data-for-calls switch.
    /// Edits stay in the draft until Save.
    fn data_storage_editor_body(
        &self,
        cx: &mut Context<Self>,
        network: NetworkKind,
        draft: &AutoDownloadNetSettings,
    ) -> Div {
        let mut body = div().flex().flex_col().gap_2();
        body = body.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .py_1()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(div().text_sm().child("Automatic download"))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(format!(
                                    "Download media automatically on {}",
                                    network.label().to_lowercase()
                                )),
                        ),
                )
                .child(
                    Switch::new("data-storage-enabled")
                        .checked(draft.is_auto_download_enabled)
                        .accessibility_label("Automatic download")
                        .on_click(cx.listener(|this, &on, _, cx| {
                            if let Some((_, draft)) = this.data_storage_editor.as_mut() {
                                draft.is_auto_download_enabled = on;
                            }
                            cx.notify();
                        })),
                ),
        );
        body = body.child(section_header("Maximum file size"));
        body = body.child(self.data_storage_cap_row(
            cx,
            "data-storage-cap-photo",
            "Photos",
            draft.max_photo_file_size,
            AutoDownloadNetSettings::cycle_photo_cap,
        ));
        body = body.child(self.data_storage_cap_row(
            cx,
            "data-storage-cap-video",
            "Videos",
            draft.max_video_file_size,
            AutoDownloadNetSettings::cycle_video_cap,
        ));
        body = body.child(self.data_storage_cap_row(
            cx,
            "data-storage-cap-other",
            "Other files",
            draft.max_other_file_size,
            AutoDownloadNetSettings::cycle_other_cap,
        ));
        body = body.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .py_1()
                .child(div().text_sm().child("Use less data for calls"))
                .child(
                    Switch::new("data-storage-editor-less-data-calls")
                        .checked(draft.use_less_data_for_calls)
                        .accessibility_label("Use less data for calls")
                        .on_click(cx.listener(|this, &on, _, cx| {
                            if let Some((_, draft)) = this.data_storage_editor.as_mut() {
                                draft.use_less_data_for_calls = on;
                            }
                            cx.notify();
                        })),
                ),
        );
        body = body.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("Saved on this device; applies to new downloads."),
        );
        body
    }

    /// Slice S4: one size-cap cycler row — the current cap as a button
    /// that steps through the preset sizes.
    fn data_storage_cap_row(
        &self,
        cx: &mut Context<Self>,
        id: &'static str,
        title: &str,
        cap: i64,
        cycle: fn(&mut AutoDownloadNetSettings),
    ) -> Div {
        div()
            .flex()
            .items_center()
            .justify_between()
            .py_1()
            .child(div().text_sm().child(title.to_string()))
            .child(
                Button::new(id)
                    .label(size_cap_label(cap))
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some((_, draft)) = this.data_storage_editor.as_mut() {
                            cycle(draft);
                        }
                        cx.notify();
                    })),
            )
    }

    /// Slice S4: the "Clear cache" row — two-step confirm, then
    /// `removeAllFilesFromDownloads`; the confirmed clear shows a note
    /// until the dialog is reopened.
    fn data_storage_clear_cache_row(&self, cx: &mut Context<Self>) -> Div {
        let cleared = self.session().is_some_and(|s| s.cache_cleared);
        let mut row = div().flex().flex_col().gap_2();
        if cleared {
            row = row.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Cache cleared."),
            );
        }
        if self.data_storage_confirm_clear {
            row = row.child(
                div()
                    .text_sm()
                    .child("Clear all cached files? In-progress downloads are left alone."),
            );
            row = row.child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("data-storage-clear-confirm")
                            .label("Clear cache")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.data_storage_confirm_clear = false;
                                if let Some(live) = this.live.as_mut() {
                                    // Send failures land on
                                    // `data_storage_error`, shown on this
                                    // dialog (the S3 pattern).
                                    let _ = live.driver.clear_download_cache();
                                } else if let Some(demo) = this.demo_session.as_mut() {
                                    demo.storage_stats = None;
                                    demo.cache_cleared = true;
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("data-storage-clear-cancel")
                            .label("Keep")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.data_storage_confirm_clear = false;
                                cx.notify();
                            })),
                    ),
            );
        } else {
            row = row.child(
                Button::new("data-storage-clear")
                    .label("Clear cache")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.data_storage_confirm_clear = true;
                        cx.notify();
                    })),
            );
        }
        row
    }
}

/// Slice S4: per-network auto-download fixture for the
/// `ReadyStorageUsage` screenshot demo — seeded as if the presets
/// arrived (Wi-Fi ← high, mobile ← medium, roaming ← low), less data
/// for calls on (injected, no live Telegram).
pub(crate) fn demo_data_storage_prefs() -> DataStoragePrefs {
    let mut prefs = DataStoragePrefs::default();
    prefs.seed_from_presets(
        AutoDownloadNetSettings {
            is_auto_download_enabled: false,
            max_photo_file_size: 1024 * 1024,
            max_video_file_size: 10 * 1024 * 1024,
            max_other_file_size: 10 * 1024 * 1024,
            video_upload_bitrate: 0,
            use_less_data_for_calls: true,
            ..Default::default()
        },
        AutoDownloadNetSettings {
            is_auto_download_enabled: true,
            max_photo_file_size: 1024 * 1024,
            max_video_file_size: 15 * 1024 * 1024,
            max_other_file_size: 10 * 1024 * 1024,
            video_upload_bitrate: 0,
            use_less_data_for_calls: true,
            ..Default::default()
        },
        AutoDownloadNetSettings {
            is_auto_download_enabled: true,
            max_photo_file_size: 10 * 1024 * 1024,
            max_video_file_size: 50 * 1024 * 1024,
            max_other_file_size: 100 * 1024 * 1024,
            video_upload_bitrate: 0,
            use_less_data_for_calls: true,
            ..Default::default()
        },
    );
    prefs
}
