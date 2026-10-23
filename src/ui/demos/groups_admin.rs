//! Screenshot demos: groups admin.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::groups::apply_ready_group_manage;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::state::InfoPanelTarget;
use std::sync::atomic::Ordering;

register_demos![
    // Group info demo (injected, no live Telegram): the
    // `ReadyGroupManage` demo supergroup (id 61, viewer an admin with
    // `can_change_info`) with the info panel open on its "Manage group"
    // entry.
    DemoSpec::chats("ready-group-info-edit", "screenshot demo — group info edit")
        .setup(QuillApp::demo_ready_group_info_edit),
];

impl QuillApp {
    fn demo_ready_group_info_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // The group-management fixture (viewer 777 is an admin with
        // `can_change_info`) with the info panel open instead of the
        // member dialog.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_group_manage(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.open_info_panel_target(InfoPanelTarget::Supergroup(61), window, cx);
        self.connection
            .set_note(StatusNote::silent("screenshot demo — group info edit"));
        cx.notify();
    }
}
