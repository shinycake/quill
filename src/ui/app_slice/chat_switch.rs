//! Floating's chat switch (Web K's W11): the new chat arrives 200 px from
//! the right and fades in over the old one, which fades out where it was,
//! 200 ms on the standard curve. Classic cuts, as before.
//!
//! While the bars float, only the history crosses: the header and composer
//! pills stay where they are and show the new chat at once, as Web K's
//! bubbles slide under a topbar and an input that do not move
//! (`floating_column`).
//!
//! The stage is the window's (`WindowSlices::chat_switch`, a
//! `page_cross::PageStage`); the slot settles it before it draws.

use super::super::app::QuillApp;
use super::super::motion::interface_animations_off;
use super::super::page_cross::CrossLook;
use crate::ui::chrome::PageSwitch;
use gpui_kit::*;
use std::time::Duration;

/// How far the new chat travels.
pub(in crate::ui) const SWITCH_TRAVEL: f32 = 200.;

/// Web K's chat transition. The old chat's picture is taken at half scale:
/// it only fades, and both pictures must fit the window's snapshot budget.
pub(in crate::ui) const CHAT_SWITCH: CrossLook = CrossLook {
    name: "chat-switch",
    hold: "chat-switch-hold",
    duration: Duration::from_millis(200),
    old_scale: 0.5,
    leaving,
    arriving,
};

/// The old chat at eased time `t`: fading out where it was. Its picture
/// holds the window's backdrop behind the chat as well, which would show a
/// seam if it moved.
pub(in crate::ui) fn leaving(t: f32, _width: f32, _forward: bool) -> SnapshotDraw {
    SnapshotDraw::IDENTITY.faded(1. - t)
}

/// The new chat at eased time `t`: from the right, fading in.
pub(in crate::ui) fn arriving(t: f32, _width: f32, _forward: bool) -> SnapshotDraw {
    SnapshotDraw::moved(point(px(SWITCH_TRAVEL * (1. - t)), px(0.))).faded(t)
}

impl QuillApp {
    /// The main window's conversation slot, crossing from the chat it drew
    /// last when the layout says so.
    pub(in crate::ui) fn chat_switch_slot(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.page_switch() != PageSwitch::Cross {
            self.slices.window.chat_switch.cut();
            return self.conversation_slot(cx);
        }
        let now = cx.background_executor().now();
        let open = self.open_chat_id();
        // The OS's reduce-motion preference cuts too, as a menu's pop does
        // (`with_snapshot_animation`).
        let animate = !interface_animations_off() && !cx.reduce_motion();
        let running = self.slices.window.chat_switch.settle(
            open,
            animate,
            |_, _| Some(true),
            &CHAT_SWITCH,
            window,
            now,
        );
        if running {
            self.request_app_tick(60, cx);
        }
        let slot = self.conversation_slot(cx);
        if self.bars_float() {
            // The floating column draws the stage around its history.
            return slot;
        }
        self.slices.window.chat_switch.draw(slot, &CHAT_SWITCH, now)
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "chat_switch_tests.rs"]
mod tests;
