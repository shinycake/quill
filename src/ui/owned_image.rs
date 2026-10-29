//! A picture Quill made itself and keeps in its own state (an edited
//! photo's preview, a rotated photo, a QR code), which leaves the GPU atlas
//! when its owner lets go of it.
//!
//! GPUI keeps a painted `Arc<RenderImage>` in the window's atlas until
//! someone calls `drop_image`, so replacing or dropping such a field left
//! its pixels on the GPU for the life of the app: a photo rotated in the
//! viewer kept every turn, up to the photo's full size each. Holding the
//! image as an [`OwnedImage`] hands it to [`image_budget::retire_all`] on
//! drop, which drops it from the atlas once no frame can still show it.
//!
//! It stays on the UI thread (it is not `Send`): retired images wait in a
//! thread-local list that only the UI thread sweeps.
//!
//! [`image_budget::retire_all`]: super::image_budget::retire_all

use gpui_kit::RenderImage;
use std::marker::PhantomData;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;

/// An image Quill owns: dropping or replacing it retires it. Not `Clone`,
/// so `.clone()` hands out the `Arc` to paint, never a second owner.
pub(super) struct OwnedImage(Arc<RenderImage>, PhantomData<Rc<()>>);

impl OwnedImage {
    pub(super) fn new(image: Arc<RenderImage>) -> Self {
        Self(image, PhantomData)
    }
}

impl From<Arc<RenderImage>> for OwnedImage {
    fn from(image: Arc<RenderImage>) -> Self {
        Self::new(image)
    }
}

impl Deref for OwnedImage {
    type Target = Arc<RenderImage>;

    fn deref(&self) -> &Arc<RenderImage> {
        &self.0
    }
}

impl Drop for OwnedImage {
    fn drop(&mut self) {
        super::image_budget::retire_all([self.0.clone()]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::image_budget::{begin_frame, take_unshowable};
    use smallvec::SmallVec;

    fn picture() -> Arc<RenderImage> {
        Arc::new(RenderImage::new(SmallVec::from_buf([image::Frame::new(
            image::RgbaImage::new(2, 2),
        )])))
    }

    /// The ids the sweep would drop from the atlas after two more frames.
    fn swept_ids() -> Vec<usize> {
        begin_frame();
        begin_frame();
        take_unshowable().iter().map(|image| image.id.0).collect()
    }

    #[test]
    fn a_replaced_image_leaves_the_atlas_and_the_new_one_stays() {
        let first = picture();
        let (first_id, first_weak) = (first.id.0, Arc::downgrade(&first));
        let mut held = OwnedImage::new(first);
        assert_eq!(held.id.0, first_id);
        let second = picture();
        let second_id = second.id.0;
        held = OwnedImage::new(second);

        let swept = swept_ids();
        assert!(swept.contains(&first_id));
        assert!(!swept.contains(&second_id));
        assert!(first_weak.upgrade().is_none());
        assert_eq!(held.id.0, second_id);
    }

    #[test]
    fn a_dropped_image_leaves_once_its_painted_copy_is_gone() {
        let image = picture();
        let (id, weak) = (image.id.0, Arc::downgrade(&image));
        let owned = OwnedImage::from(image);
        // A frame still painting it holds a clone.
        let painting: Arc<RenderImage> = owned.clone();
        drop(owned);
        assert!(!swept_ids().contains(&id));

        drop(painting);
        assert!(swept_ids().contains(&id));
        assert!(weak.upgrade().is_none());
    }
}
