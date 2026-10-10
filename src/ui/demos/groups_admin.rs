//! Screenshot demos: groups admin.

use crate::ui::app::QuillApp;
use crate::ui::groups::apply_ready_group_manage;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::state::InfoPanelTarget;
use std::sync::atomic::Ordering;

register_demos![
    // Slice G8: group info-edit demo (injected, no live Telegram):
    // the `ReadyGroupManage` demo supergroup (id 61, viewer an admin
    // with `can_change_info`) with the info panel open, so the new
    // Edit title / Edit description / Change photo rows render
    // directly.
    DemoSpec::chats("ready-group-info-edit", "screenshot demo — group info edit")
        .setup(QuillApp::demo_ready_group_info_edit),
];

impl QuillApp {
    fn demo_ready_group_info_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Slice G8: same group-management fixture (viewer 777 is an
        // admin with `can_change_info`), but open the info panel
        // instead of the member dialog so the Edit title / Edit
        // description / Change photo rows render directly.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_group_manage(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.open_info_panel_target(InfoPanelTarget::Supergroup(61), window, cx);
        self.connection.status_note = "screenshot demo — group info edit".into();
        cx.notify();
    }
}
