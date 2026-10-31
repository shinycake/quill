//! Freed heap goes back to the system once Quill is idle (`quill::free_heap`,
//! `docs/decisions/idle-heap-return.md`): the task that waits for it, the
//! input that puts it off, and the frame it runs after while an animation
//! keeps a window drawing.

use super::app::QuillApp;
use gpui_kit::*;
use quill::free_heap::{self, IdleRelease};

/// When freed heap goes back, and the task that waits for it.
#[derive(Default)]
pub(super) struct HeapRelease {
    idle: IdleRelease,
    task: Option<Task<()>>,
}

impl QuillApp {
    /// A window begins a frame. If the release is due while windows keep
    /// drawing (an animation, with no input), it runs once this frame is
    /// drawn, in the gap before the next; otherwise the task waits for it.
    pub(super) fn heap_frame_begins(&mut self, cx: &mut Context<Self>) {
        let now = cx.background_executor().now();
        if self.slices.heap.idle.frame_begins(now) {
            let app = cx.weak_entity();
            // Effects run once the window's frame is drawn and presented.
            cx.defer(move |cx| {
                let _ = app.update(cx, |this, _| this.with_heap(IdleRelease::frame_drawn));
            });
        }
        self.schedule_heap_release(cx);
    }

    /// Input reached a window (a key, a pointer event, a scroll), or a
    /// window moved or was resized: the release waits
    /// `free_heap::AFTER_INPUT` past it.
    pub(super) fn note_heap_input(&mut self, cx: &App) {
        let now = cx.background_executor().now();
        self.slices.heap.idle.input(now);
    }

    /// Once at least `free_heap::WORTH_RELEASING` was freed (laid-out
    /// text, messages, pictures, sticker frames), wait for its release.
    /// Call where a frame begins or after something freed outside one.
    /// One waiting task at most; it looks again until the batch went back.
    pub(super) fn schedule_heap_release(&mut self, cx: &mut Context<Self>) {
        if self.slices.heap.task.is_some() || free_heap::freed() < free_heap::WORTH_RELEASING {
            return;
        }
        self.slices.heap.task = Some(cx.spawn(async move |this, cx| {
            loop {
                let frames = cx.update(frames_begun);
                let wait = this.update(cx, |this, cx| {
                    let now = cx.background_executor().now();
                    let wait = this.with_heap(|idle| idle.look(now, frames));
                    if wait.is_none() {
                        this.slices.heap.task = None;
                    }
                    wait
                });
                let Ok(Some(wait)) = wait else {
                    break;
                };
                cx.background_executor().timer(wait).await;
            }
        }));
    }

    /// `f` on the release schedule; a release it made is logged with
    /// `QUILL_TRACE_TICKS=1`.
    fn with_heap<R>(&mut self, f: impl FnOnce(&mut IdleRelease) -> R) -> R {
        let (freed, released) = (free_heap::freed(), free_heap::releases());
        let out = f(&mut self.slices.heap.idle);
        if free_heap::releases() > released && super::frame_clock::trace_ticks() {
            eprintln!("heap release: {:.1} MB freed", freed as f64 / 1048576.0);
        }
        out
    }
}

/// Frames begun so far in all of Quill's windows: a change means some
/// window drew.
fn frames_begun(cx: &mut App) -> u64 {
    cx.windows()
        .into_iter()
        .filter_map(|window| window.update(cx, |_, window, _| window.frame_number()).ok())
        .sum()
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "idle_heap_tests.rs"]
mod tests;
