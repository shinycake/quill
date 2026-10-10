//! Media sizing and shape: bubble widths, frames, corner radii and the
//! download/play disc.

use super::*;

/// Display frame for a photo/GIF/video in a bubble: the media's aspect
/// ratio fitted into at most 360×400 (and at least 120 on the short
/// side), so pictures are never cropped to a fixed strip and the
/// not-yet-downloaded placeholder already has the final size — the row
/// keeps its height when the file arrives.
pub(in crate::ui) fn media_frame(width: i32, height: i32) -> (Pixels, Pixels) {
    const MAX_W: f32 = 360.;
    const MAX_H: f32 = 400.;
    const MIN_SIDE: f32 = 120.;
    if width <= 0 || height <= 0 {
        return (px(260.), px(180.));
    }
    let (w, h) = (width as f32, height as f32);
    let scale = (MAX_W / w).min(MAX_H / h);
    let (w, h) = (w * scale, h * scale);
    // Very wide/tall media: keep a usable short side (cropped by Cover).
    (px(w.max(MIN_SIDE)), px(h.max(MIN_SIDE)))
}

/// Smallest content width of a media bubble (`historyPhotoBubbleMinWidth`).
const MEDIA_BUBBLE_MIN_WIDTH: f32 = 100.;

/// Content width of a bubble led by media of `media` width: the media
/// decides it (Telegram Desktop `Photo::countCurrentSize`), never less
/// than the bubble minimum or the width the footer needs (`footer_min`,
/// `minWidthForMedia`). Captions and reactions wrap to this width.
pub(in crate::ui) fn media_content_width(media: Pixels, footer_min: Pixels) -> Pixels {
    media.max(footer_min).max(px(MEDIA_BUBBLE_MIN_WIDTH))
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

/// Display width of a single photo / video / GIF, `None` for content that
/// doesn't lead with such media.
pub(in crate::ui) fn single_media_width(
    content: &quill::telegram::envelope::MessageContent,
) -> Option<Pixels> {
    use quill::telegram::envelope::MessageContent;
    let (w, _) = match content {
        MessageContent::Photo(photo) => photo
            .largest_size()
            .or_else(|| photo.thumb_size())
            .map(|size| media_frame(size.width, size.height))
            .unwrap_or_else(|| media_frame(0, 0)),
        MessageContent::Video(video) => media_frame(video.width, video.height),
        MessageContent::Animation(animation) => media_frame(animation.width, animation.height),
        _ => return None,
    };
    Some(w)
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
        MediaDisc::Progress(fraction) => ProgressCircle::new("media-disc-ring")
            .size(px(34.))
            .color(gpui_kit::white())
            .loading(fraction.is_none())
            .value(fraction.unwrap_or(0.) * 100.)
            .accessibility_label("Downloading")
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
pub(super) fn download_fraction(file_id: FileId, files: &HashMap<i32, ParsedFile>) -> Option<f32> {
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
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(super) fn mask_colors(self, backdrop: Hsla) -> [Hsla; 4] {
        let small = px(MEDIA_SMALL_RADIUS);
        [self.tl, self.tr, self.br, self.bl].map(|radius| match self.behind {
            Some(bubble) if radius <= small => bubble,
            _ => backdrop,
        })
    }

    #[cfg(any(target_os = "macos", test))]
    /// The corner radii in whole points, `[tl, tr, br, bl]`.
    pub(super) fn radii(self) -> [u32; 4] {
        [self.tl, self.tr, self.br, self.bl].map(|r| f32::from(r).round().max(0.) as u32)
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
    use super::{bubble_outer_width, media_content_width};
    use gpui_kit::px;

    #[test]
    fn media_decides_the_bubble_width() {
        // A wide photo is never widened by its caption or footer.
        assert_eq!(media_content_width(px(360.), px(120.)), px(360.));
        // Tiny media keeps a usable minimum and room for the footer.
        assert_eq!(media_content_width(px(40.), px(0.)), px(100.));
        assert_eq!(media_content_width(px(100.), px(140.)), px(140.));
    }

    #[test]
    fn outer_width_adds_only_the_bubble_chrome() {
        assert_eq!(bubble_outer_width(px(360.), true, false), px(362.));
        assert_eq!(bubble_outer_width(px(360.), false, false), px(386.));
        assert_eq!(bubble_outer_width(px(360.), true, true), px(360.));
    }
}
