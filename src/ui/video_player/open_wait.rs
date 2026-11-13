//! Hearing that a clip opened, without polling it.
#![deny(clippy::indexing_slicing)]

use super::VideoPlayer;
use gpui_kit::{App, Task};
use std::rc::Rc;
use std::time::Instant;

impl VideoPlayer {
    /// Redraw the watchers once, as soon as the backend says the clip is
    /// open (or failed to). The task holds no strong handle: dropping the
    /// player ends it. `None` for a backend that cannot say so: the frames it
    /// is drawn in ask for its picture for a short while
    /// (`transport::awaits_picture`).
    pub(super) fn await_open(&self, cx: &mut App) -> Option<Task<()>> {
        let (signal, opened) = futures_channel::oneshot::channel::<()>();
        let signals = self.with_backend(|backend| {
            backend.on_open(Box::new(move || {
                let _ = signal.send(());
            }))
        })?;
        if !signals {
            return None;
        }
        // The backend says when it opened: no frames while it opens, and
        // the wake asks for the first picture.
        self.with_inner(|inner| inner.life.picture_came());
        let weak = Rc::downgrade(&self.0);
        Some(cx.spawn(async move |cx| {
            let _ = opened.await;
            let Some(cell) = weak.upgrade() else {
                return;
            };
            let player = VideoPlayer(cell);
            let now = Instant::now();
            player.update_status(now);
            player.with_inner(|inner| inner.life.await_picture(now));
            let watchers = player.views_to_redraw();
            cx.update(|cx| {
                for id in watchers {
                    cx.notify(id);
                }
            });
        }))
    }
}
