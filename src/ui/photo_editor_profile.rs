//! The profile photo crop (tdesktop `PrepareProfilePhoto`): a picked or
//! taken photo opens the photo editor with a centered square crop shown
//! as a circle; "Set Photo" renders the square, scaled up to at least
//! 640 px (`kProfilePhotoSize`), and hands it to the photo sources'
//! target: the own profile photo is set, a group's waits in its Edit box.
//! The editor shows in a dialog of its own so it sits above the dialog it
//! came from.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::image_decode::{DecodeError, DecodeSize, decode_file, decode_image};
use super::photo_edit::CropRect;
use super::photo_editor::{EditorPurpose, PhotoEditor, preview_of};
use super::profile_photo_sources::{ChosenPhoto, PhotoPreview, PhotoTarget};
use super::shell::{DialogKind, QuillShell};
use super::video_editor::EditorTarget;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::*;
use image::RgbaImage;
use quill::lang::{keys, tr};
use quill::profile_photo_sources::ChatPhotoInput;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// The smallest side a profile photo is sent with (tdesktop
/// `Editor::kProfilePhotoSize`).
pub(super) const PROFILE_PHOTO_SIZE: u32 = 640;

/// tdesktop refuses photos more than ten times wider than tall, or the
/// other way round (`lng_bad_photo`).
fn ratio_is_bad(width: u32, height: u32) -> bool {
    width == 0
        || height == 0
        || u64::from(width) > 10 * u64::from(height)
        || u64::from(height) > 10 * u64::from(width)
}

/// The centered square of the shorter side, in normalized coordinates.
fn centered_square(width: u32, height: u32) -> CropRect {
    if width == 0 || height == 0 {
        return CropRect::FULL;
    }
    let side = width.min(height) as f32;
    let w = side / width as f32;
    let h = side / height as f32;
    CropRect {
        x: (1.0 - w) / 2.0,
        y: (1.0 - h) / 2.0,
        w,
        h,
    }
}

/// A corner drag that keeps the crop square in pixels: the opposite
/// corner stays put and the side follows the farther of the pointer's two
/// distances, kept inside the image. Corners as in `CropDrag::Corner`.
fn square_corner(
    crop: CropRect,
    corner: usize,
    (x, y): (f32, f32),
    (width, height): (u32, u32),
) -> CropRect {
    if width == 0 || height == 0 {
        return crop;
    }
    let (width, height) = (width as f32, height as f32);
    let right = matches!(corner, 1 | 2);
    let bottom = matches!(corner, 2 | 3);
    let anchor_x = if right { crop.x } else { crop.x + crop.w };
    let anchor_y = if bottom { crop.y } else { crop.y + crop.h };
    let room_x = if right { 1.0 - anchor_x } else { anchor_x } * width;
    let room_y = if bottom { 1.0 - anchor_y } else { anchor_y } * height;
    let min_side = CropRect::MIN_SIDE * width.min(height);
    let side = ((x - anchor_x).abs() * width)
        .max((y - anchor_y).abs() * height)
        .max(min_side)
        .min(room_x)
        .min(room_y);
    let (w, h) = (side / width, side / height);
    CropRect {
        x: if right { anchor_x } else { anchor_x - w },
        y: if bottom { anchor_y } else { anchor_y - h },
        w,
        h,
    }
}

/// The finished photo: the crop's render, opaque (tdesktop reads profile
/// photos with `forceOpaque`) and at least `PROFILE_PHOTO_SIZE` square.
fn finish_square(mut image: RgbaImage) -> RgbaImage {
    for pixel in image.pixels_mut() {
        let [r, g, b, a] = pixel.0;
        let blend =
            |c: u8| ((u16::from(c) * u16::from(a) + 255 * (255 - u16::from(a))) / 255) as u8;
        pixel.0 = [blend(r), blend(g), blend(b), 255];
    }
    let (width, height) = image.dimensions();
    if width.min(height) >= PROFILE_PHOTO_SIZE {
        return image;
    }
    let scale = PROFILE_PHOTO_SIZE as f32 / width.min(height).max(1) as f32;
    image::imageops::resize(
        &image,
        ((width as f32 * scale).round() as u32).max(PROFILE_PHOTO_SIZE),
        ((height as f32 * scale).round() as u32).max(PROFILE_PHOTO_SIZE),
        image::imageops::FilterType::Lanczos3,
    )
}

/// Image files tdesktop never sends to the video editor: a decode failure
/// on one of these is `lng_bad_photo`.
fn looks_like_image(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|ext| {
            matches!(
                ext.as_str(),
                "jpg" | "jpeg" | "png" | "webp" | "bmp" | "tif" | "tiff" | "heic" | "heif" | "avif"
            )
        })
}

impl PhotoEditor {
    pub(super) fn is_profile(&self) -> bool {
        matches!(self.purpose, EditorPurpose::Profile(_))
    }

    /// A profile photo keeps its crop square.
    pub(super) fn square_crop(&self) -> bool {
        self.is_profile()
    }

    /// The crop the editor opens with (and "Reset" goes back to).
    pub(super) fn initial_crop(&self) -> CropRect {
        if self.square_crop() {
            let (width, height) = self.dimensions();
            centered_square(width, height)
        } else {
            CropRect::FULL
        }
    }

    pub(super) fn square_corner_drag(&self, corner: usize, point: (f32, f32)) -> CropRect {
        square_corner(self.crop, corner, point, self.dimensions())
    }
}

/// The image on the clipboard, if it holds one (GPUI reads PNG, JPEG,
/// TIFF and the rest on macOS, the image MIME types on X11 and Wayland, and
/// the PNG / DIB formats on Windows).
pub(super) fn clipboard_image(cx: &App) -> Option<Image> {
    cx.read_from_clipboard()?
        .into_entries()
        .find_map(|entry| match entry {
            ClipboardEntry::Image(image) => Some(image),
            _ => None,
        })
}

impl QuillApp {
    /// "Photo or video" (tdesktop `UserpicButton` → `ChoosePhotoOrVideo`):
    /// the native file dialog; a photo opens the crop, anything else the
    /// video editor. An image that does not decode is `lng_bad_photo`.
    pub(super) fn choose_photo_or_video(
        &mut self,
        target: PhotoTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(tr(keys::ATTACH_PHOTO_OR_VIDEO).into()),
        });
        self.spawn_scoped_in(window, cx, async move |this, cx| {
            let Ok(Ok(Some(paths))) = picker.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let read = path.clone();
            let decoded = cx
                .background_executor()
                .spawn(async move { decode_file(&read, DecodeSize::FULL) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.open_picked_photo(target, path, decoded, window, cx);
            });
        })
        .detach();
    }

    /// "From clipboard" (tdesktop `addFromClipboard`): the copied image
    /// opens the crop. Read again on the click: the menu showed the item
    /// when the clipboard held an image, which may have changed since.
    pub(super) fn paste_photo_from_clipboard(
        &mut self,
        target: PhotoTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(image) = clipboard_image(cx) else {
            self.photo_target_note(target, tr(keys::BAD_PHOTO), cx);
            return;
        };
        self.spawn_scoped_in(window, cx, async move |this, cx| {
            let decoded = cx
                .background_executor()
                .spawn(async move { decode_image(&image.bytes, None, DecodeSize::FULL) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.photo_target() != Some(target) {
                    return;
                }
                match decoded {
                    Ok(pixels) => this.open_profile_photo_crop(target, pixels, cx),
                    Err(_) => this.photo_target_note(target, tr(keys::BAD_PHOTO), cx),
                }
            });
        })
        .detach();
    }

    fn open_picked_photo(
        &mut self,
        target: PhotoTarget,
        path: PathBuf,
        decoded: Result<RgbaImage, DecodeError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The box or dialog the photo was for closed meanwhile.
        if self.photo_target() != Some(target) {
            return;
        }
        match decoded {
            Ok(pixels) => self.open_profile_photo_crop(target, pixels, cx),
            Err(_) if looks_like_image(&path) => {
                self.photo_target_note(target, tr(keys::BAD_PHOTO), cx);
            }
            Err(_) => self.open_video_editor(EditorTarget::Avatar, path, None, window, cx),
        }
    }

    /// Open the crop on decoded pixels (the camera's shot comes here too).
    pub(super) fn open_profile_photo_crop(
        &mut self,
        target: PhotoTarget,
        pixels: RgbaImage,
        cx: &mut Context<Self>,
    ) {
        let (width, height) = pixels.dimensions();
        if ratio_is_bad(width, height) {
            self.photo_target_note(target, tr(keys::BAD_PHOTO), cx);
            return;
        }
        let mut editor = PhotoEditor::new(EditorPurpose::Profile(target), pixels);
        editor.crop = editor.initial_crop();
        self.viewer.photo_editor = Some(editor);
        cx.notify();
    }

    /// Decode a file (the camera's shot) off the UI thread, then crop it.
    pub(super) fn open_profile_photo_crop_file(
        &mut self,
        target: PhotoTarget,
        path: PathBuf,
        cx: &mut Context<Self>,
    ) {
        self.decode_image_then(
            path,
            move |this, decoded, cx| match decoded {
                Ok(pixels) if this.photo_target() == Some(target) => {
                    this.open_profile_photo_crop(target, pixels, cx);
                }
                Ok(_) => {}
                Err(_) => this.photo_target_note(target, tr(keys::BAD_PHOTO), cx),
            },
            cx,
        );
    }

    /// A photo problem shown where the person is looking: under the Edit
    /// box's photo, or as a status note for the own profile.
    fn photo_target_note(&mut self, target: PhotoTarget, text: &str, cx: &mut Context<Self>) {
        match target {
            PhotoTarget::Chat(_) => {
                if let Some(edit) = self.group_edit_box_mut() {
                    edit.photo_note = Some(text.into());
                }
            }
            PhotoTarget::NewChat => {
                if let Some(dialog) = self.new_chat_dialog_mut() {
                    dialog.photo_note = Some(text.into());
                }
            }
            PhotoTarget::Profile => self.connection.set_note(StatusNote::error(text)),
        }
        cx.notify();
    }

    /// "Set Photo": render the square off the UI thread, write it, and
    /// hand it to the target.
    pub(super) fn finish_profile_crop(
        &mut self,
        editor: PhotoEditor,
        target: PhotoTarget,
        cx: &mut Context<Self>,
    ) {
        let job = editor.into_render_job();
        cx.notify();
        self.run_off_ui_then(
            move || {
                let square = finish_square(job.render());
                let saved = super::edit_media_tools::write_edited_png(&square)?;
                Some((saved, preview_of(&square)))
            },
            move |this, done, cx| {
                let Some((saved, preview)) = done else {
                    this.photo_target_note(
                        target,
                        tr(keys::Q_MISC_COULDNT_SAVE_THE_EDITED_PHOTO),
                        cx,
                    );
                    return;
                };
                let photo = ChosenPhoto {
                    input: ChatPhotoInput::Static {
                        path: saved.to_string_lossy().into_owned(),
                    },
                    preview: Some(PhotoPreview::Image(preview)),
                };
                if let Err(note) = this.take_chosen_photo(target, photo) {
                    this.photo_target_note(target, &note, cx);
                }
                cx.notify();
            },
            cx,
        );
    }

    /// The crop over the dialog it came from, edge to edge like the video
    /// editor.
    pub(super) fn build_profile_photo_crop_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::ProfilePhotoCrop, |this, _, cx| {
                this.close_photo_editor(cx);
            });
        let body = app.update(cx, |this, cx| this.photo_editor_overlay_body(cx));
        let body = Rc::new(RefCell::new(body));
        dialog
            .overlay(true)
            .close_button(false)
            .w(px(100_000.))
            .margin_top(px(0.))
            .p_0()
            .bg(gpui_kit::black())
            .border_color(gpui_kit::black())
            .content(move |content, window, cx| {
                let margin = cx.theme().spacing_tokens().lg;
                let height = (window.viewport_size().height - margin * 2.).max(px(320.));
                let body = body
                    .borrow_mut()
                    .take()
                    .unwrap_or_else(|| div().into_any_element());
                content.child(div().relative().h(height).w_full().child(body))
            })
            .on_close(on_close)
    }
}

crate::ui::shell::register_dialogs! {
    ProfilePhotoCrop => DialogSpec::new(
        5444,
        |app| app.viewer.photo_editor.as_ref().is_some_and(PhotoEditor::is_profile),
        QuillApp::build_profile_photo_crop_dialog,
    ),
}

#[cfg(test)]
mod tests {
    use super::{centered_square, finish_square, looks_like_image, ratio_is_bad, square_corner};
    use crate::ui::photo_edit::CropRect;
    use std::path::Path;

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn the_crop_opens_on_the_centered_square() {
        let crop = centered_square(400, 200);
        assert!(near(crop.w, 0.5) && near(crop.h, 1.0));
        assert!(near(crop.x, 0.25) && near(crop.y, 0.0));
        let crop = centered_square(100, 300);
        assert!(near(crop.w * 100.0, crop.h * 300.0));
    }

    #[test]
    fn a_corner_drag_stays_square_and_inside() {
        let (w, h) = (400u32, 200u32);
        let start = centered_square(w, h);
        // Bottom-right pulled far out: limited by the image's height.
        let crop = square_corner(start, 2, (1.0, 1.0), (w, h));
        assert!(near(crop.w * w as f32, crop.h * h as f32));
        assert!(crop.x + crop.w <= 1.0 + 1e-4 && crop.y + crop.h <= 1.0 + 1e-4);
        assert!(near(crop.x, start.x) && near(crop.y, start.y));
        // Top-left pulled in: the bottom-right corner stays put.
        let crop = square_corner(start, 0, (0.5, 0.5), (w, h));
        assert!(near(crop.x + crop.w, start.x + start.w));
        assert!(near(crop.y + crop.h, start.y + start.h));
        assert!(near(crop.w * w as f32, crop.h * h as f32));
        // Never smaller than the minimum side.
        let crop = square_corner(start, 0, (0.75, 1.0), (w, h));
        assert!(crop.h >= CropRect::MIN_SIDE - 1e-4);
    }

    #[test]
    fn extreme_ratios_are_refused() {
        assert!(!ratio_is_bad(1000, 100));
        assert!(ratio_is_bad(1001, 100));
        assert!(ratio_is_bad(10, 101));
        assert!(ratio_is_bad(0, 10));
    }

    #[test]
    fn small_photos_are_scaled_up_and_made_opaque() {
        let image = image::RgbaImage::from_pixel(100, 100, image::Rgba([0, 0, 0, 0]));
        let done = finish_square(image);
        assert_eq!(done.dimensions(), (640, 640));
        assert!(done.pixels().all(|p| p.0 == [255, 255, 255, 255]));
        let big = image::RgbaImage::from_pixel(800, 800, image::Rgba([10, 20, 30, 255]));
        assert_eq!(finish_square(big).dimensions(), (800, 800));
    }

    #[test]
    fn only_image_extensions_count_as_bad_photos() {
        assert!(looks_like_image(Path::new("/a/b.JPG")));
        assert!(!looks_like_image(Path::new("/a/clip.mp4")));
        assert!(!looks_like_image(Path::new("/a/noext")));
    }
}
