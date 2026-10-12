//! A locked `messagePaidMedia` (tdesktop `HistoryView::Photo` with an
//! `extendedMediaPreview`): the blurred inline preview, an "Unlock for
//! ★N" button and, for an album, the item count. A press anywhere on the
//! card opens the Stars checkout of the message (`inputInvoiceMessage`,
//! `Payments::CheckoutProcess::Start` in `MakePaidMediaLink`). Once the
//! viewer has paid, TDLib replaces the content with a photo or video and
//! the ordinary media renderers take over (`telegram/envelope/paid_media.rs`).

use super::*;
use quill::payment_invoice::InputInvoice;
use quill::telegram::envelope::PaidMediaPreview;
use quill::wallpaper_raster::{Raster, blur};
use std::sync::{Mutex, OnceLock};

/// Longest side the preview is blurred at; the tiny JPEG (about 40 px) is
/// scaled up to this first so the blur has smooth pixels to work with.
const BLUR_SIDE: u32 = 96;
/// Blur radius in preview pixels (`Images::Blur` strength in tdesktop's
/// blurred placeholders is of the same order).
const BLUR_RADIUS: u32 = 10;
/// Blurred previews kept; the cache is dropped when it grows past this.
const CACHE_LIMIT: usize = 64;

/// The checkout a press on a locked card opens.
pub(in crate::ui) fn paid_media_invoice(chat_id: ChatId, message_id: MessageId) -> InputInvoice {
    InputInvoice::Message {
        chat_id,
        message_id,
    }
}

/// Card frame of a paid-media preview of `width` x `height` pixels: the
/// same box a photo gets (`Photo::countCurrentSize`), so the bubble keeps
/// the photo width policy.
pub(in crate::ui) fn paid_media_frame(width: i32, height: i32) -> (Pixels, Pixels) {
    media_frame_for(MediaFrameKind::Photo, width, height, 0)
}

/// "★25", the price on the unlock button (`lng_paid_price`).
pub(in crate::ui) fn star_price(stars: i64) -> String {
    format!("\u{2605}{stars}")
}

/// The preview JPEG scaled up and blurred, ready to draw; `None` when the
/// bytes do not decode.
pub(in crate::ui) fn blurred_raster(jpeg: &[u8]) -> Option<Raster> {
    let image = image::load_from_memory_with_format(jpeg, image::ImageFormat::Jpeg)
        .ok()?
        .to_rgba8();
    let (w, h) = image.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    // Scale the tiny picture up so the blur has smooth pixels to work on.
    let scale = (BLUR_SIDE as f32 / w.max(h) as f32).max(1.0);
    let target = (
        ((w as f32 * scale).round() as u32).max(1),
        ((h as f32 * scale).round() as u32).max(1),
    );
    let image = image::imageops::resize(
        &image,
        target.0,
        target.1,
        image::imageops::FilterType::Triangle,
    );
    let (w, h) = image.dimensions();
    let mut raster = Raster::from_rgba(w, h, image.into_raw());
    blur(&mut raster, BLUR_RADIUS);
    Some(raster)
}

fn cache() -> &'static Mutex<std::collections::HashMap<u64, Option<Arc<RenderImage>>>> {
    static CACHE: OnceLock<Mutex<std::collections::HashMap<u64, Option<Arc<RenderImage>>>>> =
        OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// The blurred preview as a GPUI image, decoded once per distinct preview.
fn blurred_preview(jpeg: &[u8]) -> Option<Arc<RenderImage>> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    jpeg.hash(&mut hasher);
    let key = hasher.finish();
    let mut cache = cache().lock().ok()?;
    if let Some(found) = cache.get(&key) {
        return found.clone();
    }
    if cache.len() >= CACHE_LIMIT {
        cache.clear();
    }
    let image = blurred_raster(jpeg).and_then(|raster| {
        // GPUI's sprites are BGRA, which is how a `Raster` is stored.
        let buffer = image::RgbaImage::from_raw(raster.width, raster.height, raster.bgra)?;
        Some(Arc::new(RenderImage::new(smallvec::SmallVec::from_buf([
            image::Frame::new(buffer),
        ]))))
    });
    cache.insert(key, image.clone());
    image
}

/// `messagePaidMedia` while still locked.
pub(in crate::ui) fn paid_media_card(
    chat_id: ChatId,
    message_id: MessageId,
    stars: i64,
    locked: &[PaidMediaPreview],
    caption: &str,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row_id = message_id.0 as u64;
    let first = locked.first();
    let (width, height) =
        paid_media_frame(first.map_or(0, |p| p.width), first.map_or(0, |p| p.height));
    let mut tile = div()
        .id(("paid-media", row_id))
        .relative()
        .w(width)
        .h(height)
        .overflow_hidden()
        .rounded_md()
        .bg(fill_muted())
        .role(gpui_kit::Role::Button)
        .aria_label(format!("Unlock for {stars} Stars"))
        .tab_index(0)
        .cursor_pointer()
        .on_click(cx.listener(move |this, _, window, cx| {
            this.start_invoice_checkout(paid_media_invoice(chat_id, message_id), window, cx);
        }));
    if let Some(preview) = first
        .and_then(|p| p.minithumbnail.as_ref())
        .filter(|mini| !mini.data.is_empty())
        .and_then(|mini| blurred_preview(&mini.data))
    {
        tile = tile.child(
            img(ImageSource::Render(preview))
                .size_full()
                .object_fit(ObjectFit::Cover),
        );
    }
    // Over the picture the colors are fixed (they sit on media, not on
    // the theme), like the video play badge and time pill.
    let pill = |child: AnyElement| {
        div()
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .py_0p5()
            .rounded_full()
            .bg(gpui_kit::black().opacity(0.55))
            .text_color(gpui_kit::white())
            .text_xs()
            .child(child)
    };
    tile = tile
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .size(px(44.))
                                .rounded_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .bg(gpui_kit::black().opacity(0.55))
                                .text_color(gpui_kit::white())
                                .child(Icon::new(gpui_kit::assets::IconName::Lock)),
                        )
                        .child(
                            div()
                                .px_3()
                                .py_1()
                                .rounded_full()
                                .bg(gpui_kit::black().opacity(0.6))
                                .text_color(gpui_kit::white())
                                .text_sm()
                                .font_medium()
                                .child(format!("Unlock for {}", star_price(stars))),
                        ),
                ),
        )
        .when(locked.len() > 1, |this| {
            this.child(
                div()
                    .absolute()
                    .top_2()
                    .right_2()
                    .child(pill(format!("{} items", locked.len()).into_any_element())),
            )
        });
    if let Some(duration) = first.map(|p| p.duration).filter(|d| *d > 0) {
        tile = tile.child(
            div()
                .absolute()
                .top_2()
                .left_2()
                .child(pill(format_voice_duration(duration).into_any_element())),
        );
    }
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(tile)
        .when(!caption.is_empty(), |this| {
            this.child(div().text_sm().child(caption.to_string()))
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{InputInvoice, blurred_raster, paid_media_frame, paid_media_invoice, star_price};
    use quill::ids::{ChatId, MessageId};

    fn tiny_jpeg() -> Vec<u8> {
        // A 8x6 checkerboard: strongly varying, so a blur is measurable.
        let image = image::RgbImage::from_fn(8, 6, |x, y| {
            if (x + y) % 2 == 0 {
                image::Rgb([250, 250, 250])
            } else {
                image::Rgb([5, 5, 5])
            }
        });
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgb8(image)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Jpeg,
            )
            .expect("encode");
        bytes
    }

    #[test]
    fn the_unlock_press_asks_for_the_message_invoice() {
        let invoice = paid_media_invoice(ChatId(-100), MessageId(42));
        assert_eq!(
            invoice,
            InputInvoice::Message {
                chat_id: ChatId(-100),
                message_id: MessageId(42),
            }
        );
        assert_eq!(invoice.to_json()["@type"], "inputInvoiceMessage");
        assert_eq!(invoice.to_json()["message_id"], 42);
    }

    #[test]
    fn the_price_is_a_star_and_the_count() {
        assert_eq!(star_price(25), "\u{2605}25");
        assert_eq!(star_price(1), "\u{2605}1");
    }

    #[test]
    fn the_card_keeps_the_photo_frame_policy() {
        use quill::bubble_layout::{MAX_MEDIA_SIZE, Size, photo_current};
        let context = crate::ui::history::bubble_width::media_context(0, 0);
        for (w, h) in [(800, 600), (600, 800), (300, 300), (0, 0)] {
            let (fw, fh) = paid_media_frame(w, h);
            let expected = photo_current(Size::new(w, h), context, MAX_MEDIA_SIZE);
            assert_eq!(f32::from(fw), expected.w as f32);
            assert_eq!(f32::from(fh), expected.h as f32);
        }
    }

    #[test]
    fn the_preview_is_scaled_up_and_blurred() {
        let raster = blurred_raster(&tiny_jpeg()).expect("decodes");
        assert!(raster.width >= 8 * 8 && raster.height >= 6 * 8);
        // The checkerboard's neighbouring pixels differ by ~245; after the
        // blur they are close.
        let a = raster.pixel(raster.width / 2, raster.height / 2)[0];
        let b = raster.pixel(raster.width / 2 + 1, raster.height / 2)[0];
        assert!(a.abs_diff(b) < 40, "{a} vs {b}");
        assert!(blurred_raster(b"not a jpeg").is_none());
    }
}
