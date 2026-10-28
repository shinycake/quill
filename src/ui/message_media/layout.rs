//! Media sizing and shape: bubble widths, frames, corner radii and the
//! download/play disc.

use super::*;

/// What kind of picture a frame holds; each has its own box in Telegram
/// Desktop (`maxMediaSize`, `maxGifSize`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum MediaFrameKind {
    Photo,
    Video,
    Gif,
}

/// Display frame for a photo, video or GIF in a bubble, as Telegram
/// Desktop sizes it (`quill::bubble_layout::photo_current` /
/// `clip_current` at the bubble's widest): fitted into 430x430 (GIFs 320),
/// never upscaled on its own, at least 200 wide in a bubble and 100 on a
/// side, a tall picture cropped to a square unless the crop would lose over
/// a quarter. A caption wider than the picture widens it up to
/// `msgMaxWidth` (`caption_width` is its longest line, unpadded;
/// `Photo::countCurrentSize`'s `maxWithCaption`), the picture then showing
/// more of its height. A not-yet-downloaded placeholder already has the
/// final size, so the row keeps its height when the file arrives.
/// `width_max` is the widest the bubble lets the picture be
/// (`BubbleRoom::media`, `maxMediaSize` in a wide history): a narrow
/// history scales the picture down to it, keeping its proportions
/// (`countCurrentSize` with the narrower width).
pub(in crate::ui) fn media_frame_for(
    kind: MediaFrameKind,
    width: i32,
    height: i32,
    caption_width: i32,
    width_max: i32,
) -> (Pixels, Pixels) {
    use quill::bubble_layout::{ClipKind, MAX_MEDIA_SIZE, Size, clip_current, photo_current};
    let dims = Size::new(width, height);
    let context = crate::ui::history::bubble_width::media_context(caption_width, 0);
    let new_width = width_max.clamp(1, MAX_MEDIA_SIZE);
    let size = match kind {
        MediaFrameKind::Photo => photo_current(dims, context, new_width),
        MediaFrameKind::Video => clip_current(dims, ClipKind::Video, context, 0, new_width),
        MediaFrameKind::Gif => clip_current(dims, ClipKind::Gif, context, 0, new_width),
    };
    (px(size.w as f32), px(size.h as f32))
}

/// Content width of a bubble led by media of `media` width: the media
/// decides it (Telegram Desktop `Photo::countCurrentSize`), never less
/// than `historyPhotoBubbleMinWidth` or the width the time pill needs
/// (`footer_min` plus its margins, `minWidthForMedia`), unless the
/// bubble's room (`width_max`) is narrower still. Captions and reactions
/// wrap to this width.
pub(in crate::ui) fn media_content_width(
    media: Pixels,
    footer_min: Pixels,
    width_max: i32,
) -> Pixels {
    use quill::bubble_layout::{DATE_IMG_DELTA, DATE_IMG_PADDING_X, PHOTO_BUBBLE_MIN_WIDTH};
    let pill = if footer_min > px(0.) {
        footer_min + px((2 * (DATE_IMG_DELTA + DATE_IMG_PADDING_X)) as f32)
    } else {
        px(0.)
    };
    media
        .max(pill)
        .max(px(PHOTO_BUBBLE_MIN_WIDTH as f32))
        .min(px(width_max.max(1) as f32).max(media))
}

/// Outer bubble width for `content` width: a media-led bubble keeps only
/// its 1 px border around the picture, a padded one adds the text padding
/// (`px_3`) too, the plain look has neither.
pub(in crate::ui) fn bubble_outer_width(content: Pixels, media_led: bool, plain: bool) -> Pixels {
    if plain {
        content
    } else if media_led {
        content + px(2.)
    } else {
        content + px(26.)
    }
}

/// Display width of a single photo / video / GIF with a caption of
/// `caption_width` (0 without) in a bubble that leaves it `width_max`,
/// `None` for content that doesn't lead with such media.
pub(in crate::ui) fn single_media_width(
    content: &quill::telegram::envelope::MessageContent,
    caption_width: i32,
    width_max: i32,
) -> Option<Pixels> {
    single_media_frame(content, caption_width, width_max).map(|(w, _)| w)
}

/// Display frame of a single photo / video / GIF with a caption of
/// `caption_width` (0 without) in a bubble that leaves it `width_max`,
/// `None` for content that doesn't lead with such media.
pub(in crate::ui) fn single_media_frame(
    content: &quill::telegram::envelope::MessageContent,
    caption_width: i32,
    width_max: i32,
) -> Option<(Pixels, Pixels)> {
    use quill::telegram::envelope::MessageContent;
    let frame = match content {
        MessageContent::Photo(photo) => photo
            .largest_size()
            .or_else(|| photo.thumb_size())
            .map(|size| {
                media_frame_for(
                    MediaFrameKind::Photo,
                    size.width,
                    size.height,
                    caption_width,
                    width_max,
                )
            })
            .unwrap_or_else(|| {
                media_frame_for(MediaFrameKind::Photo, 0, 0, caption_width, width_max)
            }),
        MessageContent::Video(video) => media_frame_for(
            MediaFrameKind::Video,
            video.width,
            video.height,
            caption_width,
            width_max,
        ),
        MessageContent::Animation(animation) => media_frame_for(
            MediaFrameKind::Gif,
            animation.width,
            animation.height,
            caption_width,
            width_max,
        ),
        // A picture sent as a file is framed like the photo it shows.
        MessageContent::Document(doc) if quill::media_viewer::shows_as_photo(doc) => {
            let (width, height) = doc
                .thumbnail
                .as_ref()
                .map_or((0, 0), |thumb| (thumb.width, thumb.height));
            media_frame_for(
                MediaFrameKind::Photo,
                width,
                height,
                caption_width,
                width_max,
            )
        }
        _ => return None,
    };
    Some(frame)
}

/// Content whose picture leads the bubble: a photo, video or GIF, or a
/// picture file shown as a photo.
pub(in crate::ui) fn leads_with_media(content: &quill::telegram::envelope::MessageContent) -> bool {
    use quill::telegram::envelope::MessageContent;
    match content {
        MessageContent::Photo(_) | MessageContent::Video(_) | MessageContent::Animation(_) => true,
        MessageContent::Document(doc) => quill::media_viewer::shows_as_photo(doc),
        _ => false,
    }
}

/// Hover group for a media frame: the pause disc shows only on hover
/// while the clip plays.
pub(super) const MEDIA_VISUAL_GROUP: &str = "media-visual";

/// What the round status disc centered on a photo/video/GIF frame shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) enum MediaDisc {
    Download,
    /// Downloading — the fraction once TDLib knows the total size.
    Progress(Option<f32>),
    Play,
    Pause,
}

/// Telegram-style status disc: a dark translucent circle holding a
/// download arrow, a progress ring, or play/pause.
pub(in crate::ui) fn media_disc(id: impl Into<ElementId>, state: MediaDisc) -> Stateful<Div> {
    let inner: AnyElement = match state {
        // The cross says a press cancels (tdesktop `historyFileThumbCancel`).
        MediaDisc::Progress(fraction) => div()
            .relative()
            .size(px(34.))
            .flex()
            .items_center()
            .justify_center()
            .child(
                ProgressCircle::new("media-disc-ring")
                    .absolute()
                    .inset_0()
                    .size(px(34.))
                    .color(gpui_kit::white())
                    .loading(fraction.is_none())
                    .value(fraction.unwrap_or(0.) * 100.)
                    .accessibility_label(quill::lang::tr(
                        quill::lang::keys::SR_MESSAGE_DOWNLOADING,
                    )),
            )
            .child(
                Icon::new(gpui_kit::assets::IconName::X)
                    .size(px(16.))
                    .text_color(gpui_kit::white()),
            )
            .into_any_element(),
        MediaDisc::Download | MediaDisc::Play | MediaDisc::Pause => {
            let icon = match state {
                MediaDisc::Download => gpui_kit::assets::IconName::ArrowDown,
                MediaDisc::Pause => gpui_kit::assets::IconName::Pause,
                _ => gpui_kit::assets::IconName::Play,
            };
            Icon::new(icon)
                .size(px(22.))
                .text_color(gpui_kit::white())
                .into_any_element()
        }
    };
    div()
        .id(id)
        .size(px(48.))
        .flex_none()
        .rounded_full()
        .bg(gpui_kit::black().opacity(0.5))
        .flex()
        .items_center()
        .justify_center()
        .child(inner)
}

/// Download fraction of `file_id`, when TDLib reported a total.
pub(in crate::ui) fn download_fraction(
    file_id: FileId,
    files: &quill::state::FileMap,
) -> Option<f32> {
    files
        .get(&file_id.0)
        .and_then(ParsedFile::download_progress)
}

/// Corner radii of a media tile (Telegram Desktop `BubbleRounding`): a
/// picture that fills a bubble edge takes the bubble's radius there, while a
/// corner against a caption, a header or a neighbouring album tile takes the
/// small inner radius. GPUI clips children to rectangles only, so every
/// image rounds itself and native video surfaces get a mask over them.
#[derive(Clone, Copy)]
pub(in crate::ui) struct MediaCorners {
    pub(in crate::ui) tl: Pixels,
    pub(in crate::ui) tr: Pixels,
    pub(in crate::ui) br: Pixels,
    pub(in crate::ui) bl: Pixels,
    /// What shows behind the tile's corners (the bubble's color), for the
    /// video mask; `None` uses the history's backdrop.
    pub(in crate::ui) behind: Option<Hsla>,
}

/// The small inner radius (`rounded_md`).
const MEDIA_SMALL_RADIUS: f32 = 6.;

impl MediaCorners {
    /// Every corner at the small inner radius.
    pub(in crate::ui) fn small() -> Self {
        Self::edges(px(MEDIA_SMALL_RADIUS), true, true, true, true, None)
    }

    /// `large` where the tile touches the bubble's edge and the small
    /// radius elsewhere.
    pub(in crate::ui) fn edges(
        large: Pixels,
        tl: bool,
        tr: bool,
        br: bool,
        bl: bool,
        behind: Option<Hsla>,
    ) -> Self {
        let small = px(MEDIA_SMALL_RADIUS).min(large);
        let pick = |on: bool| if on { large } else { small };
        Self {
            tl: pick(tl),
            tr: pick(tr),
            br: pick(br),
            bl: pick(bl),
            behind,
        }
    }

    /// Corners for a tile in a message bubble. A `led` tile (photo, video
    /// or GIF with no header) fills the bubble's width, so its free edges
    /// take the bubble's radius (the bubble's `radius_2xl` less its 1 px
    /// border); `top_free`/`bottom_free` say whether the sender name, a
    /// caption or other rows sit above or below it. Tiles that share the
    /// bubble with a header, and the plain (bubble-less) look, keep the
    /// small radius.
    pub(in crate::ui) fn in_bubble(
        cx: &App,
        outgoing: bool,
        plain: bool,
        led: bool,
        top_free: bool,
        bottom_free: bool,
    ) -> Self {
        if plain {
            return Self::small();
        }
        let behind = Hsla::from(if outgoing {
            accent_strong()
        } else {
            bg_bubble_incoming()
        });
        let large = if led {
            cx.theme().radius_2xl() - px(1.)
        } else {
            px(MEDIA_SMALL_RADIUS)
        };
        Self::edges(
            large,
            top_free,
            top_free,
            bottom_free,
            bottom_free,
            Some(behind),
        )
    }

    /// Corners for media drawn without a bubble (`media_bubble`): the
    /// bubble's radius everywhere, small only on the top corner that joins
    /// the sender's bubble above (Telegram Desktop `countMessageRounding`;
    /// such media skips the tail).
    pub(in crate::ui) fn bare(cx: &App, right: bool, joined_above: bool) -> Self {
        let joined = |side: bool| !(joined_above && side);
        Self::edges(
            cx.theme().radius_2xl(),
            joined(!right),
            joined(right),
            true,
            true,
            None,
        )
    }

    /// An album tile's corners: a corner keeps the mosaic's radius only where
    /// the tile touches that corner of the whole mosaic; against a neighbour
    /// it takes the small inner radius.
    pub(in crate::ui) fn tile(self, left: bool, top: bool, right: bool, bottom: bool) -> Self {
        let small = px(MEDIA_SMALL_RADIUS).min(self.tl.max(self.tr).max(self.br).max(self.bl));
        let pick = |own: Pixels, on: bool| if on { own } else { small.min(own) };
        Self {
            tl: pick(self.tl, left && top),
            tr: pick(self.tr, right && top),
            br: pick(self.br, right && bottom),
            bl: pick(self.bl, left && bottom),
            behind: self.behind,
        }
    }

    /// What shows behind each corner of a video mask, `[tl, tr, br, bl]`: a
    /// corner on the bubble's outer edge (large radius) lies mostly outside
    /// the bubble's own rounded shape, so it takes the history `backdrop`;
    /// a corner against a caption, header or tile (small radius) sits on
    /// the bubble. Bubble-less media uses the backdrop everywhere.
    pub(super) fn mask_colors(self, backdrop: Hsla) -> [Hsla; 4] {
        let small = px(MEDIA_SMALL_RADIUS);
        [self.tl, self.tr, self.br, self.bl].map(|radius| match self.behind {
            Some(bubble) if radius <= small => bubble,
            _ => backdrop,
        })
    }

    pub(in crate::ui) fn round<E: Styled>(self, element: E) -> E {
        element
            .rounded_tl(self.tl)
            .rounded_tr(self.tr)
            .rounded_br(self.br)
            .rounded_bl(self.bl)
    }
}

#[cfg(test)]
mod tests {
    use super::{MediaFrameKind, bubble_outer_width, media_content_width, media_frame_for};
    use gpui_kit::px;

    /// `maxMediaSize`: what a wide history leaves a picture.
    const WIDE: i32 = 430;

    #[test]
    fn a_narrow_bubble_scales_the_picture_down_keeping_its_shape() {
        // 1200x800 fills 430 wide in a wide history...
        assert_eq!(
            media_frame_for(MediaFrameKind::Photo, 1200, 800, 0, WIDE),
            (px(430.), px(286.))
        );
        // ...and the 250 px a 380 px history leaves a group row.
        assert_eq!(
            media_frame_for(MediaFrameKind::Photo, 1200, 800, 0, 250),
            (px(250.), px(166.))
        );
        assert_eq!(
            media_frame_for(MediaFrameKind::Video, 1280, 720, 0, 250),
            (px(250.), px(140.))
        );
        // A caption can no longer widen it past the room.
        assert_eq!(
            media_frame_for(MediaFrameKind::Photo, 300, 300, 1000, 250).0,
            px(250.)
        );
        // The bubble's floor and the time pill give way to the room too.
        assert_eq!(media_content_width(px(150.), px(0.), 180), px(180.));
        assert_eq!(media_content_width(px(250.), px(0.), 250), px(250.));
    }

    #[test]
    fn a_wide_caption_widens_the_picture_up_to_msg_max_width() {
        // Without a caption a 300x300 picture keeps its size.
        assert_eq!(
            media_frame_for(MediaFrameKind::Photo, 300, 300, 0, WIDE),
            (px(300.), px(300.))
        );
        // A caption 380 px wide (402 with padding) widens it to 402; the
        // square picture is cropped to that strip, since showing it whole
        // would mean covering more than a quarter (`adjustHeightForLessCrop`).
        assert_eq!(
            media_frame_for(MediaFrameKind::Photo, 300, 300, 380, WIDE),
            (px(402.), px(300.))
        );
        // A slightly wider caption keeps the picture whole: the cover
        // loses under a quarter, so the full height is shown.
        assert_eq!(
            media_frame_for(MediaFrameKind::Photo, 300, 300, 340, WIDE),
            (px(362.), px(362.))
        );
        // Never past msgMaxWidth.
        assert_eq!(
            media_frame_for(MediaFrameKind::Photo, 300, 300, 1000, WIDE).0,
            px(430.)
        );
        // A GIF stays inside maxGifSize whatever the caption.
        assert_eq!(
            media_frame_for(MediaFrameKind::Gif, 300, 200, 1000, WIDE).0,
            px(320.)
        );
    }

    #[test]
    fn media_decides_the_bubble_width() {
        // A wide photo is never widened by its caption or footer.
        assert_eq!(media_content_width(px(360.), px(120.), WIDE), px(360.));
        // Tiny media keeps `historyPhotoBubbleMinWidth` and room for the
        // time pill with its margins (`minWidthForMedia`).
        assert_eq!(media_content_width(px(40.), px(0.), WIDE), px(200.));
        assert_eq!(media_content_width(px(100.), px(200.), WIDE), px(224.));
    }

    #[test]
    fn outer_width_adds_only_the_bubble_chrome() {
        assert_eq!(bubble_outer_width(px(360.), true, false), px(362.));
        assert_eq!(bubble_outer_width(px(360.), false, false), px(386.));
        assert_eq!(bubble_outer_width(px(360.), true, true), px(360.));
    }
}
