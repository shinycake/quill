//! The stream picture's images: which frame is shown, which one is being
//! converted, and which image can leave memory.
//!
//! A stream changes picture up to 30 times a second, each a full RGBA
//! frame (3.5 MiB at 1280 × 720). The window keeps two images: the one it
//! shows and the one before it, which the last drawn frame may still be
//! using. When a third arrives the oldest is handed back to the caller,
//! which drops it from every window's atlas at once (`App::drop_image`).
//! Nothing waits for the main window's sweep, which does not run while
//! that window is minimized or in the background.
//!
//! Converting a frame (copying it and swapping red and blue for GPUI) runs
//! off the UI thread; one conversion runs at a time, and a frame that
//! arrives meanwhile is picked up on the next draw.
#![deny(clippy::indexing_slicing)]

use gpui_kit::{RenderImage, Task};
use quill::calls::engine::VideoFrame;
use std::sync::Arc;

#[derive(Default)]
pub(crate) struct StreamPicture {
    /// The image shown: chat, frame sequence, image.
    shown: Option<(i64, u64, Arc<RenderImage>)>,
    /// The image shown before it.
    previous: Option<Arc<RenderImage>>,
    /// The frame being converted.
    converting: Option<(i64, u64)>,
    /// A frame that could not be converted: not copied again every draw.
    failed: Option<(i64, u64)>,
    task: Option<Task<()>>,
    /// Every image made, to check that old ones are freed.
    #[cfg(test)]
    pub(crate) made: Vec<std::sync::Weak<RenderImage>>,
}

impl StreamPicture {
    /// The image to draw for `chat`.
    pub(crate) fn current(&self, chat: i64) -> Option<Arc<RenderImage>> {
        self.shown
            .as_ref()
            .filter(|(shown_chat, ..)| *shown_chat == chat)
            .map(|(.., image)| image.clone())
    }

    /// Whether frame `seq` of `chat` still needs converting.
    pub(crate) fn wants(&self, chat: i64, seq: u64) -> bool {
        self.converting.is_none()
            && self.failed != Some((chat, seq))
            && self
                .shown
                .as_ref()
                .is_none_or(|(shown_chat, shown_seq, _)| (*shown_chat, *shown_seq) != (chat, seq))
    }

    /// A conversion of frame `seq` started.
    pub(crate) fn start(&mut self, chat: i64, seq: u64, task: Task<()>) {
        self.converting = Some((chat, seq));
        self.task = Some(task);
    }

    /// The conversion finished. Returns the image that can leave memory.
    pub(crate) fn finish(
        &mut self,
        chat: i64,
        seq: u64,
        image: Option<Arc<RenderImage>>,
    ) -> Option<Arc<RenderImage>> {
        if self.converting != Some((chat, seq)) {
            return image;
        }
        self.converting = None;
        self.task = None;
        let Some(image) = image else {
            self.failed = Some((chat, seq));
            return None;
        };
        #[cfg(test)]
        self.made.push(Arc::downgrade(&image));
        let older = self
            .shown
            .replace((chat, seq, image))
            .map(|(.., image)| image);
        std::mem::replace(&mut self.previous, older)
    }

    /// Forget everything (the call ended). Returns the images to drop.
    pub(crate) fn clear(&mut self) -> Vec<Arc<RenderImage>> {
        self.converting = None;
        self.failed = None;
        self.task = None;
        self.shown
            .take()
            .map(|(.., image)| image)
            .into_iter()
            .chain(self.previous.take())
            .collect()
    }
}

/// A stream frame as a GPUI image (BGRA). One copy, made off the UI
/// thread; `None` for a malformed frame.
pub(crate) fn frame_image(frame: &VideoFrame) -> Option<Arc<RenderImage>> {
    let mut bgra = frame.rgba.clone();
    for pixel in bgra.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let image = image::RgbaImage::from_raw(u32::from(frame.width), u32::from(frame.height), bgra)?;
    Some(Arc::new(RenderImage::new(smallvec::SmallVec::from_buf([
        image::Frame::new(image),
    ]))))
}

#[cfg(test)]
mod tests {
    use super::{StreamPicture, frame_image};
    use gpui_kit::RenderImage;
    use quill::calls::engine::VideoFrame;
    use std::sync::Arc;

    fn image() -> Arc<RenderImage> {
        frame_image(&VideoFrame {
            seq: 0,
            width: 2,
            height: 1,
            rgba: vec![1, 2, 3, 4, 5, 6, 7, 8],
            is_local: false,
            participant_user_id: None,
            is_screen: false,
        })
        .expect("a 2 × 1 frame")
    }

    #[test]
    fn only_the_shown_image_and_the_one_before_stay() {
        let mut picture = StreamPicture::default();
        assert!(picture.wants(7, 1));
        picture.converting = Some((7, 1));
        assert!(!picture.wants(7, 2), "one conversion at a time");
        assert!(picture.finish(7, 1, Some(image())).is_none());
        assert!(picture.current(7).is_some());
        assert!(picture.current(8).is_none(), "another chat shows nothing");
        assert!(
            !picture.wants(7, 1),
            "the shown frame is not converted again"
        );
        picture.converting = Some((7, 2));
        assert!(picture.finish(7, 2, Some(image())).is_none());
        picture.converting = Some((7, 3));
        let freed = picture.finish(7, 3, Some(image()));
        assert!(freed.is_some(), "the third image frees the first");
        assert_eq!(picture.clear().len(), 2);
    }

    #[test]
    fn a_frame_that_cannot_be_converted_is_not_tried_again() {
        let mut picture = StreamPicture {
            converting: Some((7, 4)),
            ..Default::default()
        };
        assert!(picture.finish(7, 4, None).is_none());
        assert!(!picture.wants(7, 4), "the failed frame is skipped");
        assert!(picture.wants(7, 5), "the next frame is converted");
    }

    #[test]
    fn a_stale_conversion_is_handed_back() {
        let mut picture = StreamPicture {
            converting: Some((7, 2)),
            ..Default::default()
        };
        assert!(picture.finish(7, 1, Some(image())).is_some());
        assert!(picture.current(7).is_none());
    }

    #[test]
    fn a_frame_is_swapped_to_bgra_and_a_short_one_is_refused() {
        let image = image();
        let bytes = image.as_bytes(0).expect("frame 0");
        assert_eq!(bytes, &[3, 2, 1, 4, 7, 6, 5, 8]);
        let short = VideoFrame {
            seq: 0,
            width: 4,
            height: 4,
            rgba: vec![0; 4],
            is_local: false,
            participant_user_id: None,
            is_screen: false,
        };
        assert!(frame_image(&short).is_none());
    }
}
