//! Sticker attachments.

use super::*;
use quill::lang::{keys, tr, tr_with};

pub(in crate::ui) fn sticker_attachment(
    row_id: u64,
    sticker: &quill::telegram::envelope::StickerContent,
    files: &quill::state::FileMap,
    downloading: &quill::state::DownloadIntents,
    media_roots: &[PathBuf],
    animated: Option<crate::ui::sticker_player::StickerView>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    // Telegram Desktop `Sticker::Size`: the sticker's own size, downscaled
    // into `maxStickerSize` (224).
    let (w, h) = sticker_box(sticker.width, sticker.height);
    // Drawn by the conversation's animation layer (`anim_layer`), or in
    // place outside the conversation, at the box's pixel size.
    if let Some(view) = animated {
        return crate::ui::anim_layer::frames(view)
            .mt_2()
            .flex_none()
            .w(w)
            .h(h)
            .into_any_element();
    }
    let display_id = sticker.display_file_id().unwrap_or(sticker.file_id);
    let fallback_label = sticker_label(sticker);
    if let Some(path) = [Some(display_id), sticker.thumb_file_id]
        .into_iter()
        .flatten()
        .filter_map(|id| files.get(&id.0).and_then(|file| file.usable_path()))
        .find_map(|path| sandboxed_display_path(path, media_roots))
    {
        let fallback_label = fallback_label.clone();
        return img(crate::ui::image_budget::sized_media(
            &path,
            (w, h),
            Some((sticker.width, sticker.height)),
            crate::ui::image_budget::Fit::Contain,
        ))
        .id(("sticker-img", row_id))
        .mt_2()
        .w(w)
        .h(h)
        .aspect_ratio(w / h)
        .object_fit(ObjectFit::Contain)
        .with_fallback(move || {
            div()
                .w(w)
                .h(h)
                .rounded_md()
                .bg(fill_muted())
                .flex()
                .items_center()
                .justify_center()
                .child(fallback_label.clone())
                .into_any_element()
        })
        .into_any_element();
    }
    // Until the image lands: the sticker's emoji, faded, in a box the size
    // of the sticker itself so the row doesn't jump when it arrives.
    let downloading_now = file_is_downloading(display_id, files, downloading);
    let label = if downloading_now {
        tr_with(
            keys::Q_MEDIA_DOWNLOADING_3,
            &[("sticker_label", &(sticker_label(sticker)).to_string())],
        )
    } else {
        sticker_label(sticker)
    };
    let glyph = if sticker.emoji.is_empty() {
        "🙂".to_string()
    } else {
        sticker.emoji.clone()
    };
    div()
        .id(("sticker-ph", row_id))
        .mt_2()
        .w(w)
        .h(h)
        .flex_none()
        .rounded_lg()
        .bg(fill_muted().opacity(0.5))
        .flex()
        .items_center()
        .justify_center()
        .role(gpui_kit::Role::Image)
        .aria_label(label)
        .when(display_id.0 != 0 && !downloading_now, |this| {
            this.role(gpui_kit::Role::Button)
                .tab_index(0)
                .cursor_pointer()
                .pressable(cx.theme())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.request_media_download(display_id, None, cx);
                }))
        })
        .child(
            div()
                .text_size(px(56.))
                .opacity(if downloading_now { 0.35 } else { 0.6 })
                .child(glyph),
        )
        .into_any_element()
}

/// The box a sticker of `width`x`height` pixels draws in.
pub(in crate::ui) fn sticker_box(width: i32, height: i32) -> (Pixels, Pixels) {
    let size = quill::bubble_layout::sticker_size(quill::bubble_layout::Size::new(width, height));
    (px(size.w as f32), px(size.h as f32))
}

pub(in crate::ui) fn sticker_label(sticker: &quill::telegram::envelope::StickerContent) -> String {
    if sticker.emoji.is_empty() {
        tr(keys::IN_DLG_STICKER).into()
    } else {
        tr_with(
            keys::Q_MEDIA_STICKER_2,
            &[("emoji", &sticker.emoji.to_string())],
        )
    }
}
