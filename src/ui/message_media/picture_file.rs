//! A picture sent as a file, shown in the chat as the photo it is: framed
//! like a compressed photo, with a pill in the top-left corner marking it
//! as a file (a file glyph and the format: `HEIC`, `PNG`, plus `HDR` when
//! the file carries HDR data) and its name and size along the bottom. The
//! thumbnail stands in until the file has loaded; a click opens the
//! viewer. A file Quill can't decode keeps showing its thumbnail. Files
//! without a thumbnail, or too big to open in the viewer, keep the file
//! row (`quill::media_viewer::shows_as_photo`).

use super::layout::{MediaDisc, download_fraction, media_disc};
use super::*;
use crate::ui::image_budget::{Fit, sized_media};
use quill::lang::{keys, tr, tr_with};
use quill::telegram::envelope::DocumentContent;

#[allow(clippy::too_many_arguments)]
pub(in crate::ui) fn picture_file_attachment(
    row_id: u64,
    doc: &DocumentContent,
    files: &quill::state::FileMap,
    downloading: &quill::state::DownloadIntents,
    media_roots: &[PathBuf],
    (chat_id, message_id): (ChatId, MessageId),
    (frame_w, frame_h): (Pixels, Pixels),
    corners: MediaCorners,
    // The GIF looping in the row (`gif_clip`), for an `image/gif` file.
    gif: Option<crate::ui::gif_clip::GifPlay>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let file_id = doc.file_id;
    let file = files.get(&file_id.0);
    let display = |id: FileId| {
        files
            .get(&id.0)
            .and_then(|file| file.usable_path())
            .and_then(|path| sandboxed_display_path(path, media_roots))
    };
    let full_path = display(file_id);
    let thumb = doc.thumbnail.as_ref();
    let thumb_path = thumb.and_then(|thumb| display(thumb.file_id));
    let dims = thumb.map(|thumb| (thumb.width, thumb.height));
    let downloading_now = full_path.is_none() && file_is_downloading(file_id, files, downloading);
    let size = file.map(ParsedFile::display_size).unwrap_or(doc.size);
    let name = if doc.file_name.is_empty() {
        tr(keys::Q_REST_PICTURE).to_string()
    } else {
        doc.file_name.clone()
    };
    let status = if downloading_now {
        let done = file.map(|f| f.local.downloaded_size).unwrap_or(0);
        if done > 0 && size > 0 {
            format!("{} / {}", format_bytes(done), format_bytes(size))
        } else {
            format_bytes(size)
        }
    } else {
        format_bytes(size)
    };
    let fill = move |element: Img| {
        element
            .absolute()
            .inset_0()
            .size_full()
            .map(|this| corners.round(this))
            .object_fit(ObjectFit::Cover)
    };
    // The thumbnail (or the minithumbnail) underneath; the file on top once
    // it is here. A file that fails to decode leaves the thumbnail showing.
    let preview: Option<AnyElement> = match (&thumb_path, &doc.minithumbnail) {
        (Some(path), _) => Some(
            fill(img(sized_media(path, (frame_w, frame_h), dims, Fit::Cover))).into_any_element(),
        ),
        (None, Some(mini)) if !mini.data.is_empty() => {
            Some(fill(img(crate::ui::mini_image::mini_image(mini))).into_any_element())
        }
        _ => None,
    };
    let hdr = full_path
        .as_deref()
        .is_some_and(|path| crate::ui::hdr_image::has_hdr(path, cx));
    let format = quill::media_viewer::picture_format_label(doc);
    let still = full_path.as_ref().map(|path| {
        fill(img(sized_media(path, (frame_w, frame_h), dims, Fit::Cover)))
            .with_fallback(|| div().into_any_element())
            .into_any_element()
    });
    let picture = match gif {
        // Drawn by the conversation's animation layer, which paints over
        // the row, so the pill and the name ride with the picture.
        Some(gif) => {
            let (format, name, status) = (format.clone(), name.clone(), status.clone());
            let draw = move |frame: Arc<RenderImage>| {
                div()
                    .absolute()
                    .inset_0()
                    .child(fill(img(ImageSource::Render(frame))))
                    .child(file_pill(format.clone(), hdr))
                    .child(name_badge(name.clone(), status.clone(), frame_w))
                    .into_any_element()
            };
            let under = gif.frame_now().map(&draw).or(still);
            let fps = gif.fps();
            Some(
                crate::ui::anim_layer::tile(
                    div().absolute().inset_0().children(under),
                    fps,
                    move || gif.frame_now().map(&draw),
                )
                .into_any_element(),
            )
        }
        None => still,
    };
    let disc = full_path.is_none().then(|| {
        let state = if downloading_now {
            MediaDisc::Progress(download_fraction(file_id, files))
        } else {
            MediaDisc::Download
        };
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .child(media_disc(("picture-file-disc", row_id), state))
    });
    div()
        .id(("picture-file", row_id))
        .relative()
        .overflow_hidden()
        .w(frame_w)
        .h(frame_h)
        .map(|this| corners.round(this))
        .bg(fill_muted())
        .role(gpui_kit::Role::Button)
        .aria_label(tr_with(
            keys::Q_REST_OPEN_PICTURE_FILE,
            &[("name", &name.to_string())],
        ))
        .tab_index(0)
        .cursor_pointer()
        .children(preview)
        .children(picture)
        .children(disc)
        .child(file_pill(format, hdr))
        .child(name_badge(name, status, frame_w))
        .on_click(cx.listener(move |this, _, _, cx| {
            if !this.open_file_viewer(chat_id, message_id, cx) {
                this.request_media_download(file_id, None, cx);
            }
        }))
        .into_any_element()
}

/// The top-left pill that tells a picture file from a photo: a file glyph,
/// the format and, when the file carries HDR data, an "HDR" tag. Fixed
/// colours: it sits on the picture, not the theme.
fn file_pill(format: String, hdr: bool) -> Div {
    div()
        .absolute()
        .top_1()
        .left_1()
        .flex()
        .items_center()
        .gap_1()
        .px_1p5()
        .py_0p5()
        .rounded_md()
        .bg(gpui_kit::black().opacity(0.5))
        .text_xs()
        .font_semibold()
        .text_color(gpui_kit::white())
        .child(
            Icon::new(gpui_kit::assets::IconName::File)
                .size(px(12.))
                .text_color(gpui_kit::white()),
        )
        .child(format)
        .when(hdr, |this| {
            this.child(
                div()
                    .px_1()
                    .rounded_sm()
                    .bg(gpui_kit::white().opacity(0.22))
                    .child("HDR"),
            )
        })
}

/// The file's name and size (or download progress) along the bottom left,
/// leaving the bottom right to the time.
fn name_badge(name: String, status: String, frame_w: Pixels) -> Div {
    div()
        .absolute()
        .bottom_1()
        .left_1()
        .max_w(frame_w * 0.6)
        .flex()
        .items_center()
        .gap_1()
        .px_1p5()
        .rounded_md()
        .bg(gpui_kit::black().opacity(0.5))
        .text_xs()
        .text_color(gpui_kit::white())
        .child(div().min_w_0().truncate().child(name))
        .child(div().flex_none().opacity(0.8).child(status))
}
