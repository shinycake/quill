//! Floating's folder pages (Web K's W7): picking another folder tab slides
//! the old page out and the new one in, a page's width either way by tab
//! order, 200 ms on the standard curve, inside the list card. Classic
//! cuts, as before.
//!
//! The stage is the account's (`ChatListUi::folder_slide`, a
//! `page_cross::PageStage` keyed by the tab), settled where the rows draw.

use super::super::app::QuillApp;
use super::super::folder_tabs::{FolderSlotKind, selected_slot};
use super::super::motion::interface_animations_off;
use super::super::page_cross::CrossLook;
use crate::ui::chrome::PageSwitch;
use gpui_kit::*;
use std::time::Duration;

/// Web K's slider transition.
pub(in crate::ui) const FOLDER_SLIDE: CrossLook = CrossLook {
    name: "folder-slide",
    hold: "folder-slide-hold",
    duration: Duration::from_millis(200),
    old_scale: 1.,
    leaving,
    arriving,
};

/// Which way a page leaves: left going forward, right going back.
fn side(forward: bool) -> f32 {
    if forward { -1. } else { 1. }
}

/// The old page at eased time `t`: out by `t` widths.
pub(in crate::ui) fn leaving(t: f32, width: f32, forward: bool) -> SnapshotDraw {
    SnapshotDraw::moved(point(px(side(forward) * width * t), px(0.)))
}

/// The new page at eased time `t`: in from the other side.
pub(in crate::ui) fn arriving(t: f32, width: f32, forward: bool) -> SnapshotDraw {
    SnapshotDraw::moved(point(px(-side(forward) * width * (1. - t)), px(0.)))
}

impl QuillApp {
    /// The chat rows of the folder tab shown, sliding from the tab drawn
    /// last when the layout says so.
    pub(in crate::ui) fn folder_slide_rows(
        &mut self,
        rows: AnyElement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.page_switch() != PageSwitch::Cross {
            self.chat_list.folder_slide.cut();
            return rows;
        }
        let now = cx.background_executor().now();
        let slots = self.folder_slots();
        let view = &self.chat_list.view;
        let shown = slots
            .get(selected_slot(&slots, view.filter(), view.folder()))
            .map(|slot| slot.kind);
        let at = |kind: FolderSlotKind| slots.iter().position(|slot| slot.kind == kind);
        // The OS's reduce-motion preference cuts too, as a menu's pop does
        // (`with_snapshot_animation`).
        let animate = !interface_animations_off() && !cx.reduce_motion();
        let running = self.chat_list.folder_slide.settle(
            shown,
            animate,
            |from, to| Some(at(to)? > at(from)?),
            &FOLDER_SLIDE,
            window,
            now,
        );
        if running {
            self.request_app_tick(60, cx);
        }
        self.chat_list.folder_slide.draw(rows, &FOLDER_SLIDE, now)
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "folder_slide_tests.rs"]
mod tests;
