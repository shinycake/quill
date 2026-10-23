//! Batch 6: Data & Storage → local storage (tdesktop
//! `Settings::LocalStorage`): usage by type with "Clear selected" /
//! "Clear all", per-chat "Clear", the "{size} freed on your device!"
//! result, and the limits ("Total size limit", "Clear files older than").
//! The cache is TDLib's: clearing is `optimizeStorage`, the limits are the
//! options of its storage optimizer.

use super::super::app::QuillApp;
use super::super::index_sliders::StepSlider;
use super::data_storage::format_storage_bytes;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::table::{Table, TableBody, TableRow};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::data_settings::top_chats_by_size;
use quill::lang::{keys, tr, tr_with, trn_with};
use quill::limit_steps::{keep_limit_index, size_limit_index};
use quill::storage_limits::{StorageOptionValue, keep_label, options_for, size_limit_label};
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
                .child(
                    div()
                        .font_semibold()
                        .text_sm()
                        .child(tr(keys::LOCAL_STORAGE_DEVICE_TOTAL)),
                )
                .child(
                    div()
                        .text_sm()
                        .child(format_storage_bytes(stats.total_size)),
                ),
        );
        for category in &categories {
            let size_text = trn_with(
                keys::Q_OPS_FILES,
                category.count,
                &[("size", &(format_storage_bytes(category.size)).to_string())],
            );
            let row = div().flex().items_center().justify_between().gap_2();
            let label = category.label;
            let right = div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(size_text);
            section = section.child(match category.file_type {
                Some(file_type) => {
                    let checked = self.settings.storage_selected.contains(file_type);
                    row.child(
                        Checkbox::new(format!("local-storage-type-{file_type}"))
                            .label(label)
                            .checked(checked)
                            .on_click(cx.listener(move |this, &on: &bool, _, cx| {
                                if on {
                                    this.settings.storage_selected.insert(file_type);
                                } else {
                                    this.settings.storage_selected.remove(file_type);
                                }
                                this.settings.storage_confirm = None;
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
                    .is_some_and(|t| self.settings.storage_selected.contains(t))
            })
            .map(|c| c.size)
            .sum();
        let clearing = session
            .as_ref()
            .is_some_and(|s| s.settings.storage_clearing);
        let freed = session.as_ref().and_then(|s| s.settings.storage_freed);
        section = section.child(self.local_storage_actions(
            cx,
            &categories_selected(self),
            selected_size,
            clearing,
        ));
        if clearing {
            section = section
                .child(div().text_sm().child(tr(keys::LOCAL_STORAGE_CLEARING)))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(keys::LOCAL_STORAGE_CLEARING_ABOUT)),
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
                .child(tr(keys::LOCAL_STORAGE_ABOUT)),
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
        let Some(pending) = self.settings.storage_confirm else {
            return div()
                .flex()
                .flex_wrap()
                .gap_2()
                .child(
                    Button::new("local-storage-clear-selected")
                        .label(tr(keys::LOCAL_STORAGE_CLEAR_SELECTED))
                        .outline()
                        .disabled(selected.is_empty() || clearing)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.settings.storage_confirm = Some(StorageClear::Selected);
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("local-storage-clear-all")
                        .label(tr(keys::LOCAL_STORAGE_CLEAR))
                        .outline()
                        .disabled(clearing)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.settings.storage_confirm = Some(StorageClear::All);
                            cx.notify();
                        })),
                );
        };
        let question = match pending {
            StorageClear::All => tr(keys::Q_OPS_CLEAR_ALL_CACHED_FILES).to_string(),
            StorageClear::Selected => tr_with(
                keys::Q_LEFTOVERS_CLEAR_SELECTED_SURE,
                &[
                    ("count", &(selected.len()).to_string()),
                    ("kind", (if selected.len() == 1 { "type" } else { "types" })),
                    ("size", &(format_storage_bytes(selected_size)).to_string()),
                ],
            ),
            StorageClear::Chat(chat_id) => tr_with(
                keys::Q_OPS_CLEAR_THE_CACHED_FILES_OF,
                &[(
                    "storage_chat_title",
                    &(self.storage_chat_title(chat_id)).to_string(),
                )],
            ),
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
                            .label(tr(keys::LOCAL_STORAGE_CLEAR_SOME))
                            .primary()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.run_storage_clear(cx);
                            })),
                    )
                    .child(
                        Button::new("local-storage-clear-cancel")
                            .label(tr(keys::Q_OPS_KEEP))
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.settings.storage_confirm = None;
                                cx.notify();
                            })),
                    ),
            )
    }

    fn storage_chat_title(&self, chat_id: i64) -> String {
        if chat_id == 0 {
            return tr(keys::Q_OPS_OTHER_CHATS).to_string();
        }
        self.session()
            .and_then(|s| s.chats.get(&chat_id).map(|c| c.title.clone()))
            .unwrap_or_else(|| format!("chat {chat_id}"))
    }

    /// Send the confirmed clear (`optimizeStorage`).
    fn run_storage_clear(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = self.settings.storage_confirm.take() else {
            return;
        };
        let (types, chats): (Vec<&'static str>, Vec<i64>) = match pending {
            StorageClear::All => (Vec::new(), Vec::new()),
            StorageClear::Selected => (
                self.settings.storage_selected.iter().copied().collect(),
                Vec::new(),
            ),
            StorageClear::Chat(chat_id) => (Vec::new(), vec![chat_id]),
        };
        if pending == StorageClear::Selected {
            self.settings.storage_selected.clear();
        }
        if let Some(live) = self.live.as_mut() {
            // Send failures land on `data_storage_error`, shown on this
            // dialog (the S3 pattern).
            let _ = live.driver.clear_storage(&types, &chats);
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.settings.storage_stats = None;
            demo.settings.storage_freed = Some(54_525_952);
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
                tr(keys::Q_OPS_OTHER_CHATS_2).to_string()
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
                                .child(trn_with(
                                    keys::Q_OPS_FILES_2,
                                    count,
                                    &[(
                                        "format_storage_bytes",
                                        format_storage_bytes(size).as_str(),
                                    )],
                                )),
                        )
                        .text_right(),
                    )
                    .child(
                        Self::table_cell(
                            Button::new(format!("local-storage-clear-chat-{chat_id}"))
                                .label(tr(keys::LOCAL_STORAGE_CLEAR_SOME))
                                .ghost()
                                .small()
                                .disabled(clearing)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.settings.storage_confirm =
                                        Some(StorageClear::Chat(chat_id));
                                    cx.notify();
                                })),
                        )
                        .text_right(),
                    ),
            );
        }
        section.child(subheading(tr(keys::RECENT_CHATS))).child(
            Table::new()
                .with_ix(1)
                .accessibility_label(tr(keys::Q_OPS_STORAGE_BY_CHAT))
                .w_full()
                .child(chat_body),
        )
    }

    /// Start the two limit sliders on what TDLib reports. They are made
    /// once per visit (`open_data_storage` drops them).
    pub(super) fn ensure_storage_sliders(&mut self, cx: &mut Context<Self>) {
        let sliders = &self.settings.index_sliders;
        if sliders.contains(StepSlider::StorageSize) && sliders.contains(StepSlider::StorageKeep) {
            return;
        }
        let limits = self
            .session()
            .map(|s| s.settings.storage_limits)
            .unwrap_or_default();
        self.set_step_slider(
            StepSlider::StorageSize,
            size_limit_index(limits.size_limit()),
            cx,
        );
        self.set_step_slider(
            StepSlider::StorageKeep,
            keep_limit_index(limits.keep_for()),
            cx,
        );
    }

    /// tdesktop `setupLimits`: "Total size limit" and "Clear files older
    /// than" as sliders over tdesktop's steps. TDLib has no separate media
    /// limit, so that slider has no counterpart.
    pub(super) fn local_storage_limits(&self, cx: &mut Context<Self>) -> Div {
        let (size, keep) = self.storage_slider_limits(cx);
        let size_value =
            size.map_or_else(|| tr(keys::Q_OPS_NO_LIMIT).to_string(), size_limit_label);
        let keep_value = keep.map_or_else(
            || tr(keys::LOCAL_STORAGE_LIMIT_NEVER).to_string(),
            keep_label,
        );
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(subheading(tr(keys::LOCAL_STORAGE_MANAGE_TITLE)))
            .children(self.step_slider_row(
                StepSlider::StorageSize,
                "local-storage-size-limit",
                tr(keys::LOCAL_STORAGE_SIZE_LIMIT_TITLE).to_string(),
                size_value,
                cx,
            ))
            .children(self.step_slider_row(
                StepSlider::StorageKeep,
                "local-storage-time-limit",
                tr(keys::LOCAL_STORAGE_TIME_LIMIT_TITLE).to_string(),
                keep_value,
                cx,
            ))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::LOCAL_STORAGE_MAX_SIZE_ABOUT)),
            )
    }

    /// The release of a limit slider: send the limits both sliders are on.
    pub(in crate::ui) fn apply_storage_sliders(&mut self, cx: &mut Context<Self>) {
        let (size, keep) = self.storage_slider_limits(cx);
        self.apply_storage_limits(size, keep, cx);
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
                demo.settings.storage_limits.apply_option(name, &value);
            }
        }
        cx.notify();
    }
}

fn categories_selected(app: &QuillApp) -> Vec<&'static str> {
    app.settings.storage_selected.iter().copied().collect()
}
