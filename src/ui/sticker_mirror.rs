//! Mirrored Premium stickers, like tdesktop's `Sticker::mirrorHorizontal`.
//!
//! tdesktop flips the sticker bitmap itself when the sticker has a Premium
//! effect and its message is not right-aligned, so the sticker faces the
//! same way as its (mirrored) effect. Here the decoded frames are flipped
//! once, kept by the identity of the decoded frames, and drawn from then
//! on without further work.

use std::cell::RefCell;
use std::sync::Arc;

use gpui_kit::RenderImage;

use super::anim_layer::LayeredClip;
use super::emoji_effect::flip_frames;
use super::sticker_playback::AnimatedVisual;

/// Flipped sets kept: frames are large, so only what is on screen.
const KEPT: usize = 16;

thread_local! {
    /// (source frames, flipped frames), most recently used last.
    static FLIPPED: RefCell<Vec<(Arc<[Arc<RenderImage>]>, Arc<[Arc<RenderImage>]>)>> =
        const { RefCell::new(Vec::new()) };
}

/// `tdesktop Sticker::mirrorHorizontal`: a Premium sticker (one with a
/// Premium effect) that is not right-aligned.
pub(super) fn mirrors(has_premium_effect: bool, right_aligned: bool) -> bool {
    has_premium_effect && !right_aligned
}

fn flipped_set(frames: &Arc<[Arc<RenderImage>]>) -> Arc<[Arc<RenderImage>]> {
    let found = FLIPPED.with(|cache| {
        let mut cache = cache.borrow_mut();
        let at = cache.iter().position(|(src, _)| Arc::ptr_eq(src, frames))?;
        let entry = cache.remove(at);
        let flipped = entry.1.clone();
        cache.push(entry);
        Some(flipped)
    });
    if let Some(found) = found {
        return found;
    }
    let flipped = flip_frames(frames);
    FLIPPED.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= KEPT {
            let (_, old) = cache.remove(0);
            super::image_budget::retire_all(old.iter().cloned());
        }
        cache.push((frames.clone(), flipped.clone()));
    });
    flipped
}

/// `visual` flipped left to right. A clip is flipped as a whole; a single
/// image (the path without an animation layer) is looked up by identity,
/// so each frame is flipped once.
pub(super) fn mirrored(visual: AnimatedVisual) -> AnimatedVisual {
    match visual {
        AnimatedVisual::Layered(clip) => AnimatedVisual::Layered(LayeredClip {
            frames: flipped_set(&clip.frames),
            ..clip
        }),
        AnimatedVisual::Image(image) => {
            let set: Arc<[Arc<RenderImage>]> = Arc::from(vec![image]);
            // Keyed by the image itself, not the throwaway slice.
            let flipped = FLIPPED.with(|cache| {
                cache
                    .borrow()
                    .iter()
                    .find(|(src, _)| src.len() == 1 && Arc::ptr_eq(&src[0], &set[0]))
                    .map(|(_, f)| f.clone())
            });
            let flipped = flipped.unwrap_or_else(|| flipped_set(&set));
            AnimatedVisual::Image(flipped[0].clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{mirrored, mirrors};
    use crate::ui::sticker_playback::AnimatedVisual;
    use gpui_kit::RenderImage;
    use std::sync::Arc;

    fn image(pixels: [u8; 8]) -> Arc<RenderImage> {
        let rgba = image::RgbaImage::from_raw(2, 1, pixels.to_vec()).unwrap();
        Arc::new(RenderImage::new(smallvec::SmallVec::from_buf([
            image::Frame::new(rgba),
        ])))
    }

    #[test]
    fn only_a_premium_sticker_off_the_right_is_mirrored() {
        assert!(mirrors(true, false));
        assert!(!mirrors(true, true));
        assert!(!mirrors(false, false));
        assert!(!mirrors(false, true));
    }

    #[test]
    fn mirrored_image_is_flipped_and_reused() {
        let src = image([1, 1, 1, 255, 2, 2, 2, 255]);
        let AnimatedVisual::Image(a) = mirrored(AnimatedVisual::Image(src.clone())) else {
            panic!("image stays an image");
        };
        assert_eq!(a.as_bytes(0).unwrap(), &[2, 2, 2, 255, 1, 1, 1, 255]);
        let AnimatedVisual::Image(b) = mirrored(AnimatedVisual::Image(src)) else {
            panic!("image stays an image");
        };
        assert!(Arc::ptr_eq(&a, &b), "flipped once, then reused");
    }
}
