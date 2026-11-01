//! Quill-made images waiting to leave the atlas ([`retire_all`]), and the
//! sweep that drops each one once no window can show it.
//!
//! Any window may have painted a retired image, and a window that stops
//! drawing (minimized, covered, hidden) keeps its last frame, which the
//! system may show again at any time. So a sweep asks every open window
//! what its latest frame can show:
//!
//! - the main window: what `QuillApp` drew in its latest frame
//!   ([`frame`](super::frame)) and what the slices it replayed last
//!   rendered, as of that frame however long ago it was drawn;
//! - every other window (chat windows, Picture-in-Picture, calls): its
//!   latest frame, by GPUI's count of the frames it began.
//!
//! A frame only shows images made before it was done (image ids only
//! grow), so the frames a chat window retires go after its next frame,
//! whether the main window draws or not.
//!
//! Sweeps run between frames: as the main window starts a frame, and in a
//! task woken by a retirement, a chat window's frame or a window closing
//! ([`start_sweeper`]), so retired images also go with no window drawing,
//! or none open.

use super::{SLICES, frame, next_image_id};
use futures_channel::oneshot;
use gpui_kit::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::Arc;

thread_local! {
    /// Images waiting to leave the atlas, by image id.
    static RETIRED: RefCell<HashMap<usize, Retired>> = RefCell::new(HashMap::new());
    /// Sweeps so far.
    static SWEEPS: Cell<u64> = const { Cell::new(0) };
    /// Each other open window's latest frame, as the sweeps saw it.
    static SEEN: RefCell<HashMap<WindowId, Seen>> = RefCell::new(HashMap::new());
    /// The main window (`QuillApp`'s), once it drew.
    static MAIN: Cell<Option<WindowId>> = const { Cell::new(None) };
    /// The main window's latest frame once done, and the bound on the ids
    /// of the images it painted.
    static MAIN_DONE: Cell<Option<(u64, usize)>> = const { Cell::new(None) };
    /// Wakes the sweeper task.
    static WAKE: RefCell<Option<oneshot::Sender<()>>> = const { RefCell::new(None) };
}

struct Retired {
    image: Arc<RenderImage>,
    /// The main window's frame and the sweep count when it was (last)
    /// retired.
    frame: u64,
    sweep: u64,
}

/// A window's latest frame, as the sweeps saw it.
#[derive(Clone, Copy)]
struct Seen {
    /// GPUI's count of the frames the window began.
    frame: u64,
    /// The first sweep that saw it: a frame first seen two sweeps after a
    /// retirement began after it.
    sweep: u64,
    /// Every image the frame painted has an id below this.
    images_below: usize,
}

impl Seen {
    /// Whether the frame may show image `id`, retired after sweep `retired`.
    fn may_show(self, id: usize, retired: u64) -> bool {
        self.sweep <= retired + 1 && id < self.images_below
    }
}

/// What the main window's latest frame shows.
struct MainFrame {
    frame: u64,
    /// Every image the frame painted has an id below this.
    images_below: usize,
    /// Each slice it replayed: the frame of the slice's last render, and
    /// the bound on what that render painted.
    slices: Vec<(u64, usize)>,
}

impl MainFrame {
    /// Between frames: the main window's latest frame is done.
    fn now() -> Self {
        let frame = frame();
        let images_below = close_main_frame();
        let slices = SLICES.with(|slices| {
            slices
                .borrow()
                .values()
                .filter(|s| s.shown >= frame)
                .map(|s| (s.rendered, s.images_below.unwrap_or(usize::MAX)))
                .collect()
        });
        Self {
            frame,
            images_below,
            slices,
        }
    }

    /// Whether it may show image `id`, retired in main frame `retired`:
    /// `QuillApp` drew it if it was retired no earlier than this frame and
    /// already existed then; a replayed slice, if it was retired no
    /// earlier than the slice's last render and existed by its end.
    /// Without the id check a chat list replaying for minutes kept every
    /// video frame retired meanwhile.
    fn may_show(&self, id: usize, retired: u64) -> bool {
        (retired >= self.frame && id < self.images_below)
            || self
                .slices
                .iter()
                .any(|&(rendered, below)| retired >= rendered && id < below)
    }
}

/// The main window's latest frame is done: bound the ids of the images it
/// (and each slice that rendered in it) painted, once. Call between frames
/// or as the next main frame starts. Returns the bound.
pub(super) fn close_main_frame() -> usize {
    let frame = frame();
    if let Some((done, below)) = MAIN_DONE.get()
        && done == frame
    {
        return below;
    }
    let below = next_image_id();
    MAIN_DONE.set(Some((frame, below)));
    SLICES.with(|slices| {
        for slice in slices.borrow_mut().values_mut() {
            if slice.rendered == frame && slice.images_below.is_none() {
                slice.images_below = Some(below);
            }
        }
    });
    below
}

/// Hand images Quill made (and painted) to the atlas sweeper once their
/// cache lets go of them: each is dropped from the atlas when no window
/// can show it and nothing else holds it. Retiring an image again only
/// moves its stamps: two entries would each hold it, and it would never
/// leave.
pub(in crate::ui) fn retire_all(images: impl IntoIterator<Item = Arc<RenderImage>>) {
    let (frame, sweep) = (frame(), SWEEPS.get());
    let mut any = false;
    RETIRED.with(|retired| {
        retired.borrow_mut().extend(images.into_iter().map(|image| {
            any = true;
            (
                image.id.0,
                Retired {
                    image,
                    frame,
                    sweep,
                },
            )
        }));
    });
    // The sticker player hands over an empty list for every frame it
    // fetches: no sweep for those.
    if any {
        wake();
    }
}

/// Something may have become sweepable: wake the sweeper task, if any
/// image waits.
pub(super) fn wake() {
    if RETIRED.with(|retired| retired.borrow().is_empty()) {
        return;
    }
    if let Some(wake) = WAKE.with(|slot| slot.borrow_mut().take()) {
        let _ = wake.send(());
    }
}

/// The main window starts a frame: its last frame is done, so sweep first
/// (dropping from `window`'s atlas too), then count the new frame.
pub(in crate::ui) fn begin_main_frame(window: &mut Window, cx: &mut App) -> u64 {
    MAIN.set(Some(window.window_handle().window_id()));
    sweep(Some(window), cx);
    super::begin_frame()
}

/// Marks an app whose sweeper runs; holds its watch on windows closing.
struct Sweeper {
    _closed: Subscription,
}

impl Global for Sweeper {}

/// Start sweeping between frames, once per app (`QuillApp::new_with_demo`):
/// after a retirement, a chat window's frame or a window closing. Before,
/// only the main window swept, at its own frames: while it did not draw
/// (minimized, covered, closed) a chat window playing stickers grew by
/// 25 MB a second, and with no window open what was retired stayed.
pub(in crate::ui) fn start_sweeper(cx: &mut App) {
    if cx.has_global::<Sweeper>() {
        return;
    }
    // Window ids from an earlier app on this thread (tests) mean nothing.
    MAIN.set(None);
    SEEN.with(|seen| seen.borrow_mut().clear());
    let closed = cx.on_window_closed(|_, _| wake());
    cx.set_global(Sweeper { _closed: closed });
    // Armed now, not when the task first runs: a wake before then counts.
    let mut woken = arm();
    cx.spawn(async move |cx| {
        while woken.await.is_ok() {
            cx.update(|cx| sweep(None, cx));
            woken = arm();
        }
    })
    .detach();
}

/// A fresh wake for the sweeper task. A wake while it sweeps is not
/// needed: that sweep comes after it.
fn arm() -> oneshot::Receiver<()> {
    let (wake, woken) = oneshot::channel();
    WAKE.with(|slot| *slot.borrow_mut() = Some(wake));
    woken
}

/// Drop the retired images no window can show any more from every
/// window's atlas. Call between frames: with no window drawing, or as the
/// main window (`current`) starts a frame.
fn sweep(current: Option<&mut Window>, cx: &mut App) {
    let main = MAIN.get();
    let mut main_open = false;
    let mut windows = Vec::new();
    for handle in cx.windows() {
        let id = handle.window_id();
        if Some(id) == main {
            main_open = true;
            continue;
        }
        // A window being drawn: not between frames, no sweep.
        let Ok(frame) = handle.update(cx, |_, window, _| window.frame_number()) else {
            return;
        };
        windows.push((id, frame));
    }
    drop_images_off_ui_thread(take_unshowable(main_open, &windows), current, cx);
}

pub(in crate::ui) fn drop_images_off_ui_thread(
    images: Vec<Arc<RenderImage>>,
    mut current: Option<&mut Window>,
    cx: &mut App,
) {
    if images.is_empty() {
        return;
    }
    for image in &images {
        cx.drop_image(image.clone(), current.as_deref_mut());
    }
    cx.background_spawn(async move { drop(images) }).detach();
}

/// The retired images no window can show any more and nothing else holds,
/// between frames: `main` says whether the main window is open, `windows`
/// lists every other open window and the frames it began.
pub(in crate::ui) fn take_unshowable(
    main: bool,
    windows: &[(WindowId, u64)],
) -> Vec<Arc<RenderImage>> {
    let sweep = SWEEPS.get() + 1;
    SWEEPS.set(sweep);
    let seen = see(windows, sweep);
    let main = main.then(MainFrame::now);
    RETIRED.with(|retired| {
        let mut gone = Vec::new();
        retired.borrow_mut().retain(|&id, entry| {
            // Still held elsewhere (a cache that took it back, the chat
            // list's animation layer): it may be painted again.
            if Arc::strong_count(&entry.image) > 1
                || main
                    .as_ref()
                    .is_some_and(|main| main.may_show(id, entry.frame))
                || seen.iter().any(|window| window.may_show(id, entry.sweep))
            {
                return true;
            }
            gone.push(entry.image.clone());
            false
        });
        gone
    })
}

/// Note each window's latest frame at sweep `sweep`, and forget the
/// windows that closed.
fn see(windows: &[(WindowId, u64)], sweep: u64) -> Vec<Seen> {
    SEEN.with(|seen| {
        let mut seen = seen.borrow_mut();
        seen.retain(|id, _| windows.iter().any(|(open, _)| open == id));
        windows
            .iter()
            .map(|&(id, frame)| {
                // Seen first now: done by now, so it painted nothing newer.
                let first = || Seen {
                    frame,
                    sweep,
                    images_below: next_image_id(),
                };
                let entry = seen.entry(id).or_insert_with(first);
                if entry.frame != frame {
                    *entry = first();
                }
                *entry
            })
            .collect()
    })
}

#[cfg(test)]
#[path = "retired_tests.rs"]
mod tests;
