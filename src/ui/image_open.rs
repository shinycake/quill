//! Opening a picture file for an editor or the clipboard without stalling
//! the UI thread. The decode goes through [`super::image_decode`] (size
//! limits, panic guard, HEIC / AVIF / JPEG XL) on a background task, and the
//! caller acts when it resolves.
use super::app::QuillApp;
use super::image_decode::{DecodeError, DecodeSize, decode_file};
use gpui_kit::{AppContext, Context, Task};
use std::path::PathBuf;

impl QuillApp {
    /// Decode `path` at full size off the UI thread, then call `done` on it
    /// with the pixels or the reason they could not be read.
    pub(super) fn decode_image_then(
        &mut self,
        path: PathBuf,
        done: impl FnOnce(&mut Self, Result<image::RgbaImage, DecodeError>, &mut Context<Self>)
        + 'static,
        cx: &mut Context<Self>,
    ) {
        self.decode_image_task(path, done, cx).detach();
    }

    /// [`Self::decode_image_then`] for a stage that owns the job: dropping
    /// the returned task cancels the decode and its answer.
    pub(super) fn decode_image_task(
        &mut self,
        path: PathBuf,
        done: impl FnOnce(&mut Self, Result<image::RgbaImage, DecodeError>, &mut Context<Self>)
        + 'static,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        // The editor it opens is drawn in the window that asked.
        self.run_off_ui_scoped_task(move || decode_file(&path, DecodeSize::FULL), done, cx)
    }

    /// Run `work` on the background executor and hand its result to `done`
    /// on the UI thread, as the main window of the account on screen now:
    /// after a switch `done` does not run, since whatever it would hand
    /// the result to (a driver, a box) is the other account's by then. A
    /// panic in `work` is not caught here: wrap third-party decoders (the
    /// image_decode entry points already do).
    pub(super) fn run_off_ui_then<T: Send + 'static>(
        &mut self,
        work: impl FnOnce() -> T + Send + 'static,
        done: impl FnOnce(&mut Self, T, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        self.run_off_ui_held(work, done, cx).detach();
    }

    /// [`Self::run_off_ui_then`] for an owner that keeps the task: it
    /// stops with the owner, and `done` does not run for an owner that
    /// went away.
    pub(super) fn run_off_ui_held<T: Send + 'static>(
        &mut self,
        work: impl FnOnce() -> T + Send + 'static,
        done: impl FnOnce(&mut Self, T, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let task = cx.background_spawn(async move { work() });
        self.spawn_screen(cx, async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| done(this, result, cx));
        })
    }

    /// [`Self::run_off_ui_then`] for work whose result lands in the window
    /// it started in, and is dropped once that window has closed.
    pub(super) fn run_off_ui_scoped_then<T: Send + 'static>(
        &mut self,
        work: impl FnOnce() -> T + Send + 'static,
        done: impl FnOnce(&mut Self, T, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        self.run_off_ui_scoped_task(work, done, cx).detach();
    }

    /// [`Self::run_off_ui_scoped_then`] for a stage that owns the job:
    /// dropping the returned task cancels the work and its answer.
    pub(super) fn run_off_ui_scoped_task<T: Send + 'static>(
        &mut self,
        work: impl FnOnce() -> T + Send + 'static,
        done: impl FnOnce(&mut Self, T, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let task = cx.background_spawn(async move { work() });
        self.spawn_scoped(cx, async move |app, cx| {
            let result = task.await;
            let _ = app.update(cx, |this, cx| done(this, result, cx));
        })
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "image_open_tests.rs"]
mod tests;
