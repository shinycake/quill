//! Screenshot demo of a channel's statistics panel (injected, no live
//! Telegram): the overview and one chart of every kind.
//! `QUILL_DEMO_FIRST_CHART=<n>` starts the page at the n-th chart, so the
//! later charts fit a capture.

use super::fixture::seed_channel_statistics;
use crate::ui::app::QuillApp;
use crate::ui::groups::apply_ready_channels;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::state::InfoPanelTarget;
use std::sync::atomic::Ordering;

/// The demo channel.
const CHANNEL: i64 = 13;

register_demos![
    DemoSpec::chats(
        "ready-statistics",
        "screenshot demo — channel statistics charts"
    )
    .setup(QuillApp::demo_statistics),
];

impl QuillApp {
    fn demo_statistics(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_channels(session, &self.demo_ui.sink, &self.demo_ui.seq);
            seed_channel_statistics(session, CHANNEL);
        }
        self.open_info_panel_target(InfoPanelTarget::Statistics(CHANNEL), window, cx);
    }
}

/// The chart the page starts at (`QUILL_DEMO_FIRST_CHART`), 0 for the
/// whole page.
pub(super) fn first_chart() -> usize {
    static FIRST: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *FIRST.get_or_init(|| {
        std::env::var("QUILL_DEMO_FIRST_CHART")
            .ok()
            .and_then(|n| n.parse().ok())
            .unwrap_or(0)
    })
}
