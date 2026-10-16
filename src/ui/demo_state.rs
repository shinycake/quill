//! Screenshot-demo stand-ins for devices, call video and the log sink.
#![deny(clippy::indexing_slicing)]

use quill::diagnostics::MemorySink;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

pub(crate) struct DemoUi {
    /// Phase C2c screenshot ReadyCallDevices demo: injected demo devices
    /// only (no live Telegram, no real hardware); `None` means the engine
    /// reported none.
    pub(super) call_devices: Option<Vec<quill::calls::engine::MediaDevice>>,
    pub(super) selected_devices: (Option<String>, Option<String>),
    /// Phase C2e: demo-mode camera pick (live picks go to the driver).
    pub(super) selected_camera: Option<String>,
    /// Synthetic feeds injected by the call demo fixtures, by feed: a
    /// video chat's participants, a 1:1 call's pictures. Injected demo
    /// data, not real media.
    pub(super) group_feeds:
        HashMap<quill::calls::live_video::LiveKey, super::calls::demo_feed::DemoFeed>,
    /// Demo captures can't go full screen: show the stage (and a stream
    /// window's full-screen layout) anyway.
    pub(super) group_stage: bool,
    pub(super) seq: AtomicU64,
    pub(super) sink: Arc<MemorySink>,
}

impl DemoUi {
    pub(super) fn new(demo_sink: Arc<MemorySink>) -> Self {
        Self {
            call_devices: None,
            selected_devices: (None, None),
            selected_camera: None,
            group_feeds: HashMap::new(),
            group_stage: false,
            seq: AtomicU64::new(0),
            sink: demo_sink,
        }
    }
}
