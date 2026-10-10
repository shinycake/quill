//! media download queue panel.

use super::app::QuillApp;
use super::message_media::{download_display_name, format_bytes};
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
                Err(_) => {
                    self.clear_pending_media_playback(file_id);
                    "Could not download. Please try again.".into()
                }
            };
        } else if self.demo_session.is_some() {
            self.status_note = "demo — addFileToDownloads runs with live TDLib".into();
        }
        cx.notify();
    }

    fn clear_pending_media_playback(&mut self, file_id: FileId) {
        self.pending_gif_play.take_if(|(_, id, _)| *id == file_id);
        self.pending_video_play
            .take_if(|(_, id, ..)| *id == file_id);
        self.pending_audio_play
            .take_if(|(_, _, id, _)| *id == file_id);
        self.pending_voice_play
            .take_if(|(_, _, id, ..)| *id == file_id);
        self.viewer_pending_play.take_if(|(_, id)| *id == file_id);
    }

    pub(super) fn discard_stopped_media_playback(&mut self, cx: &mut Context<Self>) {
        let failed: Vec<_> = [
            self.pending_gif_play.as_ref().map(|(_, id, _)| *id),
            self.pending_video_play.as_ref().map(|(_, id, ..)| *id),
            self.pending_audio_play.as_ref().map(|(_, _, id, _)| *id),
            self.viewer_pending_play.map(|(_, id)| id),
            self.pending_voice_play.map(|(_, _, id, ..)| id),
        ]
        .into_iter()
        .flatten()
        .filter(|id| {
            self.session().is_some_and(|s| {
                s.failed_downloads.contains(&id.0)
                    || (s.file(*id).and_then(|f| f.usable_path()).is_none()
                        && !s.downloading.contains(&id.0)
                        && !s.requests.has_download(*id)
                        && !s.file(*id).is_some_and(|f| f.local.is_downloading_active))
            })
        })
        .collect();
        if !failed.is_empty() {
            for file_id in failed {
                self.clear_pending_media_playback(file_id);
            }
            self.status_note = "Download stopped. Press Play to try again.".into();
            cx.notify();
        }
    }

    /// MED3: open a fully-downloaded file with the system viewer
    /// (`platform::open_local_file` — `xdg-open` / `open`).
    pub(super) fn open_downloaded_file(&mut self, file_id: FileId, cx: &mut Context<Self>) {
        let path: Option<PathBuf> = self
            .session()
            .and_then(|s| s.files.get(&file_id.0))
            .and_then(|f| f.usable_path())
            .map(PathBuf::from);
        match path {
            // B13: tdesktop asks before running executables, unknown types
            // and files that may reveal the IP address.
            Some(path) => self.open_file_guarded(path, cx),
            None => {
                self.status_note = "could not open the file".into();
                cx.notify();
            }
        }
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
                Ok(true) => {
                    self.clear_pending_media_playback(file_id);
                    "Download canceled.".into()
                }
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
        if let Some(summary) = session.sync.download_summary(format_bytes) {
            panel = panel.child(
                div()
                    .id("downloads-summary")
                    .px_3()
                    .pt_2()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(summary),
            );
        }
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
        let done = file.map_or(0, |f| f.local.downloaded_size);
        let meta = if active {
            let amount = if done > 0 && !size_label.is_empty() {
                format!("{} of {}", format_bytes(done), size_label)
            } else {
                size_label.clone()
            };
            let lead = if paused {
                "Paused".to_string()
            } else {
                progress.map_or("Downloading…".to_string(), |p| {
                    format!("{}%", (p * 100.0).round() as i32)
                })
            };
            if amount.is_empty() {
                lead
            } else {
                format!("{lead} · {amount}")
            }
        } else if failed {
            "Download failed".to_string()
        } else {
            size_label.clone()
        };
        // The disc is the primary action: pause/resume (inside the progress
        // ring), retry, or open.
        let (icon, label, ring) = if active {
            (
                if paused {
                    IconName::Play
                } else {
                    IconName::Pause
                },
                if paused {
                    "Resume download"
                } else {
                    "Pause download"
                },
                // Paused: a frozen ring; unknown progress: a spinning one.
                Some(if paused {
                    Some(progress.unwrap_or(0.))
                } else {
                    progress
                }),
            )
        } else if failed {
            (IconName::RotateCcw, "Retry download", None)
        } else {
            (IconName::File, "Open file", None)
        };
        let disc = super::message_media::action_disc(
            ("download-disc", file_id as u64),
            false,
            icon,
            ring,
            label,
            cx,
        )
        .size(px(40.))
        .on_click(cx.listener(move |this, _, _, cx| {
            let id = FileId(file_id);
            if active && paused {
                this.resume_media_download(id, cx);
            } else if active {
                this.pause_media_download(id, cx);
            } else if failed {
                this.request_media_download(id, None, cx);
            } else {
                this.open_downloaded_file(id, cx);
            }
        }));
        let secondary = if active {
            Some(
                Button::new(("download-cancel", file_id as u64))
                    .icon(IconName::X)
                    .ghost()
                    .small()
                    .tooltip("Cancel download")
                    .accessibility_label("Cancel download")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.cancel_media_download(FileId(file_id), cx);
                    })),
            )
        } else if !failed {
            Some(
                Button::new(("download-reveal", file_id as u64))
                    .icon(IconName::FolderOpen)
                    .ghost()
                    .small()
                    .tooltip("Show in folder")
                    .accessibility_label("Show downloaded file in folder")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.reveal_downloaded_file(FileId(file_id), cx);
                    })),
            )
        } else {
            None
        };
        div()
            .id(("download-row", file_id as u64))
            .mx_1()
            .px_2()
            .py_1p5()
            .rounded_md()
            .hover(|style| style.bg(cx.theme().accent.opacity(0.5)))
            .flex()
            .items_center()
            .gap_3()
            .child(disc)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(div().text_sm().font_medium().truncate().child(name))
                    .child(
                        div()
                            .text_xs()
                            .truncate()
                            .text_color(if failed {
                                cx.theme().danger
                            } else {
                                cx.theme().muted_foreground
                            })
                            .child(meta),
                    ),
            )
            .children(secondary)
            .into_any_element()
    }
}
