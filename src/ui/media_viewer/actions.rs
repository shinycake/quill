//! MED1: viewer actions: rotate and flip, share, save, copy, delete, the
//! sender line, album pin, attached stickers and Show in Chat.

use super::*;
use crate::ui::image_decode::DecodeSize;
use crate::ui::status_note::StatusNote;
use quill::lang::{keys, tr, tr_with};

impl QuillApp {
    /// MED1: rotate the viewer photo 90° clockwise (photos only).
    pub(in crate::ui) fn rotate_viewer_photo(&mut self, cx: &mut Context<Self>) {
        self.reorient_viewer_photo(|o| o.rotate_cw(), cx);
    }

    /// `H`: mirror the viewer photo left-to-right (tdesktop `_flip`).
    pub(in crate::ui) fn flip_viewer_horizontal(&mut self, cx: &mut Context<Self>) {
        self.reorient_viewer_photo(|o| o.flip_horizontal(), cx);
    }

    /// `V`: mirror the viewer photo top-to-bottom.
    pub(in crate::ui) fn flip_viewer_vertical(&mut self, cx: &mut Context<Self>) {
        self.reorient_viewer_photo(|o| o.flip_vertical(), cx);
    }

    /// Apply an orientation change: the re-oriented pixels decode on a
    /// background thread and the new orientation takes effect together
    /// with them (`finish_viewer_reorient`), so the overlay render stays
    /// allocation-free and the picture never shows a half-applied turn. A
    /// photo that is not local or cannot be decoded keeps its previous
    /// orientation.
    fn reorient_viewer_photo(
        &mut self,
        change: impl FnOnce(&mut ViewerOrientation),
        cx: &mut Context<Self>,
    ) {
        let item = self.viewer.state.current().cloned();
        let Some(item) = item else { return };
        if !item.kind.is_picture() {
            return;
        }
        // Presses queued while a decode runs build on the pending target.
        let mut target = self
            .viewer
            .reorient_pending
            .unwrap_or(self.viewer.orientation);
        change(&mut target);
        self.viewer.reorient_gen = self.viewer.reorient_gen.wrapping_add(1);
        let generation = self.viewer.reorient_gen;
        if target.is_identity() {
            self.viewer.reorient_pending = None;
            self.viewer.orientation = target;
            self.viewer.rotated = None;
            cx.notify();
            return;
        }
        let no_files = quill::state::FileMap::default();
        let files = self.session().map_or(&no_files, |s| &s.media.files);
        let roots = self.media_display_roots();
        let path = viewer_display_path(&item, files, &roots);
        let Some(path) = path else {
            self.viewer.reorient_pending = None;
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_AV_DOWNLOAD_THE_PHOTO_FIRST_TO,
            )));
            cx.notify();
            return;
        };
        self.viewer.reorient_pending = Some(target);
        let decode_path = path.clone();
        let job =
            cx.background_spawn(async move { Self::rotated_render_image(&decode_path, target) });
        self.spawn_scoped(cx, async move |this, cx| {
            let image = job.await;
            let _ = this.update(cx, |this, cx| {
                this.finish_viewer_reorient(generation, path, target, image, cx);
            });
        })
        .detach();
    }

    /// The decode started by `reorient_viewer_photo` finished: apply it
    /// unless the viewer moved on (another press, a step, a close).
    fn finish_viewer_reorient(
        &mut self,
        generation: u64,
        path: PathBuf,
        target: ViewerOrientation,
        image: Option<Arc<RenderImage>>,
        cx: &mut Context<Self>,
    ) {
        if self.viewer.reorient_gen != generation || !self.viewer.state.is_open() {
            return;
        }
        self.viewer.reorient_pending = None;
        match image {
            Some(image) => {
                self.viewer.orientation = target;
                self.viewer.rotated = Some((path, target.code(), image.into()));
            }
            None => self
                .connection
                .set_note(StatusNote::error(tr(keys::Q_AV_COULDNT_ROTATE_THIS_PHOTO))),
        }
        cx.notify();
    }

    /// MED1: decode `path` and orient it (flip, then rotate) into a
    /// `RenderImage` (the `decode_viewer_frames` construction pattern).
    pub(in crate::ui) fn rotated_render_image(
        path: &std::path::Path,
        orientation: ViewerOrientation,
    ) -> Option<Arc<RenderImage>> {
        let rgba = crate::ui::image_decode::decode_file(path, DecodeSize::FULL).ok()?;
        let (mut pixels, width, height) =
            orient_rgba(rgba.as_raw(), rgba.width(), rgba.height(), orientation);
        // GPUI's sprites are BGRA.
        for pixel in pixels.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        let rotated = image::RgbaImage::from_raw(width, height, pixels)?;
        Some(Arc::new(RenderImage::new(SmallVec::from_buf([
            image::Frame::new(rotated),
        ]))))
    }

    /// MED1: share the viewer media — opens the forward picker with the
    /// current message selected (the message-menu forward flow, reused).
    pub(in crate::ui) fn share_viewer_media(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(item) = self.viewer.state.current() else {
            return;
        };
        let (chat_id, message_id) = (item.chat_id, item.message_id);
        if self.viewer_is_timed() || self.refuse_protected_copy(chat_id, cx) {
            return;
        }
        // Close the overlay first so the forward picker is visible.
        self.close_media_viewer(cx);
        self.begin_forward_one(chat_id, message_id, false, window, cx);
    }

    /// MED1: save the viewer media to the downloads folder. Photos save
    /// the largest local size, a video photo its clip (tdesktop
    /// `OverlayWidget::saveAs`); videos save the full clip when local,
    /// else the thumbnail.
    pub(in crate::ui) fn save_viewer_media(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.viewer.state.current().cloned() else {
            return;
        };
        if self.viewer_is_timed() || self.refuse_protected_copy(item.chat_id, cx) {
            return;
        }
        let no_files = quill::state::FileMap::default();
        let files = self.session().map_or(&no_files, |s| &s.media.files);
        let path = match item.kind {
            MediaViewerKind::Photo if item.is_video_photo() => item
                .play_file_id
                .and_then(|id| files.get(&id.0))
                .and_then(|file| file.usable_path())
                .map(PathBuf::from),
            MediaViewerKind::Photo => {
                let roots = self.media_display_roots();
                viewer_display_path(&item, files, &roots)
            }
            // Video: save the full clip only — falling back to the
            // thumbnail would write a JPEG as the "video". If the clip
            // isn't local the user gets the honest download-first note.
            MediaViewerKind::Video | MediaViewerKind::Animation => item
                .play_file_id
                .and_then(|id| files.get(&id.0))
                .and_then(|file| file.usable_path())
                .map(PathBuf::from),
            // A file saves its own bytes, never the thumbnail (an HDR or
            // HEIC original stays as it was sent).
            MediaViewerKind::ImageFile | MediaViewerKind::File => files
                .get(&item.download_file_id.0)
                .and_then(|file| file.usable_path())
                .map(PathBuf::from),
        };
        // B13: "Ask where to save each file" opens a save dialog instead of
        // dropping the copy into the download folder.
        if let Some(path) = path.as_ref()
            && quill::file_prefs::current().ask_download_path
        {
            self.save_file_asking(path.clone(), cx);
            return;
        }
        match path {
            Some(path) => match save_media_to_downloads(&path) {
                Ok(dest) => {
                    self.show_saved_toast(dest, item.plays_clip(), cx);
                }
                Err(err) => {
                    self.connection.set_note(StatusNote::error(tr_with(
                        keys::Q_AV_COULDNT_SAVE,
                        &[("err", &err.to_string())],
                    )));
                }
            },
            None => {
                self.connection.set_note(StatusNote::silent(tr(
                    keys::Q_AV_DOWNLOAD_THE_MEDIA_FIRST_TO,
                )));
            }
        }
        cx.notify();
    }

    /// The viewer's own "saved to your Downloads folder" toast, with a
    /// link that reveals the file (tdesktop `showSaveMsgToast`). It hides
    /// itself after a few seconds.
    pub(in crate::ui) fn show_saved_toast(
        &mut self,
        dest: PathBuf,
        video: bool,
        cx: &mut Context<Self>,
    ) {
        let text = quill::viewer_extras::saved_toast_text(
            &dest,
            quill::media_viewer::downloads_dir().as_deref(),
            video,
        );
        self.viewer.extra.saved_toast = Some(quill::viewer_extras::SavedToast { dest, text });
        self.viewer.extra.saved_toast_gen += 1;
        let generation = self.viewer.extra.saved_toast_gen;
        self.spawn_scoped(cx, async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(quill::viewer_extras::SAVED_TOAST_MS))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.viewer.extra.saved_toast_gen == generation {
                    this.viewer.extra.saved_toast = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// The toast's folder link: show the saved file in the file manager.
    pub(super) fn reveal_saved_toast_file(&mut self, cx: &mut Context<Self>) {
        let Some(toast) = self.viewer.extra.saved_toast.take() else {
            return;
        };
        if !quill::platform::reveal_in_file_manager(&toast.dest) {
            self.connection
                .set_note(StatusNote::error(tr(keys::Q_AV_COULDNT_OPEN_THE_FOLDER)));
        }
        cx.notify();
    }

    /// The delete confirmation for the current viewer item, when the
    /// message may be deleted (same gate as the message menu).
    pub(in crate::ui) fn viewer_delete_confirm(&self) -> Option<quill::composer::DeleteConfirm> {
        let item = self.viewer.state.current()?;
        let session = self.session()?;
        let chat_id = item.chat_id;
        let message = session
            .histories
            .get(&chat_id.0)?
            .messages
            .get(&item.message_id.0)?;
        let mut confirm = quill::composer::DeleteConfirm::for_message(
            chat_id,
            item.message_id,
            message.is_outgoing,
            message.pending,
        )?;
        let actions = session
            .messages
            .message_menu_actions
            .filter(|(c, m, _)| *c == chat_id && *m == item.message_id)
            .map(|(_, _, actions)| actions);
        let is_channel_post = matches!(
            session.chats.get(&chat_id.0).map(|chat| &chat.kind),
            Some(quill::telegram::envelope::ChatKind::Supergroup {
                is_channel: true,
                ..
            })
        );
        let can_revoke = viewer_delete_gate(
            actions,
            message.is_outgoing,
            is_channel_post,
            session.is_saved_messages(chat_id),
        )?;
        confirm.can_revoke = can_revoke;
        confirm.revoke = can_revoke;
        Some(confirm)
    }

    /// Trash: open the message menu's delete confirmation ("Also delete
    /// for {name}" checkbox) for the current item. Once the message is
    /// gone from the history `prune_deleted_viewer_items` moves the
    /// viewer to the next item, or closes it.
    pub(in crate::ui) fn delete_viewer_media(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.viewer.state.source() == ViewerSource::Profile {
            self.delete_viewer_profile_photo(cx);
        } else if let Some(confirm) = self.viewer_delete_confirm() {
            self.open_delete_dialog(confirm, window, cx);
        }
    }

    /// Drop viewer items whose message was deleted, moving to the next
    /// item (or closing when none remain). A message that only left the
    /// loaded part of the history (paging, a jump) is not deleted: only a
    /// tombstone counts.
    pub(super) fn prune_deleted_viewer_items(&mut self, cx: &mut Context<Self>) {
        // Shared Media items reach past the loaded history; absence there
        // does not mean deleted.
        if matches!(
            self.viewer.state.source(),
            ViewerSource::SharedMedia | ViewerSource::Profile | ViewerSource::Poll
        ) {
            return;
        }
        let mut viewer = std::mem::take(&mut self.viewer.state);
        let changed = match self.session() {
            Some(session) => viewer.retain(|item| {
                session
                    .histories
                    .get(&item.chat_id.0)
                    .is_none_or(|history| !history.is_tombstone(item.message_id))
            }),
            None => false,
        };
        self.viewer.state = viewer;
        if !changed {
            return;
        }
        if self.viewer.state.is_open() {
            self.reset_viewer_item_state(cx);
        } else {
            self.stop_viewer_video();
        }
        cx.notify();
    }

    /// Cmd+C: copy the current photo to the clipboard as an image (with
    /// the viewer's rotation and flips applied).
    pub(in crate::ui) fn copy_viewer_photo(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.viewer.state.current().cloned() else {
            return;
        };
        if !item.kind.is_picture() {
            self.connection
                .set_note(StatusNote::info(tr(keys::Q_AV_ONLY_PHOTOS_CAN_BE_COPIED)));
            cx.notify();
            return;
        }
        if self.viewer_is_timed() || self.refuse_protected_copy(item.chat_id, cx) {
            return;
        }
        let no_files = quill::state::FileMap::default();
        let files = self.session().map_or(&no_files, |s| &s.media.files);
        let roots = self.media_display_roots();
        let orientation = self.viewer.orientation;
        let Some(path) = viewer_display_path(&item, files, &roots) else {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_AV_DOWNLOAD_THE_PHOTO_FIRST_TO_2,
            )));
            cx.notify();
            return;
        };
        // Decode, orient and PNG-encode off the main thread.
        let job = cx.background_spawn(async move {
            let rgba = crate::ui::image_decode::decode_file(&path, DecodeSize::FULL).ok()?;
            let (pixels, width, height) =
                orient_rgba(rgba.as_raw(), rgba.width(), rgba.height(), orientation);
            let oriented = image::RgbaImage::from_raw(width, height, pixels)?;
            let mut bytes = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgba8(oriented)
                .write_to(&mut bytes, image::ImageFormat::Png)
                .ok()?;
            Some(bytes.into_inner())
        });
        self.spawn_scoped(cx, async move |this, cx| {
            let png = job.await;
            let _ = this.update(cx, |this, cx| {
                match png {
                    Some(bytes) => {
                        cx.write_to_clipboard(ClipboardItem::new_image(
                            &gpui_kit::Image::from_bytes(gpui_kit::ImageFormat::Png, bytes),
                        ));
                        this.connection
                            .set_note(StatusNote::info(tr(keys::Q_AV_PHOTO_COPIED)));
                    }
                    None => this
                        .connection
                        .set_note(StatusNote::error(tr(keys::Q_AV_COULDNT_COPY_THIS_PHOTO))),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// "Copy Frame": the video frame on screen to the clipboard as an
    /// image (tdesktop's viewer context menu offers it for videos). Needs
    /// the in-viewer frames; a native-surface or still-loading clip says
    /// so instead of copying a thumbnail.
    pub(in crate::ui) fn copy_viewer_frame(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.viewer.state.current().cloned() else {
            return;
        };
        if self.viewer_is_timed() || self.refuse_protected_copy(item.chat_id, cx) {
            return;
        }
        let png = self
            .viewer_oriented_frame()
            .ok_or(tr(keys::Q_AV_A_FRAME_CAN_ONLY_BE))
            .and_then(|frame| {
                let size = frame.size(0);
                let (width, height) = (size.width.0 as u32, size.height.0 as u32);
                let mut pixels = frame
                    .as_bytes(0)
                    .ok_or(tr(keys::Q_AV_COULDNT_COPY_THIS_FRAME))?
                    .to_vec();
                // RenderImage pixels are BGRA.
                for pixel in pixels.as_chunks_mut::<4>().0 {
                    pixel.swap(0, 2);
                }
                let rgba = image::RgbaImage::from_raw(width, height, pixels)
                    .ok_or(tr(keys::Q_AV_COULDNT_COPY_THIS_FRAME))?;
                let mut bytes = std::io::Cursor::new(Vec::new());
                image::DynamicImage::ImageRgba8(rgba)
                    .write_to(&mut bytes, image::ImageFormat::Png)
                    .map_err(|_| tr(keys::Q_AV_COULDNT_COPY_THIS_FRAME))?;
                Ok(bytes.into_inner())
            });
        match png {
            Ok(bytes) => {
                cx.write_to_clipboard(ClipboardItem::new_image(&gpui_kit::Image::from_bytes(
                    gpui_kit::ImageFormat::Png,
                    bytes,
                )));
                self.connection
                    .set_note(StatusNote::info(tr(keys::Q_AV_FRAME_COPIED)));
            }
            Err(note) => self.connection.set_note(StatusNote::error(note)),
        }
        cx.notify();
    }

    /// "View all media": close the viewer and open the chat's Shared
    /// Media gallery on its photos and videos (tdesktop opens the same
    /// list as the viewer's paging source).
    pub(in crate::ui) fn view_all_viewer_media(&mut self, cx: &mut Context<Self>) {
        self.close_media_viewer(cx);
        self.open_shared_media_ui(cx);
        self.select_shared_media_tab_ui(quill::state::SharedMediaTab::Media, cx);
    }

    /// The sender name and date shown under the viewer's title: who sent
    /// the photo and "today at 14:05" (tdesktop's viewer header). Profile
    /// photos carry no message, so they show neither.
    pub(super) fn viewer_sender_line(&self, item: &MediaViewerItem) -> Option<(String, String)> {
        let session = self.session()?;
        let message = session
            .histories
            .get(&item.chat_id.0)?
            .messages
            .get(&item.message_id.0)?;
        let name = match message.sender {
            Some(quill::telegram::envelope::MessageSender::User { user_id }) => {
                session.user(user_id).map(|user| user.display_name())
            }
            Some(quill::telegram::envelope::MessageSender::Chat { chat_id }) => {
                session.chats.get(&chat_id).map(|chat| chat.title.clone())
            }
            None => None,
        }
        .or_else(|| {
            session
                .chats
                .get(&item.chat_id.0)
                .map(|chat| chat.title.clone())
        })?;
        if message.date <= 0 {
            return Some((name, String::new()));
        }
        let when = quill::local_time::viewer_stamp(
            &quill::local_time::civil_local(i64::from(message.date)),
            &quill::local_time::civil_local(quill::local_time::now_unix()),
        );
        Some((name, when))
    }

    /// Whose profile the sender name opens (tdesktop `Over::Name`).
    pub(super) fn viewer_sender_profile(
        &self,
        item: &MediaViewerItem,
    ) -> Option<quill::viewer_extras::SenderProfile> {
        let session = self.session()?;
        let message = session
            .histories
            .get(&item.chat_id.0)?
            .messages
            .get(&item.message_id.0)?;
        let sender_chat = match message.sender {
            Some(quill::telegram::envelope::MessageSender::Chat { chat_id }) => {
                session.chats.get(&chat_id).map(|chat| &chat.kind)
            }
            _ => None,
        };
        let chat = session.chats.get(&item.chat_id.0).map(|chat| &chat.kind);
        quill::viewer_extras::sender_profile(message.sender.as_ref(), sender_chat, chat)
    }

    /// The sender name was clicked: close the viewer and show the profile.
    pub(super) fn open_viewer_sender_profile(
        &mut self,
        profile: quill::viewer_extras::SenderProfile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use quill::state::InfoPanelTarget;
        use quill::viewer_extras::SenderProfile;
        let target = match profile {
            SenderProfile::User(id) => InfoPanelTarget::User(id),
            SenderProfile::Supergroup(id) => InfoPanelTarget::Supergroup(id),
            SenderProfile::BasicGroup(id) => InfoPanelTarget::BasicGroup(id),
        };
        self.close_media_viewer(cx);
        self.open_info_panel_target(target, window, cx);
    }

    /// Mouse-move listener for the control surfaces: keeps them shown.
    pub(super) fn over_controls_listener(
        cx: &mut Context<Self>,
    ) -> impl Fn(&MouseMoveEvent, &mut Window, &mut App) + 'static {
        cx.listener(|this, _: &MouseMoveEvent, _, cx| {
            this.viewer_note_pointer(true, cx);
        })
    }

    /// The pointer moved over the viewer, onto a control or off them. Only
    /// pointer moves change `over_controls`: a click, key or step under a
    /// resting pointer leaves it where the pointer is.
    pub(in crate::ui) fn viewer_note_pointer(
        &mut self,
        over_controls: bool,
        cx: &mut Context<Self>,
    ) {
        self.viewer.over_controls = over_controls;
        self.viewer_note_activity(cx);
    }

    /// The controls finished fading out: from then on they take no pointer
    /// input (`viewer_chrome` leaves them out).
    pub(super) fn viewer_controls_gone(&self) -> bool {
        self.viewer.controls_hidden
            && self.viewer.controls_hidden_at.elapsed()
                >= Duration::from_millis(controls_fade_ms(true))
    }

    /// Draw again once the fade-out ends, so the faded controls drop out.
    fn redraw_viewer_after_fade(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(controls_fade_ms(true)))
                .await;
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }

    /// Activity in the viewer: show the controls and restart the 1100 ms
    /// idle clock (tdesktop `mediaviewWaitHide`). They stay up while the
    /// pointer rests on one (`over_controls`).
    pub(in crate::ui) fn viewer_note_activity(&mut self, cx: &mut Context<Self>) {
        self.viewer.last_activity = std::time::Instant::now();
        if self.viewer.controls_hidden {
            self.viewer.controls_hidden = false;
            self.viewer.controls_gen += 1;
            cx.notify();
        }
        if self.viewer.hide_timer || controls_stay_up() {
            return;
        }
        self.viewer.hide_timer = true;
        self.spawn_scoped(cx, async move |this, cx| {
            loop {
                let wait = this
                    .update(cx, |this, cx| {
                        let idle = this.viewer.last_activity.elapsed().as_millis() as u64;
                        let done = !this.viewer.state.is_open() || this.viewer.controls_hidden;
                        // Never mid-drag (a seek, volume or speed thumb).
                        let held = this.viewer.over_controls || this.viewer.controls.dragging();
                        if !done && controls_should_hide(idle, held) {
                            this.viewer.controls_hidden = true;
                            this.viewer.controls_hidden_at = std::time::Instant::now();
                            this.viewer.controls_gen += 1;
                            this.redraw_viewer_after_fade(cx);
                            cx.notify();
                        } else if !done {
                            return Some(if held {
                                VIEWER_WAIT_HIDE_MS
                            } else {
                                controls_hide_wait_ms(idle).max(16)
                            });
                        }
                        this.viewer.hide_timer = false;
                        None
                    })
                    .ok()
                    .flatten();
                match wait {
                    Some(ms) => {
                        cx.background_executor()
                            .timer(Duration::from_millis(ms))
                            .await;
                    }
                    None => break,
                }
            }
        })
        .detach();
    }

    /// "Attached Stickers": the sticker sets whose stickers were added to
    /// the photo or video (`getAttachedStickerSets`); the first opens in
    /// the sticker set dialog.
    pub(in crate::ui) fn show_viewer_attached_stickers(&mut self, cx: &mut Context<Self>) {
        let Some(file_id) = self
            .viewer
            .state
            .current()
            .map(|item| item.download_file_id)
        else {
            return;
        };
        self.message_ui.menu_ui.sticker_set_open = true;
        if let Some(live) = self.live.as_mut()
            && live.driver.fetch_attached_sticker_sets(file_id).is_err()
        {
            self.message_ui.menu_ui.sticker_set_open = false;
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_AV_COULD_NOT_LOAD_THE_ATTACHED,
            )));
        }
        cx.notify();
    }

    /// MED1: "Show in chat" — close the viewer and jump to the source
    /// message (the reply-jump machinery, reused).
    pub(in crate::ui) fn show_viewer_in_chat(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.viewer.state.current() else {
            return;
        };
        let message_id = item.message_id;
        self.close_media_viewer(cx);
        self.jump_to_replied_message(message_id, cx);
    }

    /// Whether the viewer offers Reply for `item`: a sent message of the
    /// open chat (not a profile photo or poll option, not timed media)
    /// while that chat shows its composer (tdesktop `canWriteMessage()`).
    pub(super) fn viewer_can_reply(&self, item: &MediaViewerItem, timed: bool) -> bool {
        if timed
            || !matches!(
                self.viewer.state.source(),
                ViewerSource::Chat | ViewerSource::SharedMedia
            )
        {
            return false;
        }
        let Some(session) = self.session() else {
            return false;
        };
        // Shared Media pages past the loaded history are sent messages.
        let sent = item.message_id.0 > 0
            && session
                .histories
                .get(&item.chat_id.0)
                .and_then(|history| history.messages.get(&item.message_id.0))
                .is_none_or(|message| !message.pending);
        sent && session.open_chat == Some(item.chat_id) && self.composer_available(self.pane_mode())
    }

    /// Reply from the viewer: close it and start a reply to the shown
    /// message, the composer focused.
    pub(in crate::ui) fn reply_from_viewer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.viewer.state.current() else {
            return;
        };
        let (chat_id, message_id) = (item.chat_id, item.message_id);
        self.close_media_viewer(cx);
        self.begin_reply_from_message(chat_id, message_id, window, cx);
    }

    /// MED1: pin/unpin the album the viewer item belongs to. TGX
    /// `MessagePinAlbum` pins each member (`pinChatMessage` per message —
    /// TDLib 1.8.67 has no album-level pin, schema line 13559). When any
    /// member is pinned the action unpins the pinned members instead.
    /// Rights-gated on `ChatSummary::can_pin_messages`.
    pub(in crate::ui) fn toggle_viewer_album_pin(&mut self, cx: &mut Context<Self>) {
        let (chat_id, album_id) = match self.viewer.state.current() {
            Some(item) => (item.chat_id, self.viewer_album_id(item.message_id)),
            None => return,
        };
        let Some(album_id) = album_id else { return };
        let can_pin = self
            .session()
            .and_then(|s| s.chats.get(&chat_id.0))
            .is_some_and(|chat| chat.can_pin_messages());
        if !can_pin {
            self.connection
                .set_note(StatusNote::error(tr(keys::Q_AV_YOU_CANT_PIN_MESSAGES_IN)));
            cx.notify();
            return;
        }
        let history: Vec<HistoryMessage> = self
            .session()
            .and_then(|s| s.histories.get(&chat_id.0))
            .map(|h| h.ordered().into_iter().cloned().collect())
            .unwrap_or_default();
        let ids = quill::album::album_message_ids(&history, album_id);
        if ids.is_empty() {
            return;
        }
        let pinned: Vec<MessageId> = ids
            .iter()
            .filter(|id| history.iter().any(|m| m.id == **id && m.is_pinned))
            .copied()
            .collect();
        if self.demo_session.is_some() {
            // Demo: toggle only the messages whose pin state must change.
            let pin_target = pinned.is_empty();
            for id in &ids {
                let currently = history.iter().any(|m| m.id == *id && m.is_pinned);
                if currently != pin_target {
                    self.apply_demo_pin_toggle(chat_id, *id);
                }
            }
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_AV_DEMO_ALBUM_PIN_UPDATED)));
            cx.notify();
            return;
        }
        let Some(live) = self.live.as_mut() else {
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_AV_NO_LIVE_CONNECTION)));
            cx.notify();
            return;
        };
        let mut failed = 0;
        if pinned.is_empty() {
            for id in &ids {
                if live.driver.pin_chat_message(chat_id, *id, false).is_err() {
                    failed += 1;
                }
            }
            self.connection.set_note(if failed == 0 {
                StatusNote::silent(tr(keys::Q_AV_PINNING_ALBUM))
            } else {
                StatusNote::error(tr_with(
                    keys::Q_AV_COULDNT_PIN_ALBUM_ITEM_S,
                    &[("failed", &failed.to_string())],
                ))
            });
        } else {
            for id in &pinned {
                if live.driver.unpin_chat_message(chat_id, *id).is_err() {
                    failed += 1;
                }
            }
            self.connection.set_note(if failed == 0 {
                StatusNote::silent(tr(keys::Q_AV_UNPINNING_ALBUM))
            } else {
                StatusNote::error(tr_with(
                    keys::Q_AV_COULDNT_UNPIN_ALBUM_ITEM_S,
                    &[("failed", &failed.to_string())],
                ))
            });
        }
        cx.notify();
    }

    /// MED1: the `media_album_id` of the history message behind the viewer
    /// item (`None` when the message is not in an album).
    pub(in crate::ui) fn viewer_album_id(&self, message_id: MessageId) -> Option<i64> {
        let item = self.viewer.state.current()?;
        self.session()?
            .histories
            .get(&item.chat_id.0)?
            .messages
            .get(&message_id.0)
            .and_then(|m| (m.media_album_id != 0).then_some(m.media_album_id))
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "reply_ui_tests.rs"]
mod reply_ui_tests;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "rotate_ui_tests.rs"]
mod rotate_ui_tests;
