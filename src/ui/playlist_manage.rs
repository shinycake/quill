//! Managing saved music on the Playlist page (tdesktop
//! `info/saved/info_saved_music_widget.cpp`, `data/data_saved_music.cpp`):
//! drag a track of your own playlist to reorder it, and a right-click menu
//! with Show in Folder, Save, Delete. Deleting asks first
//! (`lng_selected_remove_saved_music`) and toasts `lng_saved_music_removed`.
//!
//! tdesktop's menu for a saved track is `AddSaveDocumentAction` plus Delete
//! on your own playlist. In your own playlist every track is saved and the
//! page belongs to Saved Messages' owner, so only "Save As..." is left. On
//! someone else's playlist the track can go to your profile, to Saved
//! Messages or to a file.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::context_menu_keys::context_menu_item;
use super::folder_share::finish_dialog;
use super::shell::{DialogKind, QuillShell};
use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::FileId;
use quill::lang::{keys, tr, tr_with};
use quill::state::track_name;
use quill::telegram::envelope::ProfileAudio;

/// The open track menu: its track, where it opened, and whether the
/// "Save to..." page is showing.
#[derive(Clone, Copy)]
pub(crate) struct PlaylistMenu {
    pub file_id: FileId,
    pub position: Point<Pixels>,
    pub save_to: bool,
}

/// `lng_selected_remove_saved_music`.
pub(crate) fn remove_text() -> &'static str {
    tr(keys::SELECTED_REMOVE_SAVED_MUSIC)
}
/// `lng_saved_music_removed`.
pub(crate) fn removed_toast() -> &'static str {
    tr(keys::SAVED_MUSIC_REMOVED)
}
/// `lng_saved_music_added`.
pub(crate) fn added_toast() -> &'static str {
    tr(keys::SAVED_MUSIC_ADDED)
}
/// `lng_context_save_music_about`.
pub(crate) fn save_about() -> &'static str {
    tr(keys::CONTEXT_SAVE_MUSIC_ABOUT)
}

#[derive(Clone)]
pub(crate) struct PlaylistDrag {
    pub index: usize,
    pub title: String,
}

impl Render for PlaylistDrag {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_2()
            .rounded_md()
            .bg(cx.theme().accent.opacity(0.15))
            .text_sm()
            .child(self.title.clone())
    }
}

impl QuillApp {
    pub(super) fn open_playlist_menu(
        &mut self,
        file_id: FileId,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.playback.playlist_menu = Some(PlaylistMenu {
            file_id,
            position,
            save_to: false,
        });
        // "Profile" is offered only for a track that is not saved yet.
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.check_profile_audio(file_id);
        }
        cx.notify();
    }

    fn playlist_track(&self, file_id: FileId) -> Option<ProfileAudio> {
        self.session()?
            .media
            .playlists
            .values()
            .flat_map(|playlist| &playlist.tracks)
            .find(|track| track.file_id == file_id)
            .cloned()
    }

    /// Drop a dragged track on another row of your own playlist.
    pub(super) fn reorder_playlist_track(
        &mut self,
        from: usize,
        to: usize,
        cx: &mut Context<Self>,
    ) {
        match self.live.as_mut() {
            Some(live) => {
                let _ = live.driver.move_profile_audio_track(from, to);
            }
            None => {
                if let Some(session) = self.demo_session.as_mut() {
                    session.local_move_profile_audio(from, to);
                }
            }
        }
        cx.notify();
    }

    /// "Delete" in the track menu: ask first.
    fn ask_remove_playlist_track(&mut self, file_id: FileId, cx: &mut Context<Self>) {
        self.playback.playlist_remove = Some(file_id);
        cx.notify();
    }

    /// Confirmed: the toast shows at once, as in tdesktop's box.
    fn confirm_remove_playlist_track(&mut self, cx: &mut Context<Self>) {
        let Some(file_id) = self.playback.playlist_remove.take() else {
            return;
        };
        self.connection
            .set_note(StatusNote::silent(removed_toast()));
        match self.live.as_mut() {
            Some(live) => {
                let _ = live.driver.remove_profile_audio_track(file_id);
            }
            None => {
                if let Some(session) = self.demo_session.as_mut() {
                    session.local_remove_profile_audio(file_id);
                }
            }
        }
        cx.notify();
    }

    /// "Save As..." / "... Downloads": copy the downloaded file to a place
    /// the user picks. A track that is not on disk starts downloading.
    fn save_playlist_track_as(&mut self, file_id: FileId, cx: &mut Context<Self>) {
        let Some(track) = self.playlist_track(file_id) else {
            return;
        };
        let path = self
            .session()
            .and_then(|s| s.media.files.get(&file_id.0))
            .and_then(|f| f.usable_path())
            .map(std::path::PathBuf::from);
        let Some(path) = path else {
            if let Some(live) = self.live.as_mut() {
                let _ = live.driver.ensure_media_files(&[file_id]);
            }
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_AV_DOWNLOADING_CHOOSE_SAVE_AS_AGAIN,
            )));
            cx.notify();
            return;
        };
        let local_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();
        let suggested = quill::export::names::safe_file_name(if track.file_name.is_empty() {
            &local_name
        } else {
            &track.file_name
        });
        let dir =
            quill::media_viewer::downloads_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        let picker = cx.prompt_for_new_path(&dir, Some(&suggested));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(dest))) = picker.await else {
                return;
            };
            let copied = cx
                .background_executor()
                .spawn(async move { std::fs::copy(&path, &dest).map(|_| dest) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.connection.set_note(match copied {
                    Ok(dest) => StatusNote::info(format!(
                        "saved to {}",
                        dest.file_name().and_then(|n| n.to_str()).unwrap_or("file")
                    )),
                    Err(err) => StatusNote::error(tr_with(
                        keys::Q_AV_COULDNT_SAVE,
                        &[("err", &err.to_string())],
                    )),
                });
                cx.notify();
            });
        })
        .detach();
    }

    fn save_playlist_track_to_profile(&mut self, file_id: FileId, cx: &mut Context<Self>) {
        let Some(track) = self.playlist_track(file_id) else {
            return;
        };
        match self.live.as_mut() {
            Some(live) => {
                self.connection.set_note(
                    match live.driver.save_audio_to_profile(
                        file_id,
                        track.duration,
                        &track.title,
                        &track.performer,
                        &track.file_name,
                    ) {
                        Ok(_) => StatusNote::info(added_toast()),
                        Err(_) => StatusNote::error(tr(keys::Q_AV_COULD_NOT_SAVE_TO_YOUR)),
                    },
                );
            }
            None => {
                if let Some(session) = self.demo_session.as_mut() {
                    session.local_save_profile_audio(track);
                }
                self.connection.set_note(StatusNote::silent(added_toast()));
            }
        }
        cx.notify();
    }

    fn save_playlist_track_to_saved_messages(&mut self, file_id: FileId, cx: &mut Context<Self>) {
        self.connection.set_note(match self.live.as_mut() {
            Some(live) => match live.driver.send_track_to_saved_messages(file_id) {
                Ok(_) => StatusNote::info(tr(keys::Q_AV_SAVED_TO_SAVED_MESSAGES)),
                Err(_) => StatusNote::error(tr(keys::Q_AV_COULD_NOT_SAVE_TO_SAVED)),
            },
            None => StatusNote::info(tr(keys::Q_AV_DEMO_SAVED_MESSAGES_NEEDS_LIVE)),
        });
        cx.notify();
    }

    pub(super) fn playlist_menu_overlay(
        &self,
        menu: PlaylistMenu,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let PlaylistMenu {
            file_id,
            position,
            save_to,
        } = menu;
        let session = self.session();
        let mine = session
            .and_then(|s| s.my_user_id)
            .zip(session)
            .is_some_and(|(me, s)| {
                s.media
                    .playlists
                    .get(&me)
                    .is_some_and(|p| p.tracks.iter().any(|t| t.file_id == file_id))
            });
        let local = session
            .and_then(|s| s.media.files.get(&file_id.0))
            .and_then(|f| f.usable_path())
            .is_some();
        let in_profile = session.and_then(|s| s.profile_audio_known(file_id)) == Some(true);
        let finder = cfg!(target_os = "macos");
        let row = |id: &'static str, icon: IconName, label: &'static str, danger: bool| {
            let color = if danger { danger_bright() } else { text_menu() };
            context_menu_item(
                id,
                icon,
                label,
                color.into(),
                true,
                &self.frame.context_menu_items,
                cx,
            )
        };
        let close = |this: &mut QuillApp| this.playback.playlist_menu = None;
        let mut rows: Vec<AnyElement> = Vec::new();
        if save_to {
            rows.push(
                row(
                    "playlist-menu-back",
                    IconName::ChevronLeft,
                    tr(keys::Q_AV_SAVE_TO),
                    false,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(menu) = this.playback.playlist_menu.as_mut() {
                        menu.save_to = false;
                    }
                    cx.notify();
                }))
                .into_any_element(),
            );
            if !in_profile {
                rows.push(
                    row(
                        "playlist-save-profile",
                        IconName::User,
                        tr(keys::Q_AV_PROFILE),
                        false,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        close(this);
                        this.save_playlist_track_to_profile(file_id, cx);
                    }))
                    .into_any_element(),
                );
            }
            rows.push(
                row(
                    "playlist-save-saved",
                    IconName::Bookmark,
                    tr(keys::Q_AV_SAVED_MESSAGES),
                    false,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    close(this);
                    this.save_playlist_track_to_saved_messages(file_id, cx);
                }))
                .into_any_element(),
            );
            rows.push(
                row(
                    "playlist-save-downloads",
                    IconName::Download,
                    tr(keys::Q_AV_DOWNLOADS),
                    false,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    close(this);
                    this.save_playlist_track_as(file_id, cx);
                }))
                .into_any_element(),
            );
            rows.push(
                div()
                    .h(px(1.))
                    .mx_3()
                    .my_1()
                    .bg(border())
                    .into_any_element(),
            );
            rows.push(
                div()
                    .px_3()
                    .py_1p5()
                    .max_w(px(220.))
                    .text_xs()
                    .text_color(text_muted())
                    .child(save_about())
                    .into_any_element(),
            );
        } else {
            if local {
                rows.push(
                    row(
                        "playlist-menu-folder",
                        IconName::FolderOpen,
                        if finder {
                            tr(keys::Q_AV_SHOW_IN_FINDER)
                        } else {
                            tr(keys::Q_AV_SHOW_IN_FOLDER)
                        },
                        false,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        close(this);
                        this.reveal_downloaded_file(file_id, cx);
                    }))
                    .into_any_element(),
                );
            }
            if mine {
                // Saved and in your own list: only the file is left to save.
                rows.push(
                    row(
                        "playlist-menu-save-as",
                        IconName::Download,
                        tr(keys::Q_AV_SAVE_AS),
                        false,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        close(this);
                        this.save_playlist_track_as(file_id, cx);
                    }))
                    .into_any_element(),
                );
                rows.push(
                    row(
                        "playlist-menu-delete",
                        IconName::Trash,
                        tr(keys::BOX_DELETE),
                        true,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        close(this);
                        this.ask_remove_playlist_track(file_id, cx);
                    }))
                    .into_any_element(),
                );
            } else {
                rows.push(
                    row(
                        "playlist-menu-save-to",
                        IconName::Download,
                        tr(keys::Q_AV_SAVE_TO),
                        false,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(menu) = this.playback.playlist_menu.as_mut() {
                            menu.save_to = true;
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
                );
            }
        }
        let panel = div()
            .id("playlist-menu-panel")
            .occlude()
            .flex()
            .flex_col()
            .min_w(px(200.))
            .px_1()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(accent())
            .bg(bg_canvas())
            .children(rows);
        div()
            .id("playlist-menu-overlay")
            .track_focus(&self.frame.context_menu_focus)
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .child(
                div()
                    .id("playlist-menu-backdrop")
                    .occlude()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.playback.playlist_menu = None;
                        cx.notify();
                    })),
            )
            .child(
                anchored()
                    .position(position)
                    .snap_to_window_with_margin(px(8.))
                    .child(panel),
            )
            .focus_trap("playlist-menu-focus", &self.frame.context_menu_focus)
            .into_any_element()
    }

    fn build_remove_playlist_track_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close = QuillShell::on_close_kind(
            app,
            shell,
            DialogKind::RemovePlaylistTrack,
            |this, _, cx| {
                this.playback.playlist_remove = None;
                cx.notify();
            },
        );
        app.update(cx, |this, cx| {
            let name = this
                .playback
                .playlist_remove
                .and_then(|id| this.playlist_track(id))
                .map(|track| track_name(&track))
                .unwrap_or_default();
            let body = div()
                .id("remove-playlist-track")
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_sm().child(remove_text()))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .truncate()
                        .child(name),
                );
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("remove-playlist-cancel")
                        .label(tr(keys::CANCEL))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.playback.playlist_remove = None;
                            cx.notify();
                            this.close_kit_dialog_if_done(
                                DialogKind::RemovePlaylistTrack,
                                window,
                                cx,
                            );
                        })),
                )
                .child(
                    Button::new("remove-playlist-confirm")
                        .label(tr(keys::BOX_DELETE))
                        .danger()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.confirm_remove_playlist_track(cx);
                            this.close_kit_dialog_if_done(
                                DialogKind::RemovePlaylistTrack,
                                window,
                                cx,
                            );
                        })),
                );
            finish_dialog(
                dialog,
                tr(keys::BOX_DELETE).to_string(),
                body.into_any_element(),
                Some(footer.into_any_element()),
                on_close,
            )
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// Confirm removing a track from your own Playlist page.
    RemovePlaylistTrack => DialogSpec::new(
        5461,
        |app| app.playback.playlist_remove.is_some(),
        QuillApp::build_remove_playlist_track_dialog,
    ),
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "playlist_manage_tests.rs"]
mod tests;
