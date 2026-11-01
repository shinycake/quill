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
use super::super::index_sliders::{StepSlider, cap_slider_start};
use super::super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::data_settings::{
    AutoDownloadNetSettings, CapKind, DataStoragePrefs, NO_LIMIT_SIZE_CAP, NetworkKind,
};
use quill::lang::{keys, tr, tr_with};
use quill::limit_steps::cap_at;
use std::cell::Cell;
use std::rc::Rc;
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

pub(super) fn section_header(title: &'static str) -> impl IntoElement {
    div()
        .id(format!("storage-heading-{title}"))
        .role(Role::Heading)
        .aria_label(title)
        .text_xs()
        .font_semibold()
        .child(title)
}

pub(crate) struct StorageScroll {
    pub(in crate::ui) page: ScrollHandle,
    usage: ScrollAnchor,
    pending: bool,
    /// The rows above the list were still loading at the last scroll: it
    /// goes again once they arrive, unless the user scrolled meanwhile.
    until_settled: bool,
    /// The page's vertical offset the last scroll left.
    placed: Rc<Cell<Option<Pixels>>>,
}

impl StorageScroll {
    pub(crate) fn new() -> Self {
        let page = ScrollHandle::new();
        let usage = ScrollAnchor::for_handle(page.clone());
        Self {
            page,
            usage,
            pending: false,
            until_settled: false,
            placed: Rc::default(),
        }
    }
}

impl QuillApp {
    pub(crate) fn open_local_storage(&mut self, cx: &mut Context<Self>) {
        self.open_data_storage(cx);
        self.settings.storage_scroll.pending = true;
    }

    pub(in crate::ui) fn ensure_storage_scroll(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let settled = self.session().is_none_or(|session| {
            session.settings.data_storage.seeded || !session.settings.auto_download_presets_loading
        });
        let scroll = &mut self.settings.storage_scroll;
        let user_moved = scroll
            .placed
            .get()
            .is_some_and(|placed| placed != scroll.page.offset().y);
        if user_moved {
            scroll.until_settled = false;
        }
        let again = scroll.until_settled && settled;
        if std::mem::take(&mut scroll.pending) || again {
            scroll.until_settled = !settled;
            scroll.usage.scroll_to(window, cx);
            let (placed, page) = (scroll.placed.clone(), scroll.page.clone());
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
        self.open_settings(Some(quill::settings_catalog::Page::DataStorage));
        self.settings.data_storage_editor = None;
        self.settings.index_sliders.remove(StepSlider::StorageSize);
        self.settings.index_sliders.remove(StepSlider::StorageKeep);
        self.settings.storage_confirm = None;
        if let Some(live) = self.live.as_mut() {
            live.driver.session.settings.storage_freed = None;
            let _ = live.driver.maybe_fetch_storage_statistics();
            let _ = live.driver.fetch_auto_download_presets();
            let _ = live.driver.fetch_network_statistics();
        }
        self.privacy.extra.network_reset_confirm = false;
        if let Some(demo) = self.demo_session.as_mut() {
            demo.settings.storage_freed = None;
            if demo.settings.privacy_data.network_usage.is_none() {
                demo.settings.privacy_data.network_usage =
                    Some(super::data_extra::demo_network_usage());
            }
        }
        cx.notify();
    }

    /// The Data & Storage settings page: the per-network editor when one
    /// is open, otherwise the overview with a refresh button.
    pub(crate) fn data_storage_page(&mut self, cx: &mut Context<Self>) -> Div {
        self.ensure_storage_sliders(cx);
        let Some((network, draft)) = self.settings.data_storage_editor else {
            return div()
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
                );
        };
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_lg().font_semibold().child(tr_with(
                keys::Q_OPS_NETWORK_DOWNLOADS,
                &[("network", network.label())],
            )))
            .child(self.data_storage_editor_body(cx, network, &draft))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("data-storage-back")
                            .label(tr(keys::Q_OPS_BACK))
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.settings.data_storage_editor = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("data-storage-save")
                            .label(tr(keys::SETTINGS_SAVE))
                            .primary()
                            .on_click(
                                cx.listener(|this, _, _, cx| this.save_data_storage_editor(cx)),
                            ),
                    ),
            )
    }

    fn save_data_storage_editor(&mut self, cx: &mut Context<Self>) {
        let Some((network, draft)) = self.settings.data_storage_editor.take() else {
            return;
        };
        if let Some(live) = self.live.as_mut() {
            // Applied on the confirmed `ok`, never optimistically; a failed
            // send restores the draft so nothing is lost.
            if live
                .driver
                .set_auto_download_settings(network, draft)
                .is_err()
            {
                self.settings.data_storage_editor = Some((network, draft));
            }
        } else if let Some(demo) = self.demo_session.as_mut() {
            *demo.settings.data_storage.for_network_mut(network) = draft;
            demo.settings.data_storage.seeded = true;
        }
        cx.notify();
    }

    /// Leaving the page drops an unsaved editor and any pending confirm.
    pub(crate) fn leave_data_storage(&mut self) {
        self.settings.data_storage_editor = None;
        self.settings.storage_confirm = None;
    }

    /// Slice S4: the overview — error line, the three auto-download
    /// network rows, the less-data-for-calls toggle, the storage
    /// breakdown (totals, per-type, per-chat), and clear cache.
    pub(crate) fn data_storage_body(&self, cx: &mut Context<Self>) -> Div {
        let session = self.session();
        let mut body = div().flex().flex_col().gap_3();
        // Failures surface here, never as toasts (the S3 pattern).
        if let Some(err) = session
            .as_ref()
            .and_then(|s| s.settings.data_storage_error.clone())
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
        body = body
            .child(section_header(tr(keys::SETTINGS_EXPORT_DATA)))
            .child(
                div()
                    .text_xs()
                    .child(tr(keys::Q_OPS_SAVE_YOUR_ACCOUNT_INFORMATION_CONTACTS)),
            );
        if let Some(label) = &running {
            body = body.child(
                div()
                    .id("account-export-status")
                    .role(Role::Status)
                    .aria_label(label.clone())
                    .text_sm()
                    .child(label.clone()),
            );
        }
        body = body.child(
            div().flex().child(
                Button::new("account-export-open")
                    .label(if running.is_some() {
                        tr(keys::Q_OPS_SHOW_EXPORT_PROGRESS)
                    } else {
                        tr(keys::Q_OPS_EXPORT_TELEGRAM_DATA)
                    })
                    .outline()
                    .small()
                    .disabled(self.live.is_none())
                    .on_click(cx.listener(|this, _, _, cx| this.open_account_export(cx))),
            ),
        );
        // --- Automatic downloads ---
        body = body.child(section_header(tr(keys::Q_OPS_AUTOMATIC_DOWNLOADS)));
        let prefs = session.as_ref().map(|s| &s.settings.data_storage);
        if !prefs.is_some_and(|p| p.seeded) {
            let loading = session.is_some_and(|s| s.settings.auto_download_presets_loading);
            body = body.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(if loading {
                        tr(keys::Q_OPS_LOADING_DOWNLOAD_SETTINGS)
                    } else {
                        tr(keys::Q_OPS_DOWNLOAD_SETTINGS_UNAVAILABLE)
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
                            .child(
                                div()
                                    .text_sm()
                                    .child(tr(keys::Q_OPS_USE_LESS_DATA_FOR_CALLS)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(tr(keys::Q_OPS_LOWER_DATA_USAGE_DURING_TELEGRAM)),
                            ),
                    )
                    .child(
                        Switch::new("data-storage-less-data-calls")
                            .checked(all_on)
                            .accessibility_label(tr(keys::Q_OPS_USE_LESS_DATA_FOR_CALLS))
                            .on_click(cx.listener(move |this, &on, _, cx| {
                                if let Some(live) = this.live.as_mut() {
                                    // Send failures land on
                                    // `data_storage_error`, shown on
                                    // this dialog (the S3 pattern).
                                    let _ = live.driver.set_less_data_for_calls(on);
                                } else if let Some(demo) = this.demo_session.as_mut() {
                                    for n in NetworkKind::ALL {
                                        demo.settings
                                            .data_storage
                                            .for_network_mut(n)
                                            .use_less_data_for_calls = on;
                                    }
                                }
                                cx.notify();
                            })),
                    ),
            );
        }
        // --- Download folder (B13) ---
        body = body.child(self.download_folder_section(cx));
        // --- Storage usage ---
        body = body.child(
            div()
                .id("storage-usage")
                .anchor_scroll(Some(self.settings.storage_scroll.usage.clone()))
                .child(section_header(tr(keys::Q_OPS_STORAGE_USAGE))),
        );
        let stats = session
            .as_ref()
            .and_then(|s| s.settings.storage_stats.clone());
        let loading = session.is_some_and(|s| s.settings.storage_stats_loading);
        match stats {
            None => {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(if loading {
                            tr(keys::Q_OPS_LOADING)
                        } else {
                            tr(keys::Q_OPS_NO_STORAGE_DATA_YET)
                        }),
                );
            }
            Some(stats) => {
                body = body.child(self.local_storage_section(cx, &stats));
            }
        }
        // --- Network usage (B13) ---
        body = body.child(self.network_usage_section(cx));
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
            .test_support()
            .role(gpui_kit::Role::Button)
            .aria_label(tr_with(
                keys::Q_OPS_AUTOMATIC_DOWNLOAD_SETTINGS,
                &[("label", (network.label()))],
            ))
            .tab_index(0)
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
                    .map(|s| *s.settings.data_storage.for_network(network));
                if let Some(draft) = draft {
                    for kind in CAP_KINDS {
                        let start = draft.cap(kind);
                        let start = if start > 0 {
                            start
                        } else {
                            quill::data_settings::DEFAULT_ENABLED_CAP
                        };
                        this.set_step_slider(
                            StepSlider::DataCap(kind),
                            cap_slider_start(start),
                            cx,
                        );
                    }
                    this.settings.data_storage_editor = Some((network, draft));
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
                        .child(div().text_sm().child(tr(keys::Q_OPS_AUTOMATIC_DOWNLOAD)))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(tr_with(
                                    keys::Q_LEFTOVERS_AUTO_DOWNLOAD_ON,
                                    &[("network", &(network.label().to_lowercase()).to_string())],
                                )),
                        ),
                )
                .child(
                    Switch::new("data-storage-enabled")
                        .checked(draft.is_auto_download_enabled)
                        .accessibility_label(tr(keys::Q_OPS_AUTOMATIC_DOWNLOAD))
                        .on_click(cx.listener(|this, &on, _, cx| {
                            if let Some((_, draft)) = this.settings.data_storage_editor.as_mut() {
                                draft.is_auto_download_enabled = on;
                            }
                            cx.notify();
                        })),
                ),
        );
        body = body.child(section_header(tr(keys::Q_OPS_MAXIMUM_FILE_SIZE)));
        for (kind, id, title) in [
            (CapKind::Photo, "photo", tr(keys::Q_OPS_PHOTOS)),
            (CapKind::Video, "video", tr(keys::Q_OPS_VIDEOS)),
            (CapKind::Other, "other", tr(keys::Q_OPS_OTHER_FILES)),
        ] {
            body = body.child(self.data_storage_cap_row(cx, kind, id, title, draft));
        }
        body = body.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .py_1()
                .child(
                    div()
                        .text_sm()
                        .child(tr(keys::Q_OPS_USE_LESS_DATA_FOR_CALLS)),
                )
                .child(
                    Switch::new("data-storage-editor-less-data-calls")
                        .checked(draft.use_less_data_for_calls)
                        .accessibility_label(tr(keys::Q_OPS_USE_LESS_DATA_FOR_CALLS))
                        .on_click(cx.listener(|this, &on, _, cx| {
                            if let Some((_, draft)) = this.settings.data_storage_editor.as_mut() {
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
                .child(tr(keys::Q_OPS_SAVED_ON_THIS_DEVICE_APPLIES)),
        );
        body
    }

    /// One media type of the editor (tdesktop `AutoDownloadBox`): its
    /// switch, and while it is on the "Limit by size" slider
    /// (`AddSizeLimitSlider`: `lng_media_size_limit` / `lng_media_size_up_to`).
    fn data_storage_cap_row(
        &self,
        cx: &mut Context<Self>,
        kind: CapKind,
        id: &'static str,
        title: &str,
        draft: &AutoDownloadNetSettings,
    ) -> Div {
        let cap = draft.cap(kind);
        let value = if cap >= NO_LIMIT_SIZE_CAP {
            tr(keys::Q_OPS_NO_LIMIT).to_string()
        } else {
            tr_with(
                keys::MEDIA_SIZE_UP_TO,
                &[(
                    "size",
                    tr_with(
                        keys::Q_LEFTOVERS_MB,
                        &[("cap", &(cap / (1024 * 1024)).to_string())],
                    )
                    .as_str(),
                )],
            )
        };
        let slider = (cap > 0)
            .then(|| {
                self.step_slider_row(
                    StepSlider::DataCap(kind),
                    cap_slider_id(kind),
                    tr(keys::MEDIA_SIZE_LIMIT).to_string(),
                    value,
                    cx,
                )
            })
            .flatten();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .py_1()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_sm().child(title.to_string()))
                    .child(
                        Switch::new(format!("data-storage-cap-{id}"))
                            .checked(cap > 0)
                            .accessibility_label(title.to_string())
                            .on_click(cx.listener(move |this, &on, _, cx| {
                                // Switching on resumes at the slider's step.
                                let index = this
                                    .settings
                                    .index_sliders
                                    .index(StepSlider::DataCap(kind), cx);
                                if let Some((_, draft)) = this.settings.data_storage_editor.as_mut()
                                {
                                    draft.set_cap_enabled(kind, on);
                                    if let Some(index) = index.filter(|_| on) {
                                        draft.set_cap(kind, cap_at(index));
                                    }
                                }
                                cx.notify();
                            })),
                    ),
            )
            .children(slider)
    }
}

const CAP_KINDS: [CapKind; 3] = [CapKind::Photo, CapKind::Video, CapKind::Other];

fn cap_slider_id(kind: CapKind) -> &'static str {
    match kind {
        CapKind::Photo => "data-storage-limit-photo",
        CapKind::Video => "data-storage-limit-video",
        CapKind::Other => "data-storage-limit-other",
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
