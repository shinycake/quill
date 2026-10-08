//! Bounded image memory for the main window.
//!
//! GPUI keeps every image an `img(path)` ever loaded — decoded in RAM and
//! uploaded to the window's sprite atlas — for the life of the app, and an
//! `Arc<RenderImage>` painted once stays in the atlas until someone calls
//! `drop_image`. Scrolling through avatars and photos therefore only ever
//! grew the footprint. This module bounds both:
//!
//! - [`BoundedImageCache`] loads the window's path images (an `ImageCache`
//!   installed over `QuillApp` by [`CacheScope`]) and, past a byte budget,
//!   evicts the least recently used images that can no longer be on screen,
//!   dropping them from the atlas too.
//! - [`retire_all`] takes `RenderImage`s Quill made itself (sticker frames,
//!   blurred previews…) once their cache lets go of them; [`sweep`] drops
//!   them from the atlas when no frame can still show them.
//!
//! "Can no longer be on screen" accounts for cached slices (`app_slice`):
//! a slice replays the frame it last rendered, so an image it used is only
//! gone once that slice rendered again without it. Images used outside any
//! slice are re-requested by `QuillApp` every frame.

use gpui_kit::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

/// Decoded path images kept beyond what is on screen.
const IMAGE_BUDGET_BYTES: usize = 128 * 1024 * 1024;
/// Cache entries kept beyond what is on screen (failed loads weigh 0).
const IMAGE_BUDGET_ENTRIES: usize = 4096;

thread_local! {
    /// Window frames drawn so far (`begin_frame`).
    static FRAME: Cell<u64> = const { Cell::new(0) };
    /// Each slice's last render frame, and the last frame it was shown.
    static SLICES: RefCell<HashMap<EntityId, (u64, u64)>> = RefCell::new(HashMap::new());
    /// Quill-made images waiting to leave the atlas, with the frame they
    /// were retired in.
    static RETIRED: RefCell<Vec<(Arc<RenderImage>, u64)>> = const { RefCell::new(Vec::new()) };
}

/// Start a window frame (`QuillApp::render`, before anything renders).
pub(super) fn begin_frame() -> u64 {
    FRAME.with(|frame| {
        frame.set(frame.get() + 1);
        frame.get()
    })
}

fn frame() -> u64 {
    FRAME.with(Cell::get)
}

/// A slice rendered its content this frame.
pub(super) fn slice_rendered(id: EntityId) {
    let now = frame();
    SLICES.with(|slices| {
        slices.borrow_mut().insert(id, (now, now));
    });
}

/// A slice is part of this frame (rendered or replayed).
pub(super) fn slice_shown(id: EntityId) {
    let now = frame();
    SLICES.with(|slices| {
        slices.borrow_mut().entry(id).or_insert((0, now)).1 = now;
    });
}

/// Whether an image last used in `scope` at `used` can still be on
/// screen, as of the start of the current frame.
fn may_be_visible(scope: Option<EntityId>, used: u64) -> bool {
    let now = frame();
    match scope {
        // `QuillApp` itself renders every frame: the last one shows all.
        None => used + 1 >= now,
        Some(id) => SLICES.with(|slices| {
            slices
                .borrow()
                .get(&id)
                // A slice that wasn't shown last frame replays nothing.
                .is_some_and(|(rendered, shown)| shown + 1 >= now && used >= *rendered)
        }),
    }
}

/// The oldest frame a cached slice shown last frame may still replay.
fn oldest_replayable() -> u64 {
    let now = frame();
    SLICES.with(|slices| {
        slices
            .borrow()
            .values()
            .filter(|(_, shown)| shown + 1 >= now)
            .map(|(rendered, _)| *rendered)
            .min()
            .unwrap_or(now)
            // `QuillApp` re-renders each frame, so last frame's images
            // are the newest anything still shows.
            .min(now.saturating_sub(1))
    })
}

/// Hand images Quill made (and painted) to the atlas sweeper once their
/// cache lets go of them: each is dropped from the atlas when no frame can
/// show it and nothing else holds it.
pub(super) fn retire_all(images: impl IntoIterator<Item = Arc<RenderImage>>) {
    let now = frame();
    RETIRED.with(|retired| {
        retired
            .borrow_mut()
            .extend(images.into_iter().map(|image| (image, now)));
    });
}

/// Drop retired images no frame can show any more from the atlas. Call at
/// the start of a frame, before anything renders.
pub(super) fn sweep(window: &mut Window, cx: &mut App) {
    let safe_before = oldest_replayable();
    let gone: Vec<Arc<RenderImage>> = RETIRED.with(|retired| {
        let mut retired = retired.borrow_mut();
        let mut gone = Vec::new();
        retired.retain(|(image, at)| {
            // Still held elsewhere (a cache that took it back, the chat
            // list's animation layer): it may be painted again.
            if *at >= safe_before || Arc::strong_count(image) > 1 {
                return true;
            }
            gone.push(image.clone());
            false
        });
        gone
    });
    for image in gone {
        cx.drop_image(image, Some(&mut *window));
    }
}

struct Entry {
    item: ImageCacheItem,
    /// Decoded size once loaded (0 for failures).
    bytes: Option<usize>,
    /// The last frame each scope (slice, or `None` for the app) used it.
    uses: smallvec::SmallVec<[(Option<EntityId>, u64); 2]>,
}

impl Entry {
    fn last_used(&self) -> u64 {
        self.uses.iter().map(|(_, at)| *at).max().unwrap_or(0)
    }

    fn may_be_visible(&self) -> bool {
        self.uses
            .iter()
            .any(|(scope, used)| may_be_visible(*scope, *used))
    }
}

/// The main window's image cache: least recently used images that can't
/// be on screen go once the decoded total passes the budget.
pub(super) struct BoundedImageCache {
    entries: HashMap<u64, Entry>,
    bytes: usize,
    /// The slice being drawn (shared with `app_slice`).
    scope: Rc<Cell<Option<EntityId>>>,
}

impl BoundedImageCache {
    pub(super) fn new(scope: Rc<Cell<Option<EntityId>>>) -> Self {
        Self {
            entries: HashMap::new(),
            bytes: 0,
            scope,
        }
    }

    /// Evict down to the budget. Call at the start of a frame, before
    /// anything renders, so last frame's loads are all counted.
    pub(super) fn trim(&mut self, window: &mut Window, cx: &mut App) {
        if self.bytes <= IMAGE_BUDGET_BYTES && self.entries.len() <= IMAGE_BUDGET_ENTRIES {
            return;
        }
        let mut candidates: Vec<(u64, u64)> = self
            .entries
            .iter()
            .filter(|(_, entry)| !entry.may_be_visible())
            .map(|(key, entry)| (entry.last_used(), *key))
            .collect();
        candidates.sort_unstable();
        for (_, key) in candidates {
            if self.bytes <= IMAGE_BUDGET_BYTES && self.entries.len() <= IMAGE_BUDGET_ENTRIES {
                break;
            }
            let Some(entry) = self.entries.remove(&key) else {
                continue;
            };
            self.bytes -= entry.bytes.unwrap_or(0);
            if let Some(Ok(image)) = entry.item.get() {
                cx.drop_image(image, Some(window));
            }
        }
    }
}

impl ImageCache for BoundedImageCache {
    fn load(
        &mut self,
        resource: &Resource,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        let key = hash(resource);
        let scope = self.scope.get();
        let now = frame();
        let entry = self.entries.entry(key).or_insert_with(|| Entry {
            item: ImageCacheItem::new(resource, cx),
            bytes: None,
            uses: smallvec::SmallVec::new(),
        });
        match entry.uses.iter_mut().find(|(s, _)| *s == scope) {
            Some(use_) => use_.1 = now,
            None => entry.uses.push((scope, now)),
        }
        let result = entry.item.use_image(window);
        if entry.bytes.is_none()
            && let Some(result) = &result
        {
            let bytes = match result {
                Ok(image) => (0..image.frame_count())
                    .map(|frame| {
                        let size = image.size(frame);
                        size.width.0.max(0) as usize * size.height.0.max(0) as usize * 4
                    })
                    .sum(),
                Err(_) => 0,
            };
            entry.bytes = Some(bytes);
            self.bytes += bytes;
        }
        result
    }
}

/// Installs an image cache over its child for every drawing phase. GPUI's
/// own `image_cache` element skips prepaint, where virtual lists and
/// cached views build their content.
pub(super) struct CacheScope {
    cache: AnyImageCache,
    child: AnyElement,
}

impl CacheScope {
    pub(super) fn new(cache: AnyImageCache, child: impl IntoElement) -> Self {
        Self {
            cache,
            child: child.into_any_element(),
        }
    }
}

impl IntoElement for CacheScope {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for CacheScope {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let child = &mut self.child;
        let layout_id = window.with_image_cache(Some(self.cache.clone()), |window| {
            child.request_layout(window, cx)
        });
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let child = &mut self.child;
        window.with_image_cache(Some(self.cache.clone()), |window| {
            child.prepaint(window, cx)
        });
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let child = &mut self.child;
        window.with_image_cache(Some(self.cache.clone()), |window| child.paint(window, cx));
    }
}

#[cfg(test)]
mod tests {
    use super::{begin_frame, may_be_visible, slice_rendered, slice_shown};
    use gpui_kit::EntityId;

    #[test]
    fn app_images_stay_for_one_frame() {
        let used = begin_frame();
        assert!(may_be_visible(None, used));
        begin_frame();
        assert!(may_be_visible(None, used), "last frame may still show it");
        begin_frame();
        assert!(!may_be_visible(None, used));
    }

    #[test]
    fn slice_images_stay_until_the_slice_renders_again() {
        let slice = EntityId::from((1_u64 << 32) | 7);
        let used = begin_frame();
        slice_rendered(slice);
        // Replayed (shown, not rendered) for a while: still on screen.
        for _ in 0..5 {
            begin_frame();
            assert!(may_be_visible(Some(slice), used));
            slice_shown(slice);
        }
        // Rendered again without it: gone from the next frame on.
        slice_rendered(slice);
        begin_frame();
        assert!(!may_be_visible(Some(slice), used));
    }

    #[test]
    fn hidden_slices_show_nothing() {
        let slice = EntityId::from((1_u64 << 32) | 9);
        let used = begin_frame();
        slice_rendered(slice);
        begin_frame();
        begin_frame();
        assert!(!may_be_visible(Some(slice), used));
    }
}
