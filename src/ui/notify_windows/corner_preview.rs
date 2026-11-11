//! The Notifications page's corner preview (tdesktop `SampleWidget`s):
//! sample windows in a corner of the real screen while a slot is hovered.

use super::{Content, DELTA_Y, FAST_ANIM_MS, MIN_HEIGHT, QuillApp};
use gpui_kit::*;
use quill::notify::custom::{self, ScreenCorner};
use std::time::Instant;

impl QuillApp {
    /// The preview ends with the page that showed it (the corner slot's
    /// hover-out cannot fire once the slot is gone): the samples go and the
    /// real notifications are lit again.
    pub(in crate::ui) fn end_corner_preview(&mut self) {
        self.hide_corner_samples_except(None, Instant::now());
    }

    /// Dim the real notifications while a corner is previewed and take the
    /// samples of every other corner away.
    fn hide_corner_samples_except(&mut self, corner: Option<ScreenCorner>, now: Instant) {
        let windows = &mut self.delivery.windows;
        let dim = if corner.is_some() { 0.0 } else { 1.0 };
        if (windows.demo.to - dim).abs() > f32::EPSILON {
            windows.demo.retarget(dim, FAST_ANIM_MS, false, now);
        }
        for (sample_corner, popup) in windows.samples.iter_mut() {
            if Some(*sample_corner) != corner {
                popup.hide(false, now);
            }
        }
    }

    /// The settings' corner preview: sample windows in `corner` (as many
    /// as the count), the real ones dimmed; `None` takes them away
    /// (tdesktop `setOverCorner` / `clearOverCorner`).
    pub(in crate::ui) fn notify_corner_preview(
        &mut self,
        corner: Option<ScreenCorner>,
        cx: &mut Context<Self>,
    ) {
        let now = Instant::now();
        self.hide_corner_samples_except(corner, now);
        if let Some(corner) = corner {
            let count = custom::clamp_count(quill::notify_prefs::current().count);
            let mut present = 0;
            for (sample_corner, popup) in self.delivery.windows.samples.iter_mut() {
                if *sample_corner == corner {
                    if present < count {
                        popup.unhide(now);
                        present += 1;
                    } else {
                        popup.hide(false, now);
                    }
                }
            }
            if let Some((display_id, area)) = self.notify_display(cx) {
                for index in present..count {
                    let shift = DELTA_Y + index as f32 * (MIN_HEIGHT + DELTA_Y);
                    self.delivery.windows.next_id += 1;
                    let id = self.delivery.windows.next_id;
                    if let Some(popup) = self.open_popup(
                        id,
                        Content::Sample,
                        MIN_HEIGHT,
                        shift,
                        corner,
                        display_id,
                        area,
                        cx,
                    ) {
                        self.delivery.windows.samples.push((corner, popup));
                    }
                }
            }
        }
        self.ensure_notify_clock(cx);
    }
}
