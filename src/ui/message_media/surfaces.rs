//! Shared visual layers: inline video surfaces, badges, the unseen ring,
//! spoiler covers and blurred previews.

use super::*;

/// An autoplaying clip's current frame, cropped to the media frame.
pub(super) fn inline_surface(
    inline: crate::ui::inline_video::InlineFrame,
    frame_w: Pixels,
    frame_h: Pixels,
    corners: MediaCorners,
) -> AnyElement {
    // A decoded image (FFmpeg) rounds itself like any picture.
    if inline.picture.clips() {
        let radii = Corners {
            top_left: corners.tl,
            top_right: corners.tr,
            bottom_right: corners.br,
            bottom_left: corners.bl,
        };
        return inline
            .picture
            .element(frame_w, frame_h, ObjectFit::Cover, radii);
    }
    #[cfg(target_os = "macos")]
    {
        // The native surface can't be clipped to rounded corners, so a mask
        // in the color behind the tile (the bubble's) cuts them out.
        let mask = crate::ui::inline_video::corner_mask(
            f32::from(frame_w).round() as u32,
            f32::from(frame_h).round() as u32,
            corners.radii(),
            corners.mask_colors(inline.backdrop),
        );
        div()
            .relative()
            .w(frame_w)
            .h(frame_h)
            .overflow_hidden()
            .child(
                inline
                    .picture
                    .element(frame_w, frame_h, ObjectFit::Cover, Corners::default()),
            )
            .child(
                img(ImageSource::Render(mask))
                    .absolute()
                    .inset_0()
                    .w(frame_w)
                    .h(frame_h),
            )
            .into_any_element()
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Only images exist off macOS.
        div().w(frame_w).h(frame_h).into_any_element()
    }
}

/// A round video message's current frame: the square video cut to a
/// circle — an image clips itself; a native surface gets a mask in the
/// history's color that leaves only the circle.
pub(super) fn round_inline_surface(inline: crate::ui::inline_video::InlineFrame) -> AnyElement {
    let diameter = px(VIDEO_NOTE_DIAMETER);
    if inline.picture.clips() {
        let radius = diameter / 2.;
        return inline
            .picture
            .element(diameter, diameter, ObjectFit::Cover, Corners::all(radius));
    }
    // Twice the size for Retina edges.
    let mask =
        crate::ui::inline_video::circle_mask(VIDEO_NOTE_DIAMETER as u32 * 2, inline.backdrop);
    let video = inline
        .picture
        .element(diameter, diameter, ObjectFit::Cover, Corners::default());
    div()
        .relative()
        .size(diameter)
        .child(video)
        .child(
            img(ImageSource::Render(mask))
                .absolute()
                .inset_0()
                .size(diameter),
        )
        .into_any_element()
}

/// An autoplaying clip's tile, `visual` of its frame. When the clip is
/// layered (`InlineFrame::live`), the conversation's animation layer
/// redraws the tile from the player's current frame every tick, over the
/// one the row rendered (`anim_layer::tile`).
pub(super) fn live_tile(
    inline: crate::ui::inline_video::InlineFrame,
    visual: impl Fn(crate::ui::inline_video::InlineFrame) -> AnyElement + 'static,
) -> AnyElement {
    match inline.live.clone() {
        Some(live) => {
            let visual = std::rc::Rc::new(visual);
            let rebuild = visual.clone();
            crate::ui::anim_layer::tile(visual(inline), 30, move || {
                live.frame().map(|f| rebuild(f))
            })
            .into_any_element()
        }
        None => visual(inline),
    }
}

/// The "GIF" badge in a GIF tile's top-left corner.
pub(super) fn gif_badge() -> Div {
    div()
        .absolute()
        .top_1()
        .left_1()
        .px_1p5()
        .rounded_md()
        .bg(gpui_kit::black().opacity(0.5))
        .text_xs()
        .text_color(gpui_kit::white())
        .child("GIF")
}

/// A video tile's duration (time left while it autoplays, with a muted
/// mark) in its bottom-left corner.
pub(super) fn video_badge(duration: String, muted: bool) -> Div {
    div()
        .absolute()
        .bottom_1()
        .left_1()
        .px_1p5()
        .rounded_md()
        .bg(gpui_kit::black().opacity(0.5))
        .text_xs()
        .text_color(gpui_kit::white())
        .flex()
        .items_center()
        .gap_1()
        .child(duration)
        .when(muted, |this| {
            this.child(
                Icon::new(gpui_kit::assets::IconName::VolumeX)
                    .size(px(12.))
                    .text_color(gpui_kit::white()),
            )
        })
}

/// The accent ring over an unseen incoming round video.
pub(super) fn unseen_ring(color: Hsla) -> Div {
    div()
        .absolute()
        .inset_0()
        .rounded_full()
        .border_2()
        .border_color(color)
}

/// A round video's duration, centered near its bottom; a dot while unseen.
pub(super) fn note_duration_badge(duration: String, unseen: bool) -> Div {
    div()
        .absolute()
        .bottom(px(14.))
        .left_0()
        .right_0()
        .flex()
        .justify_center()
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .px_1p5()
                .rounded_md()
                .bg(gpui_kit::black().opacity(0.5))
                .text_xs()
                .text_color(gpui_kit::white())
                .child(duration)
                .when(unseen, |this| {
                    this.child(div().size(px(5.)).rounded_full().bg(gpui_kit::white()))
                }),
        )
}

/// A hidden spoiler (photo, video or GIF): Telegram Desktop covers it with
/// a soft preview under drifting "dust"; a click reveals it for good (until
/// the app restarts). The preview is the inline minithumbnail when there
/// is one: too small to show detail, it reads as a blur.
pub(in crate::ui) fn spoiler_cover(
    row_id: u64,
    chat_id: ChatId,
    message_id: MessageId,
    minithumbnail: Option<&quill::telegram::envelope::MiniThumbnail>,
    // The file to fetch on reveal (photos), so the picture shows at once.
    download: Option<FileId>,
    frame_w: Pixels,
    frame_h: Pixels,
    corners: MediaCorners,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let key = (chat_id.0, message_id.0 as u64, u64::MAX, false);
    // A smooth blur of the inline minithumbnail (scaled up and blurred
    // once, cached): the raw 40 px JPEG scaled to the frame shows blocks.
    let preview = minithumbnail
        .filter(|mini| !mini.data.is_empty())
        .and_then(|mini| blurred_preview(row_id, &mini.data))
        .map(|blurred| {
            img(ImageSource::Render(blurred))
                .absolute()
                .inset_0()
                .size_full()
                .map(|this| corners.round(this))
                .object_fit(ObjectFit::Cover)
        });
    // lib_ui `kImageSpoilerDarkenAlpha` (32 / 255) over the preview.
    let shade = if preview.is_some() { 0.125 } else { 0.1 };
    // tdesktop's spoiler "mess": the shared, pre-rendered speck tile,
    // drawn by the conversation's animation layer (`anim_layer`).
    let dust =
        crate::ui::anim_layer::painter(crate::ui::spoiler_fx::specks_fps(), |bounds, window| {
            crate::ui::spoiler_fx::paint_media_specks(bounds, window);
        })
        .absolute()
        .inset_0()
        .size_full();
    div()
        .id(("spoiler-cover", row_id))
        .relative()
        .overflow_hidden()
        .w(frame_w)
        .h(frame_h)
        .map(|this| corners.round(this))
        .bg(fill_muted())
        .children(preview)
        .child(
            div()
                .absolute()
                .inset_0()
                .map(|this| corners.round(this))
                .bg(gpui_kit::black().opacity(shade)),
        )
        .child(dust)
        .role(gpui_kit::Role::Button)
        .aria_label("Reveal spoiler")
        .tab_index(0)
        .cursor_pointer()
        .on_click(cx.listener(move |this, _, _, cx| {
            this.message_ui.spoiler_revealed.insert(key);
            crate::ui::spoiler_fx::mark_revealed(key);
            if let Some(file_id) = download {
                this.request_media_download(file_id, None, cx);
            }
            cx.notify();
        }))
        .into_any_element()
}

/// The minithumbnail scaled up and Gaussian-blurred, cached per message.
fn blurred_preview(row_id: u64, jpeg: &[u8]) -> Option<Arc<RenderImage>> {
    use crate::ui::lru::Lru;
    use std::cell::RefCell;
    thread_local! {
        static CACHE: RefCell<Lru<u64, Arc<RenderImage>>> = RefCell::new(Lru::new(256));
    }
    if let Some(hit) = CACHE.with(|cache| cache.borrow_mut().get(&row_id)) {
        return Some(hit);
    }
    let small = image::load_from_memory(jpeg).ok()?.to_rgba8();
    let (w, h) = small.dimensions();
    let scale = 160.0 / w.max(h).max(1) as f32;
    let large = image::imageops::resize(
        &small,
        ((w as f32 * scale) as u32).max(1),
        ((h as f32 * scale) as u32).max(1),
        image::imageops::FilterType::Triangle,
    );
    let mut blurred = image::imageops::blur(&large, 6.0);
    for pixel in blurred.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let render = Arc::new(RenderImage::new(smallvec::SmallVec::from_buf([
        image::Frame::new(blurred),
    ])));
    CACHE.with(|cache| {
        if let Some(old) = cache.borrow_mut().insert(row_id, render.clone()) {
            crate::ui::image_budget::retire_all([old]);
        }
    });
    Some(render)
}
