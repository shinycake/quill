//! media download queue panel.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::message_media::{download_display_name, format_bytes};
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::{ChatId, FileId};
use quill::lang::{keys, tr, tr_with};
use quill::platform::OsOpen;
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
            let note = match result {
                Ok(Some(_)) => StatusNote::silent(tr(keys::Q_OPS_DOWNLOADING)),
                Ok(None) => StatusNote::silent(tr(keys::Q_OPS_ALREADY_LOCAL_OR_IN_PROGRESS)),
                Err(_) => {
                    self.drop_play_intent_for(file_id);
                    StatusNote::error(tr(keys::Q_OPS_COULD_NOT_DOWNLOAD_PLEASE_TRY))
                }
            };
            self.connection.set_note(note);
        } else if let Some(session) = self.demo_session.as_mut() {
            // The transfer starts as the driver records one; no bytes arrive.
            if session.should_download(file_id) {
                session.begin_user_download(file_id);
            }
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_OPS_DEMO_ADDFILETODOWNLOADS_RUNS_WITH_LIVE,
            )));
        }
        cx.notify();
    }

    pub(super) fn discard_stopped_media_playback(&mut self, cx: &mut Context<Self>) {
        let Some(file_id) = self.playback.intent.as_ref().map(|intent| intent.file) else {
            return;
        };
        let stopped = self.session().is_some_and(|s| {
            s.media.downloads.is_failed(file_id.0)
                || (s.file(file_id).and_then(|f| f.usable_path()).is_none()
                    && !s.media.downloads.is_downloading(file_id.0, &s.media.files)
                    && !s.requests.has_download(file_id))
        });
        if stopped {
            self.drop_play_intent_for(file_id);
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_OPS_DOWNLOAD_STOPPED_PRESS_PLAY_TO,
            )));
            cx.notify();
        }
    }

    /// MED3: open a fully-downloaded file with the system viewer
    /// (`platform::open_local_file` — `xdg-open` / `open`).
    pub(super) fn open_downloaded_file(&mut self, file_id: FileId, cx: &mut Context<Self>) {
        let path: Option<PathBuf> = self
            .session()
            .and_then(|s| s.media.files.get(&file_id.0))
            .and_then(|f| f.usable_path())
            .map(PathBuf::from);
        match path {
            // B13: tdesktop asks before running executables, unknown types
            // and files that may reveal the IP address.
            Some(path) => self.open_file_guarded(path, cx),
            None => {
                self.connection
                    .set_note(StatusNote::error(tr(keys::Q_OPS_COULD_NOT_OPEN_THE_FILE)));
                cx.notify();
            }
        }
    }

    /// MED3: reveal a downloaded file in the file manager
    /// (`platform::reveal_in_file_manager`).
    pub(super) fn reveal_downloaded_file(&mut self, file_id: FileId, cx: &mut Context<Self>) {
        let path: Option<PathBuf> = self
            .session()
            .and_then(|s| s.media.files.get(&file_id.0))
            .and_then(|f| f.usable_path())
            .map(PathBuf::from);
        let open = path.map_or(OsOpen::Answered(false), |path| {
            quill::platform::reveal_in_file_manager(&path)
        });
        self.when_os_answers(open, cx, |this, opened, cx| {
            this.connection.set_note(if opened {
                StatusNote::silent(tr(keys::Q_OPS_REVEALED_IN_FILE_MANAGER))
            } else {
                StatusNote::error(tr(keys::Q_OPS_COULD_NOT_REVEAL_THE_FILE))
            });
            cx.notify();
        });
    }

    /// Cancel an in-flight download: listed (user-initiated) downloads go
    /// through `removeFileFromDownloads`; one-shot automatic downloads use
    /// `cancelDownloadFile` (TGX's cancel button on downloading media).
    pub(super) fn cancel_media_download(&mut self, file_id: FileId, cx: &mut Context<Self>) {
        self.drop_play_intent_for(file_id);
        if let Some(live) = self.live.as_mut() {
            let note = match live.driver.cancel_download(file_id) {
                Ok(true) => StatusNote::silent(tr(keys::Q_OPS_DOWNLOAD_CANCELED)),
                Ok(false) => StatusNote::silent(tr(keys::Q_OPS_NOTHING_TO_CANCEL)),
                Err(_) => StatusNote::error(tr(keys::Q_OPS_COULD_NOT_CANCEL_THE_DOWNLOAD)),
            };
            self.connection.set_note(note);
        } else if let Some(session) = self.demo_session.as_mut() {
            if session.media.downloads.is_requested(file_id.0) {
                session.abort_download(file_id);
            }
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_OPS_DEMO_CANCELDOWNLOADFILE_RUNS_WITH_LIVE,
            )));
        }
        cx.notify();
    }

    /// Slice media-downloads-pause: pause a user-initiated download
    /// (`toggleDownloadIsPaused`; the pause state arrives on
    /// `updateFileDownload`).
    pub(super) fn pause_media_download(&mut self, file_id: FileId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection
                .set_note(match live.driver.pause_download(file_id) {
                    Ok(true) => StatusNote::silent(tr(keys::Q_OPS_DOWNLOAD_PAUSED)),
                    Ok(false) => StatusNote::silent(tr(keys::Q_OPS_NOTHING_TO_PAUSE)),
                    Err(_) => StatusNote::error(tr(keys::Q_OPS_COULD_NOT_PAUSE_THE_DOWNLOAD)),
                });
        } else if self.demo_session.is_some() {
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_OPS_DEMO_TOGGLEDOWNLOADISPAUSED_RUNS_WITH_LIVE,
            )));
        }
        cx.notify();
    }

    /// Slice media-downloads-pause: resume a paused user-initiated download.
    pub(super) fn resume_media_download(&mut self, file_id: FileId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection
                .set_note(match live.driver.resume_download(file_id) {
                    Ok(true) => StatusNote::silent(tr(keys::Q_OPS_DOWNLOAD_RESUMED)),
                    Ok(false) => StatusNote::silent(tr(keys::Q_OPS_NOTHING_TO_RESUME)),
                    Err(_) => StatusNote::error(tr(keys::Q_OPS_COULD_NOT_RESUME_THE_DOWNLOAD)),
                });
        } else if self.demo_session.is_some() {
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_OPS_DEMO_TOGGLEDOWNLOADISPAUSED_RUNS_WITH_LIVE,
            )));
        }
        cx.notify();
    }

    /// MED3: downloads manager panel (TGX side-menu Downloads, empty state
    /// `NoDownloadFilesFound`). Lists user-initiated active downloads with
    /// live progress + cancel, and recently completed downloads with
    /// open / reveal actions. Sits beside the conversation like the info
    /// panel; toggled from the sidebar "Downloads" entry.
    pub(super) fn downloads_panel_open(&self) -> bool {
        self.session().is_some_and(|s| s.media.downloads_panel_open)
    }

    pub(super) fn downloads_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.downloads_panel_open() {
            return None;
        }
        self.downloads_panel_body(cx)
    }

    /// `downloads_panel` past its cheap check. A debug build reserves this
    /// builder's whole frame on entry, so frames that show nothing skip
    /// it (docs/decisions/main-thread-stack.md).
    /// The outer function repeats this body's first check; keep the two in step.
    #[inline(never)]
    fn downloads_panel_body(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.downloads_panel_open() {
            return None;
        }
        let session = self.session()?;
        let (active, recent) = (
            session.media.downloads.user_count(),
            session.media.downloads.recent(&session.media.files).count(),
        );
        // Its width is the shell's (`shell_layout`).
        let mut panel = div()
            .id("downloads-panel")
            .flex_1()
            .min_w_0()
            .h_full()
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
                    .child(div().font_semibold().child(tr(keys::ALL_DOWNLOADS)))
                    .child(
                        Button::new("downloads-panel-close")
                            .accessibility_label(tr(keys::Q_OPS_CLOSE_DOWNLOADS))
                            .icon(IconName::X)
                            .ghost()
                            .tooltip(tr(keys::Q_OPS_CLOSE_DOWNLOADS))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(live) = this.live.as_mut() {
                                    live.driver.session.media.downloads_panel_open = false;
                                } else if let Some(session) = this.demo_session.as_mut() {
                                    session.media.downloads_panel_open = false;
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
        if active == 0 && recent == 0 {
            panel = panel.child(
                div()
                    .px_3()
                    .py_4()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::Q_OPS_NO_DOWNLOADS_YET)),
            );
        } else {
            panel = panel.children(self.download_sections("", cx));
        }
        Some(panel.into_any_element())
    }

    /// Whether the download list has anything to show: tdesktop's search
    /// strip lists its Downloads tab only then.
    pub(super) fn has_downloads(&self) -> bool {
        self.session().is_some_and(|s| {
            s.media.downloads.user_count() > 0
                || s.media.downloads.recent(&s.media.files).next().is_some()
                || s.media
                    .downloads
                    .failed_ids(&s.media.files)
                    .next()
                    .is_some()
        })
    }

    /// The Active / Failed / Recent sections of the list, rows whose file
    /// name contains `query` (empty = all). Shared by the downloads panel
    /// and the Downloads tab of the global search.
    pub(super) fn download_sections(&self, query: &str, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(session) = self.session() else {
            return Vec::new();
        };
        let needle = query.trim().to_lowercase();
        let keep = |file_id: &i32| {
            needle.is_empty()
                || download_display_name(session, *file_id)
                    .to_lowercase()
                    .contains(&needle)
        };
        let mut active: Vec<i32> = session.media.downloads.user_ids().collect();
        active.sort_unstable();
        active.retain(keep);
        let mut failed: Vec<i32> = session
            .media
            .downloads
            .failed_ids(&session.media.files)
            .collect();
        failed.sort_unstable();
        failed.retain(keep);
        let mut recent: Vec<i32> = session
            .media
            .downloads
            .recent(&session.media.files)
            .collect();
        recent.retain(keep);
        let muted = cx.theme().muted_foreground;
        let heading = |text: &'static str| {
            div()
                .px_3()
                .pt_2()
                .text_xs()
                .font_semibold()
                .text_color(muted)
                .child(text)
                .into_any_element()
        };
        let mut out = Vec::new();
        if !active.is_empty() {
            out.push(heading(tr(keys::Q_OPS_ACTIVE)));
            for file_id in active {
                out.push(self.download_row(file_id, true, false, cx));
            }
        }
        if !failed.is_empty() {
            out.push(heading(tr(keys::Q_OPS_FAILED)));
            for file_id in failed {
                out.push(self.download_row(file_id, false, true, cx));
            }
        }
        if !recent.is_empty() {
            out.push(heading(tr(keys::Q_OPS_RECENT)));
            for file_id in recent.into_iter().rev() {
                out.push(self.download_row(file_id, false, false, cx));
            }
        }
        out
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
        let file = session.as_ref().and_then(|s| s.media.files.get(&file_id));
        let name = session
            .as_ref()
            .map(|s| download_display_name(s, file_id))
            .unwrap_or_else(|| tr_with(keys::Q_OPS_FILE, &[("file_id", &file_id.to_string())]));
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
                .is_some_and(|s| s.media.downloads.is_paused(file_id));
        let done = file.map_or(0, |f| f.local.downloaded_size);
        let meta = if active {
            let amount = if done > 0 && !size_label.is_empty() {
                format!("{} of {}", format_bytes(done), size_label)
            } else {
                size_label.clone()
            };
            let lead = if paused {
                tr(keys::Q_OPS_PAUSED).to_string()
            } else {
                progress.map_or(tr(keys::Q_OPS_DOWNLOADING_2).to_string(), |p| {
                    format!("{}%", (p * 100.0).round() as i32)
                })
            };
            if amount.is_empty() {
                lead
            } else {
                format!("{lead} · {amount}")
            }
        } else if failed {
            tr(keys::Q_OPS_DOWNLOAD_FAILED).to_string()
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
                    tr(keys::Q_OPS_RESUME_DOWNLOAD)
                } else {
                    tr(keys::Q_OPS_PAUSE_DOWNLOAD)
                },
                // Paused: a frozen ring; unknown progress: a spinning one.
                Some(if paused {
                    Some(progress.unwrap_or(0.))
                } else {
                    progress
                }),
            )
        } else if failed {
            (IconName::RotateCcw, tr(keys::Q_OPS_RETRY_DOWNLOAD), None)
        } else {
            (IconName::File, tr(keys::Q_OPS_OPEN_FILE), None)
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
                    .tooltip(tr(keys::Q_OPS_CANCEL_DOWNLOAD))
                    .accessibility_label(tr(keys::Q_OPS_CANCEL_DOWNLOAD))
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
                    .tooltip(tr(keys::Q_OPS_SHOW_IN_FOLDER))
                    .accessibility_label(tr(keys::Q_OPS_SHOW_DOWNLOADED_FILE_IN_FOLDER))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.reveal_downloaded_file(FileId(file_id), cx);
                    })),
            )
        } else {
            None
        };
        // The row names a file from a chat: tagged with the chat noted when
        // the download started, so the UI driver prunes it with the chat's
        // grant, and left to the list's default-deny otherwise.
        let chat_tag = session
            .as_ref()
            .and_then(|s| s.media.downloads.chat_of(file_id))
            .map(|chat| quill::agent::ui_driver::redact::chat_tag(chat.0));
        div()
            .id(("download-row", file_id as u64))
            .role(gpui_kit::Role::Group)
            .when_some(chat_tag, |this, tag| this.a11y_tag(tag))
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
