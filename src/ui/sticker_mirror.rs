//! Mirrored Premium stickers, like tdesktop's `Sticker::mirrorHorizontal`.
//!
//! tdesktop flips the sticker bitmap itself when the sticker has a Premium
//! effect and its message is not right-aligned, so the sticker faces the
//! same way as its (mirrored) effect. Here the decoded frames are flipped
//! once, kept by the identity of the decoded frames, and drawn from then
//! on without further work.
#![deny(clippy::indexing_slicing)]

use std::cell::RefCell;
use std::sync::{Arc, Weak};

use gpui_kit::RenderImage;

use super::anim_layer::LayeredClip;
use super::emoji_effect::flip_frames;
use super::sticker_playback::{AnimatedVisual, PlaybackSize};

/// Flipped sets kept: frames are large, so only what is on screen.
const KEPT: usize = 16;

/// What a flipped set was made from. Held weakly: the mirror cache must
/// not keep the source frames alive after the sticker playback cache has
/// evicted them.
enum Source {
    Clip(Weak<[Arc<RenderImage>]>),
    Image(Weak<RenderImage>),
}

impl Source {
    fn alive(&self) -> bool {
        match self {
            Self::Clip(w) => w.strong_count() > 0,
            Self::Image(w) => w.strong_count() > 0,
        }
    }

    fn is_clip(&self, frames: &Arc<[Arc<RenderImage>]>) -> bool {
        matches!(self, Self::Clip(w) if std::ptr::addr_eq(w.as_ptr(), Arc::as_ptr(frames)))
    }

    fn is_image(&self, image: &Arc<RenderImage>) -> bool {
        matches!(self, Self::Image(w) if std::ptr::addr_eq(w.as_ptr(), Arc::as_ptr(image)))
    }
}

struct Entry {
    source: Source,
    flipped: Arc<[Arc<RenderImage>]>,
    bytes: usize,
}

thread_local! {
    /// Flipped sets, most recently used last.
    static FLIPPED: RefCell<Vec<Entry>> = const { RefCell::new(Vec::new()) };
}

/// `tdesktop Sticker::mirrorHorizontal`: a Premium sticker (one with a
/// Premium effect) that is not right-aligned.
pub(super) fn mirrors(has_premium_effect: bool, right_aligned: bool) -> bool {
    has_premium_effect && !right_aligned
}

/// Decoded size of `frames`.
fn decoded_bytes(frames: &[Arc<RenderImage>]) -> usize {
    frames
        .iter()
        .map(|frame| {
            let size = frame.size(0);
            (size.width.0.max(0) as usize)
                .saturating_mul(size.height.0.max(0) as usize)
                .saturating_mul(4)
        })
        .fold(0, usize::saturating_add)
}

/// Make room for `adding` bytes: drop sets whose source is gone, then the
/// least recently used, until the count and the sticker playback byte
/// budget hold. A set bigger than the budget stands alone.
fn make_room(cache: &mut Vec<Entry>, adding: usize, count: usize, budget: usize) {
    let mut at = 0;
    while at < cache.len() {
        if cache.get(at).is_some_and(|e| e.source.alive()) {
            at += 1;
        } else {
            let old = cache.remove(at);
            super::image_budget::retire_all(old.flipped.iter().cloned());
        }
    }
    while !cache.is_empty()
        && (cache.len() >= count
            || cache
                .iter()
                .fold(0_usize, |sum, e| sum.saturating_add(e.bytes))
                .saturating_add(adding)
                > budget)
    {
        let old = cache.remove(0);
        super::image_budget::retire_all(old.flipped.iter().cloned());
    }
}

/// The cached flipped set whose source `is`, moved to the most recent.
fn take_cached(is: impl Fn(&Source) -> bool) -> Option<Arc<[Arc<RenderImage>]>> {
    FLIPPED.with(|cache| {
        let mut cache = cache.borrow_mut();
        let at = cache.iter().position(|e| is(&e.source))?;
        let entry = cache.remove(at);
        let flipped = entry.flipped.clone();
        cache.push(entry);
        Some(flipped)
    })
}

fn insert(source: Source, flipped: &Arc<[Arc<RenderImage>]>) {
    let bytes = decoded_bytes(flipped);
    FLIPPED.with(|cache| {
        let mut cache = cache.borrow_mut();
        make_room(
            &mut cache,
            bytes,
            KEPT,
            PlaybackSize::Sticker.budget_bytes(),
        );
        cache.push(Entry {
            source,
            flipped: flipped.clone(),
            bytes,
        });
    });
}

fn flipped_set(frames: &Arc<[Arc<RenderImage>]>) -> Arc<[Arc<RenderImage>]> {
    if let Some(found) = take_cached(|source| source.is_clip(frames)) {
        return found;
    }
    let flipped = flip_frames(frames);
    insert(Source::Clip(Arc::downgrade(frames)), &flipped);
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
            let flipped = take_cached(|source| source.is_image(&image)).unwrap_or_else(|| {
                let flipped = flip_frames(std::slice::from_ref(&image));
                insert(Source::Image(Arc::downgrade(&image)), &flipped);
                flipped
            });
            AnimatedVisual::Image(flipped.first().cloned().unwrap_or(image))
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

    fn entry(source: &Arc<RenderImage>, bytes: usize) -> super::Entry {
        super::Entry {
            source: super::Source::Image(Arc::downgrade(source)),
            flipped: Arc::from(vec![image([0; 8])]),
            bytes,
        }
    }

    #[test]
    fn oldest_sets_leave_when_bytes_pass_the_budget() {
        let live = image([0; 8]);
        let mut cache = vec![entry(&live, 40), entry(&live, 40)];
        super::make_room(&mut cache, 30, 16, 100);
        assert_eq!(cache.len(), 1, "40 + 40 + 30 > 100 drops the oldest");
        super::make_room(&mut cache, 500, 16, 100);
        assert!(
            cache.is_empty(),
            "a set bigger than the budget stands alone"
        );
    }

    #[test]
    fn sets_whose_source_is_gone_leave_and_do_not_pin_the_source() {
        let gone = image([0; 8]);
        let live = image([0; 8]);
        let mut cache = vec![entry(&gone, 1), entry(&live, 1)];
        drop(gone);
        super::make_room(&mut cache, 1, 16, 1000);
        assert_eq!(cache.len(), 1);
        // The cache holds its source weakly: dropping the last owner frees it.
        let src = image([0; 8]);
        let weak = Arc::downgrade(&src);
        let _held = entry(&src, 1);
        drop(src);
        assert!(weak.upgrade().is_none());
    }
}
