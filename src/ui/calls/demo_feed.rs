//! Screenshot fixtures' live feeds: a picture pushed through a
//! [`LiveSource`] at 30 fps, so a demo draws through the same live player
//! as a call. The frames are injected demo data, not real media.
#![deny(clippy::indexing_slicing)]

use gpui_kit::*;
use quill::calls::live_video::convert::rgba_to_i420;
use quill::calls::live_video::{LiveEnd, LiveFrame, LiveRotation, LiveSink, LiveSource};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A 30 fps sender.
const FRAME_GAP: Duration = Duration::from_millis(33);

/// A fixture's feed: the sink for the player and the task pushing frames
/// (dropping it stops them).
pub(crate) struct DemoFeed {
    pub(crate) sink: LiveSink,
    pub(crate) source: LiveSource,
    _task: Option<Task<()>>,
}

impl Drop for DemoFeed {
    /// The fixture's sender is gone: its feed is over.
    fn drop(&mut self) {
        self.source.end(LiveEnd::Over);
    }
}

impl DemoFeed {
    /// A feed with nothing sending: tests push through `source`.
    #[cfg(all(test, feature = "demo-capture"))]
    pub(crate) fn manual() -> Self {
        let (sink, source) = LiveSink::new();
        Self {
            sink,
            source,
            _task: None,
        }
    }
}

/// A feed of `rgba` (`width` × `height`): one frame at once, then, when
/// `repeat`, the same picture as a new frame every 33 ms.
pub(crate) fn demo_feed(
    rgba: &[u8],
    width: u16,
    height: u16,
    repeat: bool,
    cx: &mut App,
) -> Option<DemoFeed> {
    let planes: Arc<Vec<u8>> = Arc::new(rgba_to_i420(rgba, width, height)?);
    let (sink, source) = LiveSink::new();
    push(&source, 1, width, height, &planes);
    let task = repeat.then(|| {
        let source = source.clone();
        cx.spawn(async move |cx| {
            let mut seq = 1;
            while !source.is_ended() {
                cx.background_executor().timer(FRAME_GAP).await;
                seq += 1;
                push(&source, seq, width, height, &planes);
            }
        })
    });
    Some(DemoFeed {
        sink,
        source,
        _task: task,
    })
}

fn push(source: &LiveSource, seq: u64, width: u16, height: u16, planes: &[u8]) {
    let frame = LiveFrame::new(
        seq,
        width,
        height,
        LiveRotation::None,
        planes.to_vec(),
        Instant::now(),
    );
    if let Some(frame) = frame {
        source.push(frame);
    }
}
