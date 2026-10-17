//! Profile photo sources in the edit-profile dialog (tdesktop
//! `UserpicButton` menu): the camera with a live preview, the emoji and
//! sticker avatar builder with its gradient editor, and a video avatar.
//! The camera and the emoji open a panel in place of the dialog's normal
//! body; the video opens the full video editor over the dialog. The
//! request bodies, the gradient drawing and the ffmpeg command lines are
//! in `quill::profile_photo_*`.
#![deny(clippy::indexing_slicing)]

mod camera;
mod emoji;
mod gradient_editor;
mod video;

use super::app::QuillApp;
use crate::ui::user_error::user_error_for;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use crate::ui::status_note::StatusNote;
pub(crate) use camera::CameraDraft;
pub(crate) use emoji::EmojiBuilder;

/// Which source panel the dialog shows.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum SourceMode {
    /// The dialog's normal body.
    #[default]
    Closed,
    Emoji,
    Camera,
}

/// State of the source panels. Lives in the edit-profile dialog.
#[derive(Default)]
pub(crate) struct PhotoSources {
    mode: SourceMode,
    emoji: EmojiBuilder,
    camera: CameraDraft,
    busy: bool,
    note: Option<String>,
    /// Bumped per request so a late result for an older one is dropped.
    serial: u32,
}

fn work_dir() -> PathBuf {
    let dir = std::env::temp_dir().join("quill-profile-photo");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Run ffmpeg to completion; the error is a short sentence for the panel.
fn run_ffmpeg(args: &[String], log: &Path) -> Result<(), String> {
    let side = quill::profile_photo_video::AVATAR_VIDEO_SIDE;
    let job = quill::video_encoder::h264_job(
        tr(keys::Q_PEOPLE_VIDEO_AVATARS),
        args,
        side.saturating_mul(side),
    )?;
    let log_file = std::fs::File::create(log).map_err(|e| e.to_string())?;
    let status = quill::video_encoder::quiet_command_for(&job.program)
        .args(["-hide_banner", "-loglevel", "error"])
        .args(&job.args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log_file)
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(quill::media_tools::capture_failure(log, true))
    }
}

/// A GPUI picture from `width` x `height` pixels given as RGBA. GPUI draws
/// BGRA, so the red and blue bytes swap.
fn render_rgba(width: u32, height: u32, mut rgba: Vec<u8>) -> Option<Arc<RenderImage>> {
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let buffer = image::RgbaImage::from_raw(width, height, rgba)?;
    Some(Arc::new(RenderImage::new(smallvec::SmallVec::from_buf([
        image::Frame::new(buffer),
    ]))))
}

/// Replace a held picture, handing the old one to the atlas sweeper.
fn replace_image(slot: &mut Option<Arc<RenderImage>>, new: Option<Arc<RenderImage>>) {
    if let Some(old) = std::mem::replace(slot, new) {
        super::image_budget::retire_all([old]);
    }
}

impl QuillApp {
    fn sources_mut(&mut self) -> Option<&mut PhotoSources> {
        self.dialogs
            .edit_profile_dialog
            .as_mut()
            .map(|dialog| &mut dialog.sources)
    }

    fn sources(&self) -> Option<&PhotoSources> {
        self.dialogs
            .edit_profile_dialog
            .as_ref()
            .map(|dialog| &dialog.sources)
    }

    /// Called as the dialog renders: fetch thumbnails the open panel needs
    /// and pull the newest camera frame.
    pub(super) fn photo_sources_prepare_render(&mut self, cx: &mut Context<Self>) {
        match self.sources().map(|s| s.mode) {
            Some(SourceMode::Emoji) => self.emoji_prepare_render(),
            Some(SourceMode::Camera) => self.camera_prepare_render(cx),
            _ => {}
        }
    }

    pub(super) fn open_photo_source(
        &mut self,
        mode: SourceMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if mode == SourceMode::Emoji
            && let Some(live) = self.live.as_mut()
        {
            let _ = live.driver.load_profile_photo_emoji_icons();
            let _ = live.driver.fetch_editor_stickers();
        }
        if let Some(sources) = self.sources_mut() {
            sources.mode = mode;
            sources.note = None;
            sources.busy = false;
        }
        if mode == SourceMode::Camera {
            self.start_camera(window, cx);
        }
        cx.notify();
    }

    fn close_photo_source(&mut self, cx: &mut Context<Self>) {
        if let Some(sources) = self.sources_mut() {
            sources.mode = SourceMode::Closed;
            sources.camera = CameraDraft::default();
            sources.emoji.close_editor();
            sources.note = None;
            sources.busy = false;
            sources.serial += 1;
        }
        cx.notify();
    }

    fn photo_source_note(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        if let Some(sources) = self.sources_mut() {
            sources.note = Some(text.into());
            sources.busy = false;
        }
        cx.notify();
    }

    /// Send a request and close the panel; a failure stays on the panel.
    fn finish_photo_source(
        &mut self,
        send: impl FnOnce(
            &mut quill::connect::ConnectDriver<quill::connect::LiveSender>,
        ) -> Result<quill::ids::RequestId, quill::connect::ConnectSendError>,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.live.as_mut() else {
            self.photo_source_note(tr(keys::Q_PEOPLE_PROFILE_EDITS_NEED_A_SIGNED), cx);
            return;
        };
        live.driver.session.users_state.profile_edit_error = None;
        match send(&mut live.driver) {
            Ok(_) => {
                self.connection.set_note(StatusNote::silent(tr(
                    keys::Q_PEOPLE_PHOTO_UPDATE_REQUESTED,
                )));
                self.close_photo_source(cx);
            }
            Err(err) => {
                self.photo_source_note(user_error_for(keys::Q_NOTES_FAILED_SET_PHOTO, &err), cx)
            }
        }
    }

    /// The panel for the open source, or `None` for the normal body.
    pub(super) fn photo_sources_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let sources = self.sources()?;
        let body = match sources.mode {
            SourceMode::Closed => return None,
            SourceMode::Emoji => self.emoji_panel(sources, cx),
            SourceMode::Camera => self.camera_panel(sources, cx),
        };
        let theme = cx.theme();
        let note = sources.note.clone().map(|text| {
            div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(text)
        });
        Some(
            div()
                .id("profile-photo-source")
                .flex()
                .flex_col()
                .gap_3()
                .child(body)
                .children(note)
                .into_any_element(),
        )
    }

    /// Back and the confirming button under a panel.
    fn source_buttons(
        &self,
        use_label: &'static str,
        can_use: bool,
        on_use: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .gap_2()
            .child(
                Button::new("profile-photo-source-back")
                    .label(tr(keys::CREATE_GROUP_BACK))
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.close_photo_source(cx))),
            )
            .child(
                Button::new("profile-photo-source-use")
                    .label(use_label)
                    .disabled(!can_use)
                    .on_click(cx.listener(move |this, _, _, cx| on_use(this, cx))),
            )
            .into_any_element()
    }

    /// A round picture of `side` px; `image` fills it.
    fn round_picture(&self, side: f32, image: Option<Arc<RenderImage>>, cx: &App) -> Div {
        let base = div()
            .size(px(side))
            .flex_none()
            .rounded_full()
            .bg(cx.theme().muted);
        match image {
            Some(image) => base.child(
                img(ImageSource::Render(image))
                    .size(px(side))
                    .rounded_full()
                    .object_fit(ObjectFit::Cover),
            ),
            None => base,
        }
    }

    /// The buttons under the Photo section.
    pub(super) fn photo_source_buttons(&self, cx: &mut Context<Self>) -> AnyElement {
        let button = |id: &'static str,
                      label: &'static str,
                      mode: SourceMode,
                      cx: &mut Context<Self>| {
            Button::new(id).label(label).ghost().on_click(
                cx.listener(move |this, _, window, cx| this.open_photo_source(mode, window, cx)),
            )
        };
        div()
            .flex()
            .flex_wrap()
            .gap_2()
            .child(button(
                "edit-profile-camera",
                tr(keys::Q_PEOPLE_CAMERA),
                SourceMode::Camera,
                cx,
            ))
            .child(button(
                "edit-profile-emoji",
                tr(keys::Q_PEOPLE_USE_AN_EMOJI),
                SourceMode::Emoji,
                cx,
            ))
            // Straight to the file picker, then the video editor (tdesktop
            // `PrepareProfileVideo`).
            .child(
                Button::new("edit-profile-video")
                    .label(tr(keys::Q_PEOPLE_VIDEO_2))
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.choose_avatar_video(window, cx);
                    })),
            )
            .into_any_element()
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod tests;
