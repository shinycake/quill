//! The viewer's side of a timed photo or video (tdesktop
//! `OverlayWidget::refreshTtlBadge`, `markTimedMediaRead`): the ring badge
//! with the seconds left, starting the timer when an incoming message is
//! opened, and keeping save, copy and forward away from the media.

use super::*;
use quill::lang::{keys, tr_with};
use quill::media_viewer::{ViewerTtl, viewer_ttl};
use quill::state::unix_ms_now;
use std::f32::consts::{FRAC_PI_2, TAU};

/// `ttlMediaBadgeSize`, `ttlMediaBadgeLine` (tdesktop `ttl_media.style`).
const BADGE: f32 = 44.;
const LINE: f32 = 2.;
/// The ring sits `2 * line` inside the disc.
const RING_INSET: f32 = 2. * LINE;
/// `radialBg` is `#00000056`.
const BADGE_BG: f32 = 0x56 as f32 / 255.;
/// Redraw rate of the shrinking ring.
const RING_FPS: u32 = 15;

impl QuillApp {
    /// The self-destruct timer of the open item, when it has one.
    fn viewer_message_timer(&self) -> Option<quill::telegram::envelope::MessageSelfDestruct> {
        if self.viewer.state.source() == ViewerSource::Profile {
            return None;
        }
        let item = self.viewer.state.current()?;
        self.session()?
            .histories
            .get(&item.chat_id.0)?
            .messages
            .get(&item.message_id.0)?
            .self_destruct
    }

    /// The open item is a timed photo or video.
    pub(in crate::ui) fn viewer_is_timed(&self) -> bool {
        self.viewer_message_timer()
            .and_then(|sd| viewer_ttl(&sd, unix_ms_now()))
            .is_some()
    }

    /// The countdown of the open item.
    pub(super) fn viewer_ttl(&self) -> Option<ViewerTtl> {
        viewer_ttl(&self.viewer_message_timer()?, unix_ms_now())
    }

    /// Opening an incoming timed message starts its timer
    /// (`openMessageContent`; tdesktop `markTimedMediaRead`). Your own
    /// messages and timers that already run are left alone.
    pub(super) fn mark_timed_media_read(&mut self, chat_id: ChatId, message_id: MessageId) {
        let start = self
            .session()
            .and_then(|s| s.histories.get(&chat_id.0)?.messages.get(&message_id.0))
            .is_some_and(|m| {
                !m.is_outgoing && m.self_destruct.is_some_and(|sd| sd.expires_in_ms <= 0)
            });
        if !start {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.open_voice_content(chat_id, message_id);
        }
    }

    /// The ring badge: a dimmed disc, a white arc for the time left that
    /// shrinks counterclockwise from the top, and the seconds in the middle.
    pub(super) fn viewer_ttl_badge(&self, ttl: ViewerTtl) -> Option<AnyElement> {
        let left = ttl.seconds_left()?;
        let fraction = ttl.ring_fraction()?;
        let ring = canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let center = bounds.center();
                let radius = BADGE / 2. - RING_INSET;
                if fraction <= 0. {
                    return;
                }
                let steps = ((fraction * 120.).ceil() as usize).max(2);
                let points: Vec<Point<Pixels>> = (0..=steps)
                    .map(|step| {
                        // Counterclockwise from the top (Qt's positive angle).
                        let angle = -FRAC_PI_2 - TAU * fraction * step as f32 / steps as f32;
                        point(
                            center.x + px(radius * angle.cos()),
                            center.y + px(radius * angle.sin()),
                        )
                    })
                    .collect();
                let mut path = PathBuilder::stroke(px(LINE));
                path.add_polygon(&points, false);
                if let Ok(path) = path.build() {
                    window.paint_path(path, gpui_kit::white());
                }
            },
        )
        .absolute()
        .inset_0();
        Some(
            div()
                .id("media-viewer-ttl-badge")
                .absolute()
                .top(px(VIEWER_TOP_BAR + 12.))
                .right_4()
                .size(px(BADGE))
                .rounded_full()
                .bg(gpui_kit::black().opacity(BADGE_BG))
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(gpui_kit::white())
                .role(Role::Status)
                .aria_label(tr_with(
                    keys::MEDIAVIEW_TTL_IN,
                    &[(
                        "time",
                        &(quill::media_viewer::seconds_tiny(left)).to_string(),
                    )],
                ))
                .child(ring)
                .child(quill::media_viewer::seconds_tiny(left))
                .into_any_element(),
        )
    }

    /// Keep the ring moving while a countdown runs and the window shows.
    pub(super) fn tick_viewer_ttl(&self, ttl: Option<ViewerTtl>, cx: &mut Context<Self>) {
        if ttl.is_some_and(|ttl| ttl.is_running()) {
            self.request_animation_tick(RING_FPS, cx);
        }
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "ttl_ui_tests.rs"]
mod tests;
