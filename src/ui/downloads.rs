//! media download queue panel.

use super::app::QuillApp;
use super::message_media::{download_display_name, format_bytes};
use super::pressable::PressableDiv;
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::{ChatId, FileId};
use std::path::PathBuf;
impl QuillApp {
    pub(super) fn request_media_download(
        &mut self,
        file_id: FileId,
        sponsored: Option<(ChatId, i64)>,
        cx: &mut Context<Self>,
    ) {
        if file_id.0 == 0 {
            return;
        }
        if let Some((chat_id, message_id)) = sponsored {
            self.click_sponsored_message(chat_id, message_id, true, cx);
        }
        if let Some(live) = self.live.as_mut() {
            let result = live.driver.download_user_file(file_id, sponsored);
            self.status_note = match result {
                Ok(Some(_)) => "downloading…".into(),
                Ok(None) => "already local or in progress".into(),
                Err(_) => "could not download".into(),
            };
        } else if self.demo_session.is_some() {
            self.status_note = "demo — addFileToDownloads runs with live TDLib".into();
        }
        cx.notify();
    }

    /// MED3: open a fully-downloaded file with the system viewer
    /// (`platform::open_local_file` — `xdg-open` / `open`).
    pub(super) fn open_downloaded_file(&mut self, file_id: FileId, cx: &mut Context<Self>) {
        let path: Option<PathBuf> = self
            .session()
            .and_then(|s| s.files.get(&file_id.0))
            .and_then(|f| f.usable_path())
            .map(PathBuf::from);
        self.status_note = match path {
            Some(path) if quill::platform::open_local_file(&path) => "opened file".into(),
            _ => "could not open the file".into(),
        };
        cx.notify();
    }

    /// MED3: reveal a downloaded file in the file manager
    /// (`platform::reveal_in_file_manager`).
    pub(super) fn reveal_downloaded_file(&mut self, file_id: FileId, cx: &mut Context<Self>) {
        let path: Option<PathBuf> = self
            .session()
            .and_then(|s| s.files.get(&file_id.0))
            .and_then(|f| f.usable_path())
            .map(PathBuf::from);
        self.status_note = match path {
            Some(path) if quill::platform::reveal_in_file_manager(&path) => {
                "revealed in file manager".into()
            }
            _ => "could not reveal the file".into(),
        };
        cx.notify();
    }

    /// Cancel an in-flight download: listed (user-initiated) downloads go
    /// through `removeFileFromDownloads`; one-shot automatic downloads use
    /// `cancelDownloadFile` (TGX's cancel button on downloading media).
    pub(super) fn cancel_media_download(&mut self, file_id: FileId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.cancel_download(file_id) {
                Ok(true) => "download canceled".into(),
                Ok(false) => "nothing to cancel".into(),
                Err(_) => "could not cancel the download".into(),
            };
        } else if self.demo_session.is_some() {
            self.status_note = "demo — cancelDownloadFile runs with live TDLib".into();
        }
        cx.notify();
    }

    /// Slice media-downloads-pause: pause a user-initiated download
    /// (`toggleDownloadIsPaused`; the pause state arrives on
    /// `updateFileDownload`).
    pub(super) fn pause_media_download(&mut self, file_id: FileId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.pause_download(file_id) {
                Ok(true) => "download paused".into(),
                Ok(false) => "nothing to pause".into(),
                Err(_) => "could not pause the download".into(),
            };
        } else if self.demo_session.is_some() {
            self.status_note = "demo — toggleDownloadIsPaused runs with live TDLib".into();
        }
        cx.notify();
    }

    /// Slice media-downloads-pause: resume a paused user-initiated download.
    pub(super) fn resume_media_download(&mut self, file_id: FileId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.resume_download(file_id) {
                Ok(true) => "download resumed".into(),
                Ok(false) => "nothing to resume".into(),
                Err(_) => "could not resume the download".into(),
            };
        } else if self.demo_session.is_some() {
            self.status_note = "demo — toggleDownloadIsPaused runs with live TDLib".into();
        }
        cx.notify();
    }

    /// MED3: downloads manager panel (TGX side-menu Downloads, empty state
    /// `NoDownloadFilesFound`). Lists user-initiated active downloads with
    /// live progress + cancel, and recently completed downloads with
    /// open / reveal actions. Sits beside the conversation like the info
    /// panel; toggled from the sidebar "Downloads" entry.
    pub(super) fn downloads_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        if !session.downloads_panel_open {
            return None;
        }
        let mut active: Vec<i32> = session.user_downloads.iter().copied().collect();
        active.sort_unstable();
        let recent: Vec<i32> = session.completed_downloads.iter().copied().collect();
        let mut failed: Vec<i32> = session.failed_downloads.iter().copied().collect();
        failed.sort_unstable();
        let mut panel = div()
            .id("downloads-panel")
            .w(px(300.))
            .h_full()
            .flex_shrink_0()
            .border_l_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(div().font_semibold().child("Downloads"))
                    .child(
                        Button::new("downloads-panel-close")
                            .accessibility_label("Close downloads")
                            .icon(IconName::X)
                            .ghost()
                            .tooltip("Close downloads")
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(live) = this.live.as_mut() {
                                    live.driver.session.downloads_panel_open = false;
                                } else if let Some(session) = this.demo_session.as_mut() {
                                    session.downloads_panel_open = false;
                                }
                                cx.notify();
                            })),
                    ),
            );
        if active.is_empty() && recent.is_empty() {
            panel = panel.child(
                div()
                    .px_3()
                    .py_4()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No downloads yet."),
            );
        } else {
            if !active.is_empty() {
                panel = panel.child(
                    div()
                        .px_3()
                        .pt_2()
                        .text_xs()
                        .font_semibold()
                        .text_color(cx.theme().muted_foreground)
                        .child("Active"),
                );
                for file_id in active {
                    panel = panel.child(self.download_row(file_id, true, false, cx));
                }
            }
            if !failed.is_empty() {
                panel = panel.child(
                    div()
                        .px_3()
                        .pt_2()
                        .text_xs()
                        .font_semibold()
                        .text_color(cx.theme().muted_foreground)
                        .child("Failed"),
                );
                for file_id in failed {
                    panel = panel.child(self.download_row(file_id, false, true, cx));
                }
            }
            if !recent.is_empty() {
                panel = panel.child(
                    div()
                        .px_3()
                        .pt_2()
                        .text_xs()
                        .font_semibold()
                        .text_color(cx.theme().muted_foreground)
                        .child("Recent"),
                );
                for file_id in recent.into_iter().rev() {
                    panel = panel.child(self.download_row(file_id, false, false, cx));
                }
            }
        }
        Some(panel.into_any_element())
    }

    /// MED3: one downloads-manager row. Active rows show the live progress
    /// bar + percent + pause/resume and cancel buttons; failed rows show a
    /// retry button; recent rows show size + open / reveal actions.
    pub(super) fn download_row(
        &self,
        file_id: i32,
        active: bool,
        failed: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let session = self.session();
        let file = session.as_ref().and_then(|s| s.files.get(&file_id));
        let name = session
            .as_ref()
            .map(|s| download_display_name(s, file_id))
            .unwrap_or_else(|| format!("File {file_id}"));
        let size_label = file
            .map(|f| f.display_size())
            .map(format_bytes)
            .unwrap_or_default();
        let progress = file.and_then(|f| f.download_progress());
        // Slice media-downloads-pause: paused is a subset of the active
        // user downloads (`Session::paused_downloads`).
        let paused = active
            && session
                .as_ref()
                .is_some_and(|s| s.paused_downloads.contains(&file_id));
        let status = if paused {
            "paused".to_string()
        } else if active {
            match progress {
                Some(p) => format!("{}%", (p * 100.0).round() as i32),
                None => "downloading…".to_string(),
            }
        } else if failed {
            "download failed".to_string()
        } else {
            size_label.clone()
        };
        let mut row = div()
            .id(("download-row", file_id as u64))
            .px_3()
            .py_2()
            .flex()
            .flex_col()
            .gap_1()
            .child(div().text_sm().font_medium().child(name))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(status),
            );
        if active {
            row = row.child(
                div().w_full().h(px(4.)).rounded_full().bg(border()).child(
                    div()
                        .h_full()
                        .w(relative(progress.unwrap_or(0.0)))
                        .rounded_full()
                        .bg(accent()),
                ),
            );
        }
        let actions = if active {
            div()
                .flex()
                .gap_2()
                .child(
                    div()
                        .id(("download-pause", file_id as u64))
                        .role(gpui_kit::Role::Button)
                        .aria_label("Pause or resume download")
                        .tab_index(0)
                        .cursor_pointer()
                        .pressable(cx.theme())
                        .text_xs()
                        .text_color(accent())
                        .child(if paused { "Resume" } else { "Pause" })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if paused {
                                this.resume_media_download(FileId(file_id), cx);
                            } else {
                                this.pause_media_download(FileId(file_id), cx);
                            }
                        })),
                )
                .child(
                    div()
                        .id(("download-cancel", file_id as u64))
                        .role(gpui_kit::Role::Button)
                        .aria_label("Cancel download")
                        .tab_index(0)
                        .cursor_pointer()
                        .pressable(cx.theme())
                        .text_xs()
                        .text_color(accent())
                        .child("Cancel")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.cancel_media_download(FileId(file_id), cx);
                        })),
                )
        } else if failed {
            div().flex().gap_2().child(
                div()
                    .id(("download-retry", file_id as u64))
                    .role(gpui_kit::Role::Button)
                    .aria_label("Retry download")
                    .tab_index(0)
                    .cursor_pointer()
                    .pressable(cx.theme())
                    .text_xs()
                    .text_color(accent())
                    .child("Retry")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.request_media_download(FileId(file_id), None, cx);
                    })),
            )
        } else {
            div()
                .flex()
                .gap_2()
                .child(
                    div()
                        .id(("download-open", file_id as u64))
                        .role(gpui_kit::Role::Button)
                        .aria_label("Open downloaded file")
                        .tab_index(0)
                        .cursor_pointer()
                        .pressable(cx.theme())
                        .text_xs()
                        .text_color(accent())
                        .child("Open")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_downloaded_file(FileId(file_id), cx);
                        })),
                )
                .child(
                    div()
                        .id(("download-reveal", file_id as u64))
                        .role(gpui_kit::Role::Button)
                        .aria_label("Show downloaded file in Finder")
                        .tab_index(0)
                        .cursor_pointer()
                        .pressable(cx.theme())
                        .text_xs()
                        .text_color(accent())
                        .child("Show in folder")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.reveal_downloaded_file(FileId(file_id), cx);
                        })),
                )
        };
        row.child(actions).into_any_element()
    }
}
