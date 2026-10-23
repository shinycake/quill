//! `messagePaidMedia` (tdesktop `HistoryView::Photo` / `GroupedMedia` with an
//! `extendedMediaPreview`): while locked, the blurred inline preview under
//! the spoiler specks with an "Unlock for ★N" button; an album shows every
//! preview in Quill's album layout. A press anywhere on the card opens the
//! Stars checkout of the message (`inputInvoiceMessage`,
//! `Payments::CheckoutProcess::Start` in `MakePaidMediaLink`). Once the
//! viewer has paid, TDLib replaces the content: one item becomes a photo or
//! video message, several stay here as a bought album drawn with the album
//! tiles (`telegram/envelope/paid_media.rs`).

use super::surfaces::spoiler_surface;
use super::*;
use quill::album::{
    ALBUM_MAX_WIDTH, ALBUM_MIN_WIDTH, ALBUM_SPACING, AlbumRect, album_pixel_size, layout_bounds,
    layout_media_group,
};
use quill::lang::{keys, tr_with};
use quill::payment_invoice::InputInvoice;
use quill::state::HistoryMessage;
use quill::telegram::envelope::{MessageContent, PaidMediaPreview};

/// The checkout a press on a locked card opens.
pub(in crate::ui) fn paid_media_invoice(chat_id: ChatId, message_id: MessageId) -> InputInvoice {
    InputInvoice::Message {
        chat_id,
        message_id,
    }
}

/// Card frame of a single paid-media preview of `width` x `height` pixels:
/// the same box a photo gets (`Photo::countCurrentSize`), so the bubble keeps
/// the photo width policy, in a bubble that leaves it `width_max`.
pub(in crate::ui) fn paid_media_frame(width: i32, height: i32, width_max: i32) -> (Pixels, Pixels) {
    media_frame_for(MediaFrameKind::Photo, width, height, 0, width_max)
}

/// "★25", the price on the unlock button (`lng_paid_price`).
pub(in crate::ui) fn star_price(stars: i64) -> String {
    format!("\u{2605}{stars}")
}

/// Album layout of several previews or bought items, and its box, scaled
/// down to a bubble that leaves it `width_max`.
pub(in crate::ui) fn paid_album_layout(
    sizes: &[(i32, i32)],
    width_max: i32,
) -> (Vec<AlbumRect>, (i32, i32)) {
    let layout = layout_media_group(sizes, ALBUM_MAX_WIDTH, ALBUM_MIN_WIDTH, ALBUM_SPACING);
    let layout = quill::album::scale_layout(&layout, width_max, ALBUM_SPACING);
    let bounds = layout_bounds(&layout);
    (layout, bounds)
}

/// Fixed colors over a picture (they sit on media, not on the theme), like
/// the video play badge and time pill.
fn media_pill(child: AnyElement) -> Div {
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
}

/// The lock and the "Unlock for ★N" pill, centered on the card.
fn unlock_overlay(stars: i64) -> Div {
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
                        .child(tr_with(
                            keys::PAID_PRICE,
                            &[("price", &(star_price(stars)).to_string())],
                        )),
                ),
        )
}

/// `messagePaidMedia` while still locked.
pub(in crate::ui) fn paid_media_card(
    chat_id: ChatId,
    message_id: MessageId,
    stars: i64,
    locked: &[PaidMediaPreview],
    caption: &str,
    width_max: i32,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row_id = (message_id.0 as u64).wrapping_mul(64);
    let sizes: Vec<(i32, i32)> = locked.iter().map(|p| (p.width, p.height)).collect();
    let (frames, (width, height)): (Vec<AlbumRect>, (Pixels, Pixels)) = if locked.len() > 1 {
        let (layout, (w, h)) = paid_album_layout(
            &sizes
                .iter()
                .map(|&(w, h)| (w.max(1), h.max(1)))
                .collect::<Vec<_>>(),
            width_max,
        );
        (layout, (px(w as f32), px(h as f32)))
    } else {
        let (w, h) = paid_media_frame(
            sizes.first().map_or(0, |s| s.0),
            sizes.first().map_or(0, |s| s.1),
            width_max,
        );
        (
            vec![AlbumRect {
                x: 0,
                y: 0,
                width: f32::from(w) as i32,
                height: f32::from(h) as i32,
            }],
            (w, h),
        )
    };
    let corners = MediaCorners::small();
    let mut tile = div()
        .id(("paid-media", row_id))
        .relative()
        .w(width)
        .h(height)
        .overflow_hidden()
        .rounded_md()
        .bg(fill_muted())
        .role(gpui_kit::Role::Button)
        .aria_label(tr_with(
            keys::Q_MONEY_UNLOCK_FOR_STARS,
            &[("stars", &stars.to_string())],
        ))
        .tab_index(0)
        .cursor_pointer()
        .on_click(cx.listener(move |this, _, window, cx| {
            this.start_invoice_checkout(paid_media_invoice(chat_id, message_id), window, cx);
        }));
    for (index, (preview, rect)) in locked.iter().zip(&frames).enumerate() {
        let surface = spoiler_surface(
            row_id + index as u64,
            preview.minithumbnail.as_ref(),
            px(rect.width as f32),
            px(rect.height as f32),
            corners,
        );
        let mut part = div()
            .absolute()
            .left(px(rect.x as f32))
            .top(px(rect.y as f32))
            .w(px(rect.width as f32))
            .h(px(rect.height as f32))
            .child(surface);
        if preview.duration > 0 {
            part = part.child(div().absolute().top_2().left_2().child(media_pill(
                format_voice_duration(preview.duration).into_any_element(),
            )));
        }
        tile = tile.child(part);
    }
    tile = tile.child(unlock_overlay(stars));
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

/// A bought paid-media album: its photos and videos in the album layout,
/// each opening the viewer on its own item.
pub(in crate::ui) fn paid_album_card(
    message: &HistoryMessage,
    unlocked: &[MessageContent],
    caption: &str,
    files: &quill::state::FileMap,
    downloading: &quill::state::DownloadIntents,
    media_roots: &[PathBuf],
    width_max: i32,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let sizes: Vec<(i32, i32)> = unlocked.iter().map(album_pixel_size).collect();
    let (layout, (box_w, box_h)) = paid_album_layout(&sizes, width_max);
    let mut mosaic = div()
        .id(("paid-album", message.id.0 as u64))
        .relative()
        .w(px(box_w as f32))
        .h(px(box_h as f32))
        .overflow_hidden();
    for (index, (content, part)) in unlocked.iter().zip(&layout).enumerate() {
        let mut item = message.clone();
        item.content = content.clone();
        mosaic = mosaic.child(crate::ui::history::album_tile(
            &item,
            part,
            (box_w, box_h),
            MediaCorners::small(),
            files,
            downloading,
            media_roots,
            index,
            None,
            // Bought items carry no spoiler.
            None,
            cx,
        ));
    }
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(mosaic)
        .when(!caption.is_empty(), |this| {
            this.child(div().text_sm().child(caption.to_string()))
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{
        InputInvoice, paid_album_layout, paid_media_frame, paid_media_invoice, star_price,
    };
    use quill::ids::{ChatId, MessageId};

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
            let (fw, fh) = paid_media_frame(w, h, MAX_MEDIA_SIZE);
            let expected = photo_current(Size::new(w, h), context, MAX_MEDIA_SIZE);
            assert_eq!(f32::from(fw), expected.w as f32);
            assert_eq!(f32::from(fh), expected.h as f32);
        }
    }

    #[test]
    fn an_album_lays_out_every_item_inside_one_box() {
        let sizes = [(800, 600), (600, 800), (400, 400)];
        let (layout, (w, h)) = paid_album_layout(&sizes, 430);
        assert_eq!(layout.len(), 3);
        assert!(w > 0 && h > 0);
        for part in &layout {
            assert!(part.width > 0 && part.height > 0);
            assert!(part.x >= 0 && part.y >= 0);
            assert!(part.x + part.width <= w && part.y + part.height <= h);
        }
    }

    #[test]
    fn a_narrow_bubble_scales_the_card_down() {
        let sizes = [(800, 600), (600, 800), (400, 400)];
        let (_, (wide_w, wide_h)) = paid_album_layout(&sizes, 430);
        let (layout, (w, h)) = paid_album_layout(&sizes, 250);
        assert!(w <= 250 && w < wide_w && h < wide_h, "{w}x{h}");
        assert!(layout.iter().all(|part| part.x + part.width <= w));
        let (fw, _) = paid_media_frame(800, 600, 250);
        assert_eq!(f32::from(fw), 250.);
    }
}
