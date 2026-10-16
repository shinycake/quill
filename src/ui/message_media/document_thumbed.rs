//! The thumbed file row (tdesktop `HistoryDocumentThumbed`,
//! `st::msgFileThumbLayout`): a document that carries a thumbnail — a
//! picture sent as a file, a PDF with a cover — shows that picture at 72 px
//! beside its name, its size and one action link ("Open with", "Download"
//! or "Cancel"). Clicking the picture or the name opens it (a picture in
//! the viewer), downloads it first, or cancels the download.

use super::document::DocOrigin;
use super::*;
use crate::ui::image_budget::{Fit, sized_media};
use quill::lang::{keys, tr, tr_with};

/// `msgFileThumbLayout.thumbSize`.
const THUMB: f32 = 72.0;
/// `msgFileThumbLayout.thumbSkip`.
const THUMB_SKIP: f32 = 14.0;
/// `msgFileLayout.thumbSize`: the download / cancel disc over the picture.
const DISC: f32 = 44.0;

#[allow(clippy::too_many_arguments)]
pub(in crate::ui) fn thumbed_document_chip(
    row_id: u64,
    doc: &quill::telegram::envelope::DocumentContent,
    outgoing: bool,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    failed: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    origin: DocOrigin,
    plain: bool,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let file_id = doc.file_id;
    let sponsored = origin.sponsored();
    let file = files.get(&file_id.0);
    let ready = file.and_then(|f| f.usable_path()).is_some();
    let downloading_now = file_is_downloading(file_id, files, downloading);
    let failed_now = !ready && !downloading_now && failed.contains(&file_id.0);
    let progress = file.and_then(|f| f.download_progress());
    let size = file.map(|f| f.display_size()).unwrap_or(0);
    let status = if downloading_now {
        let done = file.map(|f| f.local.downloaded_size).unwrap_or(0);
        match (done > 0, size > 0) {
            (true, true) => format!("{} of {}", format_bytes(done), format_bytes(size)),
            _ => tr(keys::Q_MISC_DOWNLOADING).to_string(),
        }
    } else if failed_now {
        tr(keys::Q_MISC_DOWNLOAD_FAILED).to_string()
    } else {
        format_bytes(size)
    };
    let name = if doc.file_name.is_empty() {
        tr(keys::ATTACH_DOCUMENT).to_string()
    } else {
        doc.file_name.clone()
    };
    // `lng_media_open_with` / `lng_media_download` / `lng_media_cancel`.
    let link_label = if downloading_now {
        tr(keys::MEDIA_CANCEL)
    } else if ready {
        tr(keys::MEDIA_OPEN_WITH)
    } else {
        tr(keys::MEDIA_DOWNLOAD)
    };
    let open = move |this: &mut QuillApp, cx: &mut Context<QuillApp>| {
        if downloading_now {
            this.cancel_media_download(file_id, cx);
        } else if ready {
            origin.open(file_id, this, cx);
        } else {
            this.request_media_download(file_id, sponsored, cx);
        }
    };
    let link_action = move |this: &mut QuillApp, cx: &mut Context<QuillApp>| {
        if downloading_now {
            this.cancel_media_download(file_id, cx);
        } else if ready {
            this.open_downloaded_file(file_id, cx);
        } else {
            this.request_media_download(file_id, sponsored, cx);
        }
    };
    let thumb = doc.thumbnail.as_ref();
    let thumb_path = thumb
        .and_then(|thumb| files.get(&thumb.file_id.0))
        .and_then(|file| file.usable_path())
        .and_then(|path| sandboxed_display_path(path, media_roots));
    let picture: Option<AnyElement> = match (thumb_path, &doc.minithumbnail) {
        (Some(path), _) => Some(
            img(sized_media(
                &path,
                (px(THUMB), px(THUMB)),
                thumb.map(|thumb| (thumb.width, thumb.height)),
                Fit::Cover,
            ))
            .absolute()
            .inset_0()
            .size_full()
            .object_fit(ObjectFit::Cover)
            .into_any_element(),
        ),
        (None, Some(mini)) if !mini.data.is_empty() => Some(
            img(crate::ui::mini_image::mini_image(mini))
                .absolute()
                .inset_0()
                .size_full()
                .object_fit(ObjectFit::Cover)
                .into_any_element(),
        ),
        _ => None,
    };
    // tdesktop draws the disc over the picture until the file is loaded
    // (`historyFileThumbDownload` / `historyFileThumbCancel` on
    // `msgDateImgBg`).
    let disc = (!ready).then(|| {
        let icon = if downloading_now {
            gpui_kit::assets::IconName::X
        } else if failed_now {
            gpui_kit::assets::IconName::RotateCcw
        } else {
            gpui_kit::assets::IconName::ArrowDown
        };
        div()
            .relative()
            .size(px(DISC))
            .rounded_full()
            .bg(gpui_kit::black().opacity(0.45))
            .flex()
            .items_center()
            .justify_center()
            .when(downloading_now, |this| {
                this.child(
                    div().absolute().inset(px(3.)).child(
                        ProgressCircle::new(("doc-thumb-ring", row_id))
                            .size_full()
                            .color(gpui_kit::white())
                            .loading(progress.is_none())
                            .value(progress.unwrap_or(0.) * 100.),
                    ),
                )
            })
            .child(Icon::new(icon).size(px(20.)).text_color(gpui_kit::white()))
    });
    let open_label = if ready {
        tr_with(keys::Q_MISC_OPEN_2, &[("name", &name.to_string())])
    } else if downloading_now {
        tr_with(
            keys::Q_MISC_CANCEL_DOWNLOAD_OF,
            &[("name", &name.to_string())],
        )
    } else {
        tr_with(keys::Q_MISC_DOWNLOAD, &[("name", &name.to_string())])
    };
    let picture_box = div()
        .id(("doc-thumb", row_id))
        .relative()
        .flex_none()
        .size(px(THUMB))
        .rounded(px(6.))
        .overflow_hidden()
        .bg(fill_muted())
        .flex()
        .items_center()
        .justify_center()
        .role(gpui_kit::Role::Button)
        .aria_label(open_label)
        .tab_index(0)
        .cursor_pointer()
        .pressable(cx.theme())
        .children(picture)
        .children(disc)
        .on_click(cx.listener(move |this, _, _, cx| open(this, cx)));
    let link_color = if outgoing {
        gpui_kit::white()
    } else {
        cx.theme().primary
    };
    let (min_width, max_width) = crate::ui::history::bubble_width::file_row_bounds(plain);
    div()
        .id(("doc-chip", row_id))
        .mt_1()
        .flex()
        .items_center()
        .gap(px(THUMB_SKIP))
        .min_w(min_width)
        .max_w(max_width)
        .child(picture_box)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .h(px(THUMB))
                .flex()
                .flex_col()
                .justify_center()
                .gap_0p5()
                .child(
                    div()
                        .id(("doc-chip-name", row_id))
                        .cursor_pointer()
                        .text_sm()
                        .font_semibold()
                        .truncate()
                        .child(name)
                        .on_click(cx.listener(move |this, _, _, cx| open(this, cx))),
                )
                .child(div().text_xs().opacity(0.7).child(status))
                .child(
                    div()
                        .id(("doc-action", row_id))
                        .role(gpui_kit::Role::Button)
                        .aria_label(link_label)
                        .tab_index(0)
                        .cursor_pointer()
                        .mt_1()
                        .text_sm()
                        .font_semibold()
                        .text_color(link_color)
                        .hover(|style| style.underline())
                        .child(link_label)
                        .on_click(cx.listener(move |this, _, _, cx| link_action(this, cx))),
                ),
        )
        .into_any_element()
}
