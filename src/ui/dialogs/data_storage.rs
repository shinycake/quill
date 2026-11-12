//! Settings › Data and storage: the export, automatic media download
//! (`auto_download`), the download folder, the storage-usage breakdown with
//! per-chat rows (`getStorageStatistics` with `chat_limit` 50) and "Clear
//! cache" (`removeAllFilesFromDownloads`), and network usage.

use super::super::app::QuillApp;
use super::super::index_sliders::StepSlider;
use super::super::*;
use crate::ui::settings_window::rows::{detail_row, group};
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::data_settings::{AutoDownloadNetSettings, DataStoragePrefs};
use quill::lang::{keys, tr, tr_with};
use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};
/// Slice S4: bytes formatter that handles 0 and GB — the shared
/// `format_bytes` returns "" for 0 and tops out at MB.
pub(crate) fn format_storage_bytes(n: i64) -> String {
    if n <= 0 {
        tr(keys::Q_OPS_0_B).to_string()
    } else if n < 1024 {
        tr_with(keys::Q_OPS_B, &[("n", &n.to_string())])
    } else if n < 1024 * 1024 {
        tr_with(keys::Q_OPS_KB, &[("n_1024", &(n / 1024).to_string())])
    } else if n < 1024 * 1024 * 1024 {
        format!("{:.1} MB", n as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", n as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// How long a scroll request waits for a frame of the settings window.
const SCROLL_REQUEST_LIFE: Duration = Duration::from_secs(2);

/// Where the scroll to the storage list stands. `placed` is the page's
/// vertical offset the last scroll left (set a frame after it).
enum StorageScrollStage {
    Idle,
    /// A scroll to the list is due on the next frame of the settings
    /// window. A request no frame took by `until` (the executor clock) is
    /// dropped, so settings opened much later do not scroll by themselves.
    Requested {
        until: Instant,
    },
    /// The list was scrolled to.
    Placed {
        placed: Rc<Cell<Option<Pixels>>>,
    },
}

pub(crate) struct StorageScroll {
    pub(in crate::ui) page: ScrollHandle,
    usage: ScrollAnchor,
    stage: StorageScrollStage,
}

impl StorageScroll {
    pub(crate) fn new() -> Self {
        let page = ScrollHandle::new();
        let usage = ScrollAnchor::for_handle(page.clone());
        Self {
            page,
            usage,
            stage: StorageScrollStage::Idle,
        }
    }
}

impl QuillApp {
    pub(crate) fn open_local_storage(&mut self, cx: &mut Context<Self>) {
        self.open_data_storage(cx);
        if self.settings.open()
            && self.settings.page() == Some(quill::settings_catalog::Page::DataStorage)
        {
            self.settings.storage_scroll.stage = StorageScrollStage::Requested {
                until: cx.background_executor().now() + SCROLL_REQUEST_LIFE,
            };
        }
    }

    pub(in crate::ui) fn ensure_storage_scroll(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let scroll = &mut self.settings.storage_scroll;
        let offset = scroll.page.offset().y;
        let user_moved = match &scroll.stage {
            StorageScrollStage::Placed { placed } => {
                placed.get().is_some_and(|placed| placed != offset)
            }
            StorageScrollStage::Idle | StorageScrollStage::Requested { .. } => false,
        };
        if user_moved {
            scroll.stage = StorageScrollStage::Idle;
        }
        let now = cx.background_executor().now();
        if matches!(scroll.stage, StorageScrollStage::Requested { until } if now >= until) {
            scroll.stage = StorageScrollStage::Idle;
        }
        if matches!(scroll.stage, StorageScrollStage::Requested { .. }) {
            let placed: Rc<Cell<Option<Pixels>>> = Rc::default();
            scroll.stage = StorageScrollStage::Placed {
                placed: placed.clone(),
            };
            scroll.usage.scroll_to(window, cx);
            let page = scroll.page.clone();
            // Runs after the anchor's own next-frame scroll. The new offset
            // paints nothing on its own.
            window.on_next_frame(move |window, _| {
                placed.set(Some(page.offset().y));
                window.refresh();
            });
        }
    }

    /// Slice S4: open the Data & Storage dialog — reset the transient
    /// view state, clear the last "Cache cleared" note, and fire the
    /// storage + presets fetches (both guarded: once per session).
    pub(crate) fn open_data_storage(&mut self, cx: &mut Context<Self>) {
        self.open_settings(Some(quill::settings_catalog::Page::DataStorage), cx);
        self.close_auto_download();
        self.forget_storage_sliders();
        self.settings.storage_confirm = None;
        if let Some(live) = self.live.as_mut() {
            live.driver.session.settings.reset_storage_freed();
            let _ = live.driver.maybe_fetch_storage_statistics();
            let _ = live.driver.fetch_network_statistics();
        }
        self.privacy.extra.network_reset = Default::default();
        if let Some(demo) = self.demo_session.as_mut() {
            demo.settings.reset_storage_freed();
            if demo.settings.privacy_data.network.data().is_none() {
                demo.settings.privacy_data.network =
                    quill::state::fetch::Fetch::Ready(super::data_extra::demo_network_usage());
            }
        }
        cx.notify();
    }

    /// The Data & Storage settings page: an automatic media download box
    /// when one is open, otherwise the overview with a refresh button.
    pub(crate) fn data_storage_page(&mut self, cx: &mut Context<Self>) -> Div {
        self.settle_storage_sliders(cx);
        self.ensure_storage_sliders(cx);
        if let Some(editor) = self.auto_download_editor_page(cx) {
            return editor;
        }
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(self.data_storage_body(cx))
            .child(
                div().flex().child(
                    Button::new("storage-refresh")
                        .label(tr(keys::Q_OPS_REFRESH))
                        .outline()
                        .small()
                        .on_click(cx.listener(|this, _, _, cx| this.refresh_storage_usage(cx))),
                ),
            )
    }

    /// Leaving the page drops an unsaved editor and any pending confirm.
    pub(crate) fn leave_data_storage(&mut self) {
        self.apply_unreleased_storage_drag();
        self.settings.storage_scroll.stage = StorageScrollStage::Idle;
        self.close_auto_download();
        self.settings.storage_confirm = None;
        self.privacy.extra.network_reset = Default::default();
        self.forget_storage_sliders();
    }

    /// Arriving on the page: the sliders start from the account shown, and
    /// a failure line from an earlier visit goes (only a success cleared it).
    pub(crate) fn arrive_data_storage(&mut self) {
        self.forget_storage_sliders();
        if let Some(session) = self.session_mut() {
            session.settings.data_storage_save.clear();
            if session.settings.storage_clear.failed().is_some() {
                session.settings.storage_clear = quill::state::StorageClear::Idle;
            }
        }
    }

    /// The two limit sliders hold the account's TDLib limits: each visit
    /// of the page starts them again from the account shown
    /// (`ensure_storage_sliders`). An account switch leaves no page, so
    /// the page's arrival drops them too.
    pub(crate) fn forget_storage_sliders(&mut self) {
        let sliders = &mut self.index_sliders;
        sliders.remove(StepSlider::StorageSize);
        sliders.remove(StepSlider::StorageKeep);
    }

    /// Slice S4: the overview — error line, the three auto-download
    /// network rows, the less-data-for-calls toggle, the storage
    /// breakdown (totals, per-type, per-chat), and clear cache.
    pub(crate) fn data_storage_body(&self, cx: &mut Context<Self>) -> Div {
        let session = self.session();
        let mut body = div().flex().flex_col().gap_4();
        // Failures surface here, never as toasts (the S3 pattern).
        if let Some(err) = session
            .as_ref()
            .and_then(|s| s.settings.data_storage_error().map(str::to_owned))
        {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(danger())
                    .child(tr_with(keys::Q_OPS_ERROR, &[("err", &err.to_string())])),
            );
        }
        // `parity:data-export-full`: the full export has its own box.
        let running = session
            .as_ref()
            .and_then(|s| s.settings.account_export.as_ref())
            .map(|export| export.progress().label());
        let export = detail_row(
            "account-export-open",
            if running.is_some() {
                tr(keys::Q_OPS_SHOW_EXPORT_PROGRESS)
            } else {
                tr(keys::Q_OPS_EXPORT_TELEGRAM_DATA)
            },
            running
                .unwrap_or_else(|| tr(keys::Q_OPS_SAVE_YOUR_ACCOUNT_INFORMATION_CONTACTS).into()),
            cx.listener(|this, _, _, cx| this.open_account_export(cx)),
            cx,
        )
        .disabled(self.live.is_none())
        .into_any_element();
        body = body.child(self.setting_spot(
            &["data.export"],
            group(Some(tr(keys::SETTINGS_EXPORT_DATA)), [export]),
            cx,
        ));
        // --- Automatic media download ---
        body = body.child(self.auto_download_section(cx));
        // --- Download folder (B13) ---
        body = body.child(self.download_folder_section(cx));
        // --- Storage usage ---
        let stats = session
            .as_ref()
            .and_then(|s| s.settings.storage_stats.data().cloned());
        let loading = session.is_some_and(|s| s.settings.storage_stats.is_loading());
        let stats_error = session
            .and_then(|s| s.settings.storage_stats.error())
            .map(str::to_owned);
        let mut usage = div().flex().flex_col().gap_2().px_3().py_2();
        let has_stats = stats.is_some();
        usage = match stats {
            None if stats_error.is_some() => usage
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().danger)
                        .child(stats_error.unwrap_or_default()),
                )
                .child(
                    Button::new("storage-usage-retry")
                        .label(tr(keys::Q_MONEY_RETRY))
                        .small()
                        .on_click(cx.listener(|this, _, _, cx| this.refresh_storage_usage(cx))),
                ),
            None => usage.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(if loading {
                        tr(keys::Q_OPS_LOADING)
                    } else {
                        tr(keys::Q_OPS_NO_STORAGE_DATA_YET)
                    }),
            ),
            Some(stats) => usage.child(self.local_storage_section(cx, &stats)),
        };
        // Telegram Desktop keeps no message database; TDLib's stays and
        // is not part of the cache above, so its size is shown apart.
        if let Some(bytes) = self.disk.database_bytes {
            let size = format_storage_bytes(i64::try_from(bytes).unwrap_or(i64::MAX));
            usage = usage.child(
                div()
                    .id("storage-database-size")
                    .debug_selector(|| "storage-database-size".into())
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr_with(keys::Q_DISK_DATABASE_SIZE, &[("size", &size)])),
            );
        }
        let usage = self.setting_spot(
            &["data.storage_usage"],
            div()
                .id("storage-usage")
                .anchor_scroll(Some(self.settings.storage_scroll.usage.clone()))
                .child(group(
                    Some(tr(keys::Q_OPS_STORAGE_USAGE)),
                    [usage.into_any_element()],
                )),
            cx,
        );
        body = body.child(self.setting_stand_in(
            !has_stats,
            &["data.clear_cache", "data.size_limit", "data.clear_older"],
            usage,
            cx,
        ));
        // --- Network usage (B13) ---
        body = body.child(self.network_usage_section(cx));
        body
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
