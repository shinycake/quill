//! The "Disappears in" countdown on a timed photo or video in the viewer
//! (tdesktop `OverlayWidget::refreshTtlBadge`, `TtlCountdownBadge`,
//! `lng_mediaview_ttl_in`, `lng_mediaview_disappearing_*`).
//!
//! tdesktop shows a ring badge with the seconds left and appends
//! "Disappears in 12s" to the date line once the timer is running
//! (`mediaDestroyAt() > 0`). Before that the header still names the media
//! "Disappearing Photo" or "Disappearing Video". TDLib's
//! `message.self_destruct_in` is that timer.

use super::MediaViewerKind;
use crate::telegram::envelope::{MessageSelfDestruct, SelfDestructKind};

/// A running timer of a timed photo or video.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewerTtl {
    /// The message's full timer, in seconds (`self_destruct_time`).
    pub total_secs: i32,
    /// Time left, in milliseconds; `None` before the content was opened.
    pub left_ms: Option<i64>,
}

/// The countdown of a timed message, or `None` for a message that does not
/// use a timer (view-once media has no countdown).
pub fn viewer_ttl(self_destruct: &MessageSelfDestruct, now_ms: u64) -> Option<ViewerTtl> {
    let SelfDestructKind::Timer { secs } = self_destruct.kind else {
        return None;
    };
    let left_ms = (self_destruct.expires_in_ms > 0).then(|| {
        let elapsed =
            i64::try_from(now_ms.saturating_sub(self_destruct.fetched_at_ms)).unwrap_or(i64::MAX);
        self_destruct.expires_in_ms.saturating_sub(elapsed).max(0)
    });
    Some(ViewerTtl {
        total_secs: secs,
        left_ms,
    })
}

impl ViewerTtl {
    /// The seconds the badge and the date line show: rounded up, as
    /// tdesktop does (`(left + 999) / 1000`).
    pub fn seconds_left(&self) -> Option<i64> {
        self.left_ms.map(|ms| (ms + 999) / 1000)
    }

    /// How much of the ring is left, from 1 down to 0
    /// (`left / total`, `total` at least a second).
    pub fn ring_fraction(&self) -> Option<f32> {
        let total = f64::from(self.total_secs.max(1)) * 1000.0;
        self.left_ms
            .map(|ms| (ms as f64 / total).clamp(0.0, 1.0) as f32)
    }

    /// The timer is running and has not run out.
    pub fn is_running(&self) -> bool {
        self.left_ms.is_some_and(|ms| ms > 0)
    }

    /// The date line's suffix, `lng_mediaview_ttl_in` with `lng_seconds_tiny`.
    pub fn date_suffix(&self) -> Option<String> {
        self.seconds_left()
            .map(|left| format!("Disappears in {}", seconds_tiny(left)))
    }
}

/// `lng_seconds_tiny`: "5s".
pub fn seconds_tiny(seconds: i64) -> String {
    format!("{seconds}s")
}

/// The header of timed media (`lng_mediaview_disappearing_photo` and
/// `_video`).
pub fn disappearing_label(kind: MediaViewerKind) -> &'static str {
    match kind {
        MediaViewerKind::Video => "Disappearing Video",
        _ => "Disappearing Photo",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timer(secs: i32, expires_in_ms: i64, fetched_at_ms: u64) -> MessageSelfDestruct {
        MessageSelfDestruct {
            kind: SelfDestructKind::Timer { secs },
            expires_in_ms,
            fetched_at_ms,
        }
    }

    #[test]
    fn a_timer_that_has_not_started_shows_no_countdown() {
        let ttl = viewer_ttl(&timer(10, 0, 1_000), 5_000).unwrap();
        assert_eq!(ttl.total_secs, 10);
        assert_eq!(ttl.seconds_left(), None);
        assert_eq!(ttl.date_suffix(), None);
        assert_eq!(ttl.ring_fraction(), None);
        assert!(!ttl.is_running());
    }

    #[test]
    fn a_running_timer_counts_down_and_rounds_up() {
        // 8 s left when received, 1.5 s later.
        let ttl = viewer_ttl(&timer(10, 8_000, 1_000), 2_500).unwrap();
        assert_eq!(ttl.left_ms, Some(6_500));
        assert_eq!(ttl.seconds_left(), Some(7));
        assert_eq!(ttl.date_suffix().as_deref(), Some("Disappears in 7s"));
        assert!((ttl.ring_fraction().unwrap() - 0.65).abs() < 1e-6);
        assert!(ttl.is_running());
    }

    #[test]
    fn an_expired_timer_stops_at_zero() {
        let ttl = viewer_ttl(&timer(10, 2_000, 0), 9_000).unwrap();
        assert_eq!(ttl.left_ms, Some(0));
        assert_eq!(ttl.seconds_left(), Some(0));
        assert_eq!(ttl.ring_fraction(), Some(0.0));
        assert!(!ttl.is_running());
    }

    #[test]
    fn view_once_media_has_no_countdown() {
        let once = MessageSelfDestruct {
            kind: SelfDestructKind::Immediately,
            expires_in_ms: 0,
            fetched_at_ms: 0,
        };
        assert_eq!(viewer_ttl(&once, 0), None);
    }

    #[test]
    fn headers_name_the_media() {
        assert_eq!(
            disappearing_label(MediaViewerKind::Photo),
            "Disappearing Photo"
        );
        assert_eq!(
            disappearing_label(MediaViewerKind::Video),
            "Disappearing Video"
        );
    }
}
