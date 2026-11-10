//! Boxes that sent a request and wait for TDLib's answer (`Submit`):
//! each leaves its Sending stage on the answer (it closes) or on the
//! error (it stays, with the reason). One pump, called from `poll_live`,
//! settles them all; a new box adds its settle call here.
use super::app::QuillApp;
use gpui_kit::*;

impl QuillApp {
    pub(in crate::ui) fn settle_box_submits(&mut self, cx: &mut Context<Self>) {
        self.settle_moderation_submits(cx);
        self.settle_username_dialog(cx);
        self.settle_create_community_dialog(cx);
        self.settle_topic_editor(cx);
    }
}
