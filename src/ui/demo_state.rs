//! Screenshot-demo stand-ins for devices, call video and the log sink.

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
    /// Phase C2e: synthetic demo video frames for the screenshot
    /// fixture only — live frames come from the driver, never these.
    pub(super) remote_frame: Option<quill::calls::engine::VideoFrame>,
    pub(super) local_frame: Option<quill::calls::engine::VideoFrame>,
    /// Phase C2l: synthetic demo screen-share frame (peer side) for the
    /// screenshot fixture only — live frames come from the driver.
    pub(super) screen_frame: Option<quill::calls::engine::VideoFrame>,
    /// Phase C2e: demo-mode camera pick (live picks go to the driver).
    pub(super) selected_camera: Option<String>,
    /// Phase C2g: synthetic per-participant frames injected by the
    /// group-call demo fixture, keyed `(user_id, is_screen)`.
    /// Injected demo data, not real media.
    pub(super) group_frames: HashMap<(i64, bool), quill::calls::engine::VideoFrame>,
    /// Demo captures can't go full screen: show the stage anyway.
    pub(super) group_stage: bool,
    pub(super) seq: AtomicU64,
    pub(super) sink: Arc<MemorySink>,
}

impl DemoUi {
    pub(super) fn new(demo_sink: Arc<MemorySink>) -> Self {
        Self {
            call_devices: None,
            selected_devices: (None, None),
            remote_frame: None,
            local_frame: None,
            screen_frame: None,
            selected_camera: None,
            group_frames: HashMap::new(),
            group_stage: false,
            seq: AtomicU64::new(0),
            sink: demo_sink,
        }
    }
}
