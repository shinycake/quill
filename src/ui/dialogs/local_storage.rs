//! Batch 6: Data & Storage → local storage (tdesktop
//! `Settings::LocalStorage`): usage by type with "Clear selected" /
//! "Clear all", per-chat "Clear", the "{size} freed on your device!"
//! result, and the limits ("Total size limit", "Clear files older than").
//! The cache is TDLib's: clearing is `optimizeStorage`, the limits are the
//! options of its storage optimizer.

use super::super::app::QuillApp;
use super::data_storage::format_storage_bytes;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::table::{Table, TableBody, TableRow};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::data_settings::top_chats_by_size;
use quill::storage_limits::{
    KEEP_LIMITS, SIZE_LIMITS, StorageOptionValue, keep_label, options_for, size_limit_label,
};
use quill::telegram::envelope::{StorageStats, storage_category_entries};

/// What the pending "Clear" confirmation will clear.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StorageClear {
    /// Every type TDLib considers re-downloadable.
    All,
    /// The ticked types.
    Selected,
    /// One chat's files (`0` = files that belong to no chat).
    Chat(i64),
}

fn subheading(title: &'static str) -> impl IntoElement {
    div()
        .id(format!("local-storage-heading-{title}"))
        .role(Role::Heading)
        .aria_label(title)
        .text_xs()
        .font_semibold()
        .child(title)
}

impl QuillApp {
    /// The whole local storage block for a loaded usage answer.
    pub(super) fn local_storage_section(
        &self,
        cx: &mut Context<Self>,
        stats: &StorageStats,
    ) -> Div {
        let session = self.session();
        let categories = storage_category_entries(stats);
        let mut section = div().flex().flex_col().gap_2();
        section = section.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(div().font_semibold().text_sm().child("Total"))
                .child(
                    div()
                        .text_sm()
                        .child(format_storage_bytes(stats.total_size)),
                ),
        );
        for category in &categories {
            let size_text = format!(
                "{} files · {}",
                category.count,
                format_storage_bytes(category.size)
            );
            let row = div().flex().items_center().justify_between().gap_2();
            let label = category.label;
            let right = div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(size_text);
            section = section.child(match category.file_type {
                Some(file_type) => {
                    let checked = self.storage_selected.contains(file_type);
                    row.child(
                        Checkbox::new(format!("local-storage-type-{file_type}"))
                            .label(label)
                            .checked(checked)
                            .on_click(cx.listener(move |this, &on: &bool, _, cx| {
                                if on {
                                    this.storage_selected.insert(file_type);
                                } else {
                                    this.storage_selected.remove(file_type);
                                }
                                this.storage_confirm = None;
                                cx.notify();
                            })),
                    )
                    .child(right)
                }
                None => row.child(div().text_sm().child(label)).child(right),
            });
        }
        let selected_size: i64 = categories
            .iter()
            .filter(|c| {
                c.file_type
                    .is_some_and(|t| self.storage_selected.contains(t))
            })
            .map(|c| c.size)
            .sum();
        let clearing = session.as_ref().is_some_and(|s| s.storage_clearing);
        let freed = session.as_ref().and_then(|s| s.storage_freed);
        section = section.child(self.local_storage_actions(
            cx,
            &categories_selected(self),
            selected_size,
            clearing,
        ));
        if clearing {
            section = section.child(div().text_sm().child("Clearing…")).child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Please keep this window open while the cache is being cleared."),
            );
        } else if let Some(freed) = freed {
            section = section.child(
                div()
                    .id("local-storage-freed")
                    .role(Role::Label)
                    .aria_label(format!(
                        "{} freed on your device",
                        format_storage_bytes(freed)
                    ))
                    .text_sm()
                    .child(format!(
                        "{} freed on your device!",
                        format_storage_bytes(freed)
                    )),
            );
        }
        section = section.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(
                    "All media will stay in the Telegram cloud and can be re-downloaded if you need them again.",
                ),
        );
        section = self.local_storage_chats(cx, section, stats, clearing);
        section.child(self.local_storage_limits(cx))
    }

    /// "Clear selected" / "Clear all", or the confirmation of one of them.
    fn local_storage_actions(
        &self,
        cx: &mut Context<Self>,
        selected: &[&'static str],
        selected_size: i64,
        clearing: bool,
    ) -> Div {
        let Some(pending) = self.storage_confirm else {
            return div()
                .flex()
                .flex_wrap()
                .gap_2()
                .child(
                    Button::new("local-storage-clear-selected")
                        .label("Clear selected")
                        .outline()
                        .disabled(selected.is_empty() || clearing)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.storage_confirm = Some(StorageClear::Selected);
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("local-storage-clear-all")
                        .label("Clear all")
                        .outline()
                        .disabled(clearing)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.storage_confirm = Some(StorageClear::All);
                            cx.notify();
                        })),
                );
        };
        let question = match pending {
            StorageClear::All => "Clear all cached files?".to_string(),
            StorageClear::Selected => format!(
                "Clear {} selected {} ({})?",
                selected.len(),
                if selected.len() == 1 { "type" } else { "types" },
                format_storage_bytes(selected_size)
            ),
            StorageClear::Chat(chat_id) => {
                format!(
                    "Clear the cached files of {}?",
                    self.storage_chat_title(chat_id)
                )
            }
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_sm().child(question))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("local-storage-clear-confirm")
                            .label("Clear")
                            .primary()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.run_storage_clear(cx);
                            })),
                    )
                    .child(
                        Button::new("local-storage-clear-cancel")
                            .label("Keep")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.storage_confirm = None;
                                cx.notify();
                            })),
                    ),
            )
    }

    fn storage_chat_title(&self, chat_id: i64) -> String {
        if chat_id == 0 {
            return "other chats".to_string();
        }
        self.session()
            .and_then(|s| s.chats.get(&chat_id).map(|c| c.title.clone()))
            .unwrap_or_else(|| format!("chat {chat_id}"))
    }

    /// Send the confirmed clear (`optimizeStorage`).
    fn run_storage_clear(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = self.storage_confirm.take() else {
            return;
        };
        let (types, chats): (Vec<&'static str>, Vec<i64>) = match pending {
            StorageClear::All => (Vec::new(), Vec::new()),
            StorageClear::Selected => (self.storage_selected.iter().copied().collect(), Vec::new()),
            StorageClear::Chat(chat_id) => (Vec::new(), vec![chat_id]),
        };
        if pending == StorageClear::Selected {
            self.storage_selected.clear();
        }
        if let Some(live) = self.live.as_mut() {
            // Send failures land on `data_storage_error`, shown on this
            // dialog (the S3 pattern).
            let _ = live.driver.clear_storage(&types, &chats);
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.storage_stats = None;
            demo.storage_freed = Some(54_525_952);
        }
        cx.notify();
    }

    /// Largest chats, each with its own "Clear".
    fn local_storage_chats(
        &self,
        cx: &mut Context<Self>,
        section: Div,
        stats: &StorageStats,
        clearing: bool,
    ) -> Div {
        let chat_rows: Vec<(i64, i64, i32)> = stats
            .by_chat
            .iter()
            .map(|row| (row.chat_id, row.size, row.count))
            .collect();
        let top = top_chats_by_size(&chat_rows, 10);
        if top.is_empty() {
            return section;
        }
        let mut chat_body = TableBody::new();
        for (chat_id, size, count) in top {
            let title = self.storage_chat_title(chat_id);
            let title = if chat_id == 0 {
                "Other chats".to_string()
            } else {
                title
            };
            chat_body = chat_body.child(
                TableRow::new()
                    .child(Self::table_cell(div().text_sm().child(title.clone())))
                    .child(
                        Self::table_cell(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(format!("{count} files · {}", format_storage_bytes(size))),
                        )
                        .text_right(),
                    )
                    .child(
                        Self::table_cell(
                            Button::new(format!("local-storage-clear-chat-{chat_id}"))
                                .label("Clear")
                                .ghost()
                                .small()
                                .disabled(clearing)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.storage_confirm = Some(StorageClear::Chat(chat_id));
                                    cx.notify();
                                })),
                        )
                        .text_right(),
                    ),
            );
        }
        section.child(subheading("Chats")).child(
            Table::new()
                .with_ix(1)
                .accessibility_label("Storage by chat")
                .w_full()
                .child(chat_body),
        )
    }

    /// "Total size limit" and "Clear files older than" as chips.
    pub(super) fn local_storage_limits(&self, cx: &mut Context<Self>) -> Div {
        let limits = self.session().map(|s| s.storage_limits).unwrap_or_default();
        let size = limits.size_limit();
        let keep = limits.keep_for();
        let mut sizes = div().flex().flex_wrap().gap_1();
        for bytes in SIZE_LIMITS {
            sizes = sizes.child(
                Button::new(format!("local-storage-size-{bytes}"))
                    .label(size_limit_label(bytes))
                    .small()
                    .when(size == Some(bytes), |b| b.primary())
                    .when(size != Some(bytes), |b| b.outline())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.apply_storage_limits(Some(bytes), keep, cx);
                    })),
            );
        }
        sizes = sizes.child(
            Button::new("local-storage-size-none")
                .label("No limit")
                .small()
                .when(size.is_none(), |b| b.primary())
                .when(size.is_some(), |b| b.outline())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.apply_storage_limits(None, keep, cx);
                })),
        );
        let mut ages = div().flex().flex_wrap().gap_1();
        for secs in KEEP_LIMITS {
            ages = ages.child(
                Button::new(format!("local-storage-keep-{secs}"))
                    .label(keep_label(secs))
                    .small()
                    .when(keep == Some(secs), |b| b.primary())
                    .when(keep != Some(secs), |b| b.outline())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.apply_storage_limits(size, Some(secs), cx);
                    })),
            );
        }
        ages = ages.child(
            Button::new("local-storage-keep-never")
                .label("Never")
                .small()
                .when(keep.is_none(), |b| b.primary())
                .when(keep.is_some(), |b| b.outline())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.apply_storage_limits(size, None, cx);
                })),
        );
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(subheading("Manage limits"))
            .child(div().text_sm().child("Total size limit"))
            .child(sizes)
            .child(div().text_sm().child("Clear files older than"))
            .child(ages)
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        "If your cache size exceeds this limit, the oldest unused media will be removed from the device.",
                    ),
            )
    }

    fn apply_storage_limits(
        &mut self,
        size: Option<i64>,
        keep: Option<i64>,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.set_storage_limits(size, keep);
        } else if let Some(demo) = self.demo_session.as_mut() {
            for (name, value) in options_for(size, keep) {
                let value = match value {
                    StorageOptionValue::Boolean(on) => {
                        quill::telegram::envelope::OptionValue::Boolean(on)
                    }
                    StorageOptionValue::Integer(n) => {
                        quill::telegram::envelope::OptionValue::Integer(n)
                    }
                };
                demo.storage_limits.apply_option(name, &value);
            }
        }
        cx.notify();
    }
}

fn categories_selected(app: &QuillApp) -> Vec<&'static str> {
    app.storage_selected.iter().copied().collect()
}
