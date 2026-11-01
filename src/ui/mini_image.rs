//! Inline minithumbnails (the ~40 px JPEG Telegram sends with a photo or
//! document) as image sources.
//!
//! Drawn as `ImageSource::Image`, GPUI's asset cache keeps each decoded
//! thumbnail for the life of the app. Going through `image_budget` instead
//! decodes it into the bounded cache, so it is evicted with the rest once
//! it has scrolled away.

use std::sync::Arc;

use gpui_kit::{ImageSource, px};
use quill::telegram::envelope::MiniThumbnail;

use super::image_budget::{SizedSource, sized_image};

/// A minithumbnail is at most 40 px on its longer side; this only bounds
/// the decode, the picture is scaled to its frame when drawn.
const DECODE_EDGE: f32 = 48.;

/// `mini` as an image source in the bounded cache.
pub(super) fn mini_image(mini: &MiniThumbnail) -> ImageSource {
    sized_image(SizedSource::Mini(Arc::new(mini.clone())), px(DECODE_EDGE))
}

#[cfg(test)]
mod tests {
    use super::mini_image;
    use gpui_kit::ImageSource;
    use quill::telegram::envelope::MiniThumbnail;

    #[test]
    fn minithumbnail_goes_through_the_bounded_cache() {
        let mini = MiniThumbnail {
            width: 2,
            height: 2,
            data: vec![1, 2, 3].into(),
        };
        // `Image` sources land in GPUI's never-evicting asset cache.
        assert!(!matches!(mini_image(&mini), ImageSource::Image(_)));
        assert!(matches!(mini_image(&mini), ImageSource::Custom(_)));
    }
}
