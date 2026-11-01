//! Shared visual layers: inline video surfaces, badges, the unseen ring,
//! spoiler covers and blurred previews.

use super::*;
use crate::ui::render_decode::{OffRender, made_or_start};
use quill::lang::{keys, tr};
use quill::telegram::envelope::MessageContent;

/// An autoplaying clip's picture, cropped to the media frame. A decoded
/// image rounds itself; a GPU surface (AVPlayer), which GPUI can't clip,
/// gets its corners painted over in the colors behind them
/// (`video_frame`'s corner mask).
pub(super) fn inline_surface(
    inline: crate::ui::inline_video::InlineFrame,
    frame_w: Pixels,
    frame_h: Pixels,
    corners: MediaCorners,
) -> AnyElement {
    let radii = Corners {
        top_left: corners.tl,
        top_right: corners.tr,
        bottom_right: corners.br,
        bottom_left: corners.bl,
    };
    div()
        .w(frame_w)
        .h(frame_h)
        .child(
            crate::ui::video_player::video_frame(&inline.player)
                .object_fit(ObjectFit::Cover)
                .corner_radii_over(radii, corners.mask_colors(inline.backdrop)),
        )
        .into_any_element()
}

/// A round video message's picture: the square video cut to a circle (an
/// image clips itself; a GPU surface gets the history's color painted
/// over the outside of the circle).
pub(super) fn round_inline_surface(inline: crate::ui::inline_video::InlineFrame) -> AnyElement {
    let diameter = px(VIDEO_NOTE_DIAMETER);
    div()
        .size(diameter)
        .child(
            crate::ui::video_player::video_frame(&inline.player)
                .object_fit(ObjectFit::Cover)
                .corner_radii(Corners::all(diameter / 2.), inline.backdrop),
        )
        .into_any_element()
}

/// An autoplaying clip's tile, `visual` of its frame, over `still` (the
/// thumbnail, shown until the player has its first picture). When the
/// clip is layered (`InlineFrame::live`), the conversation's animation
/// layer redraws the tile over the one the row rendered
/// (`anim_layer::tile`); its `video_frame` asks for its own frames, so the
/// tile carries no clock rate.
pub(super) fn live_tile(
    inline: crate::ui::inline_video::InlineFrame,
    still: Option<AnyElement>,
    (w, h): (Pixels, Pixels),
    visual: impl Fn(crate::ui::inline_video::InlineFrame) -> AnyElement + 'static,
) -> AnyElement {
    let tile = match inline.live.clone() {
        Some(live) => {
            let visual = std::rc::Rc::new(visual);
            let rebuild = visual.clone();
            crate::ui::anim_layer::tile(visual(inline), 0, move || live.frame().map(|f| rebuild(f)))
                .into_any_element()
        }
        None => visual(inline),
    };
    div()
        .relative()
        .w(w)
        .h(h)
        .children(still.map(|still| div().absolute().inset_0().child(still)))
        .child(tile)
        .into_any_element()
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
pub(in crate::ui) fn video_badge(duration: String, muted: bool) -> Div {
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
/// is one: too small to show detail, it reads as a blur. A video keeps its
/// play disc and duration over the cover (tdesktop `Gif::draw`).
pub(in crate::ui) fn spoiler_cover(
    row_id: u64,
    chat_id: ChatId,
    message_id: MessageId,
    content: &MessageContent,
    files: &quill::state::FileMap,
    downloading: &quill::state::DownloadIntents,
    (frame_w, frame_h): (Pixels, Pixels),
    corners: MediaCorners,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let key = crate::ui::spoiler_fx::media_key(chat_id, message_id);
    // A photo fetches on reveal, so the picture shows at once.
    let (minithumbnail, download) = match content {
        MessageContent::Photo(photo) => (photo.minithumbnail.as_ref(), photo.open_file_id()),
        MessageContent::Video(video) => (video.minithumbnail.as_ref(), None),
        MessageContent::Animation(animation) => (animation.minithumbnail.as_ref(), None),
        _ => (None, None),
    };
    let video_marks = match content {
        MessageContent::Video(video) => {
            let play_id = video.play_file_id().unwrap_or(FileId(0));
            let disc = if file_is_downloading(play_id, files, downloading) {
                MediaDisc::Progress(download_fraction(play_id, files))
            } else {
                MediaDisc::Play
            };
            Some([
                video_badge(format_voice_duration(video.duration), false).into_any_element(),
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(media_disc(("spoiler-disc", row_id), disc))
                    .into_any_element(),
            ])
        }
        _ => None,
    };
    spoiler_surface(row_id, minithumbnail, frame_w, frame_h, corners)
        .children(video_marks.into_iter().flatten())
        .role(gpui_kit::Role::Button)
        .aria_label(tr(keys::Q_REST_REVEAL_SPOILER))
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
        .test_support()
        .into_any_element()
}

/// `media` under the cover of its just-revealed spoiler, fading out at
/// `opacity`.
pub(in crate::ui) fn fading_spoiler(
    media: AnyElement,
    cover: AnyElement,
    opacity: f32,
) -> AnyElement {
    div()
        .relative()
        .child(media)
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .opacity(opacity)
                .child(cover),
        )
        .into_any_element()
}

/// The covered picture of a spoiler without its click: the blurred inline
/// preview (scaled up and blurred once, cached under `row_id`: the raw 40 px
/// JPEG scaled to the frame shows blocks), tdesktop's darkening and the
/// drifting speck "mess". Locked paid media wears the same surface.
pub(in crate::ui) fn spoiler_surface(
    row_id: u64,
    minithumbnail: Option<&quill::telegram::envelope::MiniThumbnail>,
    frame_w: Pixels,
    frame_h: Pixels,
    corners: MediaCorners,
) -> Stateful<Div> {
    let preview = minithumbnail
        .filter(|mini| !mini.data.is_empty())
        .map(|mini| {
            img(blurred_preview(row_id, mini.data.clone()))
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
}

thread_local! {
    static BLURRED: std::cell::RefCell<OffRender<(u64, u64), Arc<RenderImage>>> = std::cell::RefCell::new(
        OffRender::new(256, |old| crate::ui::image_budget::retire_all([old])),
    );
}

/// The minithumbnail scaled up and Gaussian-blurred off the UI thread,
/// cached per message and minithumbnail: an edit that replaces the media
/// gets a new one.
fn blurred_preview(row_id: u64, jpeg: Arc<[u8]>) -> ImageSource {
    let key = (row_id, content_hash(&jpeg));
    ImageSource::Custom(Arc::new(move |window, cx| {
        let jpeg = jpeg.clone();
        made_or_start(&BLURRED, key, move || blur(&jpeg), window, cx).map(Ok)
    }))
}

fn content_hash(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

fn blur(jpeg: &[u8]) -> Option<Arc<RenderImage>> {
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
    Some(Arc::new(RenderImage::new(smallvec::SmallVec::from_buf([
        image::Frame::new(blurred),
    ]))))
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod tests {
    use super::{BLURRED, blurred_preview, content_hash};
    use gpui_kit::{
        Context, IntoElement, ParentElement, Render, Styled, TestAppContext, Window, div, img, px,
    };
    use std::sync::Arc;

    struct Spoiler(Arc<[u8]>);

    impl Render for Spoiler {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size(px(20.))
                .child(img(blurred_preview(7, self.0.clone())).size(px(10.)))
        }
    }

    fn jpeg() -> Arc<[u8]> {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::RgbImage::from_pixel(4, 3, image::Rgb([200, 40, 40]))
            .write_to(&mut bytes, image::ImageFormat::Jpeg)
            .expect("encoded");
        bytes.into_inner().into()
    }

    /// A message whose minithumbnail could not be blurred gets a preview
    /// once an edit brings a new one: the failure is kept for the old
    /// bytes only.
    #[gpui_kit::test]
    fn a_new_minithumbnail_is_blurred_after_a_failed_one(cx: &mut TestAppContext) {
        let broken: Arc<[u8]> = vec![1, 2, 3].into();
        let (view, vcx) = cx.add_window_view(|_, _| Spoiler(broken.clone()));
        vcx.update(|window, cx| window.draw(cx).clear(cx));
        vcx.run_until_parked();
        let made = |bytes: &Arc<[u8]>| {
            let key = (7, content_hash(bytes));
            BLURRED.with_borrow_mut(|cache| cache.made(&key))
        };
        assert_eq!(made(&broken), Some(false));
        let edited = jpeg();
        view.update(vcx, |view, cx| {
            view.0 = edited.clone();
            cx.notify();
        });
        vcx.update(|window, cx| window.draw(cx).clear(cx));
        vcx.run_until_parked();
        assert_eq!(made(&edited), Some(true));
    }
}
