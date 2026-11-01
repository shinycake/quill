//! Opening a picture file for an editor or the clipboard without stalling
//! the UI thread. The decode goes through [`super::image_decode`] (size
//! limits, panic guard, HEIC / AVIF / JPEG XL) on a background task, and the
//! caller acts when it resolves.
use super::app::QuillApp;
use super::image_decode::{DecodeError, DecodeSize, decode_file};
use gpui_kit::{AppContext, Context};
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
        self.run_off_ui_then(move || decode_file(&path, DecodeSize::FULL), done, cx);
    }

    /// Run `work` on the background executor and hand its result to `done`
    /// on the UI thread. A panic in `work` is not caught here: wrap
    /// third-party decoders (the image_decode entry points already do).
    pub(super) fn run_off_ui_then<T: Send + 'static>(
        &mut self,
        work: impl FnOnce() -> T + Send + 'static,
        done: impl FnOnce(&mut Self, T, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let task = cx.background_spawn(async move { work() });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| done(this, result, cx));
        })
        .detach();
    }

    /// [`Self::run_off_ui_then`] for work whose result lands in the window
    /// it started in, and is dropped once that window has closed.
    pub(super) fn run_off_ui_scoped_then<T: Send + 'static>(
        &mut self,
        work: impl FnOnce() -> T + Send + 'static,
        done: impl FnOnce(&mut Self, T, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let task = cx.background_spawn(async move { work() });
        self.spawn_scoped(cx, async move |app, cx| {
            let result = task.await;
            let _ = app.update(cx, |this, cx| done(this, result, cx));
        })
        .detach();
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "image_open_tests.rs"]
mod tests;
