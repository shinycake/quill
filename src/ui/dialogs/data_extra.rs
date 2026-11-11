//! B13: Data & Storage additions — the download folder with "Ask where to
//! save each file" (tdesktop `Settings > Advanced > Download path`) and
//! the network usage statistics with reset (Telegram X's Network Usage;
//! tdesktop has no such screen).

use super::super::app::QuillApp;
use super::data_storage::format_storage_bytes;
use crate::ui::settings_window::rows::{group, switch_row};
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::file_prefs;
use quill::lang::{keys, tr, tr_with};
use quill::local_time::{civil_local, full_stamp};
use quill::network_usage::{NetworkGroup, NetworkUsage};
use quill::voice::format_voice_duration;

impl QuillApp {
    /// "Download folder": where saved files go, and the switch that asks
    /// for a place every time instead.
    pub(super) fn download_folder_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let prefs = file_prefs::current();
        let configured = file_prefs::configured_download_dir();
        let shown = match (&configured, quill::media_viewer::os_downloads_dir()) {
            (Some(dir), _) => dir.display().to_string(),
            (None, Some(dir)) => tr_with(
                keys::Q_OPS_DEFAULT,
                &[("display", &(dir.display()).to_string())],
            ),
            (None, None) => tr(keys::Q_OPS_NOT_AVAILABLE).to_string(),
        };
        let stale = prefs.download_dir.is_some() && configured.is_none();
        let folder = div()
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .id("download-folder-path")
                            .text_sm()
                            .truncate()
                            .child(shown),
                    )
                    .when(stale, |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(tr(keys::Q_OPS_THE_CHOSEN_FOLDER_IS_MISSING)),
                        )
                    }),
            )
            .when(prefs.download_dir.is_some(), |this| {
                this.child(
                    Button::new("download-folder-default")
                        .label(tr(keys::Q_OPS_USE_DEFAULT))
                        .ghost()
                        .small()
                        .on_click(cx.listener(|_, _, _, cx| {
                            file_prefs::update(|p| p.download_dir = None);
                            cx.notify();
                        })),
                )
            })
            .child(
                Button::new("download-folder-choose")
                    .label(tr(keys::Q_OPS_CHOOSE_FOLDER))
                    .outline()
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| this.choose_download_folder(cx))),
            );
        let ask = switch_row(
            "download-folder-ask",
            tr(keys::Q_OPS_ASK_WHERE_TO_SAVE_EACH),
            Some(tr(keys::Q_OPS_CHOOSE_A_FOLDER_EVERY_TIME)),
            prefs.ask_download_path,
            |&on, _, cx| {
                file_prefs::update(|p| p.ask_download_path = on);
                cx.refresh_windows();
            },
            cx,
        );
        let rows = [
            self.setting_spot(&["data.download_folder"], folder, cx),
            self.setting_spot(&["data.ask_where_to_save"], ask, cx),
        ];
        group(Some(tr(keys::Q_OPS_DOWNLOAD_FOLDER)), rows).into_any_element()
    }

    /// The folder picker (the native directory dialog on macOS, Windows
    /// and Linux through GPUI).
    fn choose_download_folder(&mut self, cx: &mut Context<Self>) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(tr(keys::Q_OPS_USE_AS_DOWNLOAD_FOLDER).into()),
        });
        cx.spawn(async move |this, cx| {
            let result = picker.await;
            let _ = this.update(cx, |this, cx| {
                if let Ok(Ok(Some(paths))) = result
                    && let Some(folder) = paths.into_iter().next()
                {
                    file_prefs::update(|p| p.download_dir = Some(folder));
                }
                cx.notify();
                let _ = this;
            });
        })
        .detach();
    }

    /// "Network usage": bytes sent and received per network and category
    /// since the last reset, with a confirmed reset.
    pub(super) fn network_usage_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let body = self.network_usage_body(cx).into_any_element();
        self.setting_spot(
            &["data.network_usage"],
            group(Some(tr(keys::Q_OPS_NETWORK_USAGE)), [body]),
            cx,
        )
    }

    fn network_usage_body(&self, cx: &mut Context<Self>) -> Div {
        let session = self.session();
        let usage: Option<NetworkUsage> =
            session.and_then(|s| s.settings.privacy_data.network_usage.clone());
        let loading = session.is_some_and(|s| s.settings.privacy_data.network_loading);
        let mut section = div().flex().flex_col().gap_2().px_3().py_2();
        let Some(usage) = usage else {
            return section.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(if loading {
                        tr(keys::Q_OPS_LOADING)
                    } else {
                        tr(keys::Q_OPS_NO_NETWORK_USAGE_DATA_YET)
                    }),
            );
        };
        let total = usage.grand_total();
        section = section
            .child(
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
                            .id("network-usage-total")
                            .text_sm()
                            .child(format_storage_bytes(total.total())),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr_with(
                        keys::Q_LEFTOVERS_SENT_RECEIVED,
                        &[
                            ("sent", &(format_storage_bytes(total.sent)).to_string()),
                            (
                                "received",
                                &(format_storage_bytes(total.received)).to_string(),
                            ),
                        ],
                    )),
            );
        let groups = usage.active_groups();
        if groups.is_empty() {
            section = section.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::Q_OPS_NOTHING_HAS_BEEN_SENT_OR)),
            );
        }
        for group in groups {
            section = section.child(network_group_block(cx, &usage, group));
        }
        if usage.since_date > 0 {
            section = section.child(
                div()
                    .id("network-usage-since")
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr_with(
                        keys::Q_LEFTOVERS_NETWORK_USAGE_SINCE,
                        &[(
                            "date",
                            &(full_stamp(&civil_local(usage.since_date))).to_string(),
                        )],
                    )),
            );
        }
        if self.privacy.extra.network_reset == NetworkReset::Confirming {
            section.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .child(tr(keys::Q_OPS_RESET_THE_STATISTICS_THE_COUNTERS)),
                    )
                    .child(
                        Button::new("network-usage-reset-confirm")
                            .label(tr(keys::Q_OPS_RESET))
                            .danger()
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.reset_network_usage();
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("network-usage-reset-cancel")
                            .label(tr(keys::CANCEL))
                            .ghost()
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.privacy.extra.network_reset = NetworkReset::Idle;
                                cx.notify();
                            })),
                    ),
            )
        } else {
            section.child(
                div().flex().child(
                    Button::new("network-usage-reset")
                        .label(tr(keys::Q_OPS_RESET_STATISTICS))
                        .outline()
                        .small()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.privacy.extra.network_reset = NetworkReset::Confirming;
                            cx.notify();
                        })),
                ),
            )
        }
    }
}

/// "Reset statistics": asking for the confirmation, or not.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum NetworkReset {
    #[default]
    Idle,
    Confirming,
}

impl QuillApp {
    /// The confirmed reset. The box stays until the request went out; a
    /// failed send says so on the page (the `data_storage_error` pattern).
    pub(crate) fn reset_network_usage(&mut self) {
        if let Some(live) = self.live.as_mut() {
            if live.driver.reset_network_statistics().is_err() {
                live.driver.session.settings.data_storage_error =
                    Some(tr(keys::Q_OPS_RESET_STATISTICS_FAILED).to_string());
                return;
            }
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.settings.privacy_data.network_usage = Some(NetworkUsage::default());
        }
        self.privacy.extra.network_reset = NetworkReset::Idle;
    }
}

fn network_group_block(
    cx: &mut Context<QuillApp>,
    usage: &NetworkUsage,
    group: NetworkGroup,
) -> Div {
    let total = usage.group_total(group);
    let mut block = div().flex().flex_col().gap_1().child(
        div()
            .flex()
            .items_center()
            .justify_between()
            .child(div().text_sm().font_medium().child(group.label()))
            .child(div().text_sm().child(format_storage_bytes(total.total()))),
    );
    for (kind, traffic) in usage.rows(group) {
        let mut right = format!(
            "↑ {} · ↓ {}",
            format_storage_bytes(traffic.sent),
            format_storage_bytes(traffic.received)
        );
        if kind == quill::network_usage::UsageKind::Calls {
            let seconds = usage.call_seconds(group);
            if seconds > 0 {
                right.push_str(&format!(
                    " · {}",
                    format_voice_duration(i32::try_from(seconds).unwrap_or(i32::MAX))
                ));
            }
        }
        block = block.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .pl_3()
                .child(div().text_xs().child(kind.label()))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(right),
                ),
        );
    }
    block
}

/// Demo-capture fixture (English): a month of usage on Wi-Fi, a little on
/// mobile data, and one call.
pub(crate) fn demo_network_usage() -> NetworkUsage {
    let since = quill::local_time::now_unix() - 21 * 86_400;
    let file = |ty: &str, net: &str, sent: i64, received: i64| {
        serde_json::json!({
            "@type": "networkStatisticsEntryFile",
            "file_type": {"@type": ty},
            "network_type": {"@type": net},
            "sent_bytes": sent,
            "received_bytes": received,
        })
    };
    NetworkUsage::from_value(&serde_json::json!({
        "@type": "networkStatistics",
        "since_date": since,
        "entries": [
            file("fileTypeNone", "networkTypeWiFi", 4_200_000, 18_600_000),
            file("fileTypePhoto", "networkTypeWiFi", 9_800_000, 212_000_000),
            file("fileTypeVideo", "networkTypeWiFi", 31_000_000, 640_000_000),
            file("fileTypeDocument", "networkTypeWiFi", 12_000_000, 88_000_000),
            file("fileTypeVoiceNote", "networkTypeWiFi", 3_100_000, 14_000_000),
            file("fileTypeSticker", "networkTypeWiFi", 0, 27_000_000),
            file("fileTypeNone", "networkTypeMobile", 900_000, 3_400_000),
            file("fileTypePhoto", "networkTypeMobile", 600_000, 21_000_000),
            {
                "@type": "networkStatisticsEntryCall",
                "network_type": {"@type": "networkTypeWiFi"},
                "sent_bytes": 6_400_000,
                "received_bytes": 7_100_000,
                "duration": 842.0
            },
        ],
    }))
}
