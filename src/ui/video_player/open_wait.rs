//! Hearing that a clip opened, without polling it.
#![deny(clippy::indexing_slicing)]

use super::VideoPlayer;
use gpui_kit::{App, Task};
use std::rc::Rc;
use std::time::Instant;

impl VideoPlayer {
    /// Redraw the watchers once, as soon as the backend says the clip is
    /// open (or failed to). The task holds no strong handle: dropping the
    /// player ends it. `None` for a backend that cannot say so (AVPlayer
    /// reports `readyToPlay` without a signal): the frames it is drawn in
    /// poll it while it opens (`PlayState::awaits_picture`).
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
        let weak = Rc::downgrade(&self.0);
        Some(cx.spawn(async move |cx| {
            let _ = opened.await;
            let Some(cell) = weak.upgrade() else {
                return;
            };
            let player = VideoPlayer(cell);
            player.update_status(Instant::now());
            let watchers = player.watchers();
            cx.update(|cx| {
                for id in watchers {
                    cx.notify(id);
                }
            });
        }))
    }
}
