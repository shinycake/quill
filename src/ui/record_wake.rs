//! When a recording's timer next wakes. A capture has real deadlines only:
//! the chat action that must be sent again before the server forgets it, the
//! round video's limit, and the end of a preview that plays. Nothing else
//! wakes it; the record bar's time and level are redrawn from the frame
//! clock while the bar is on screen (`record_bar`).
#![deny(clippy::indexing_slicing)]

use std::time::Duration;

use quill::connect::OUTGOING_TYPING_INTERVAL_MS;

/// The server drops a chat action after a few seconds, so a recording sends
/// it again this long after the last send.
pub(super) const ACTION_REFRESH: Duration = Duration::from_millis(OUTGOING_TYPING_INTERVAL_MS);

/// The encoder closes a round video's file at the limit; the check waits this
/// long past it for the file to be done.
pub(super) const LIMIT_GRACE: Duration = Duration::from_millis(500);

/// What a capture is waiting for, as the time left until each.
pub(super) struct Deadlines {
    /// A capture runs (a voice one that is not paused, or a round video):
    /// its chat action is due again after [`ACTION_REFRESH`].
    pub(super) acting: bool,
    /// The round video's limit.
    pub(super) limit: Option<Duration>,
    /// The end of the preview that plays.
    pub(super) preview_end: Option<Duration>,
}

impl Deadlines {
    /// The wait until the nearest deadline; `None` when nothing is due ever
    /// (a paused recording with no preview playing).
    pub(super) fn next(&self) -> Option<Duration> {
        [
            self.acting.then_some(ACTION_REFRESH),
            self.limit.map(|left| left + LIMIT_GRACE),
            self.preview_end,
        ]
        .into_iter()
        .flatten()
        .min()
    }
}

#[cfg(test)]
mod tests {
    use super::{ACTION_REFRESH, Deadlines, LIMIT_GRACE};
    use std::time::Duration;

    const NONE: Deadlines = Deadlines {
        acting: false,
        limit: None,
        preview_end: None,
    };

    #[test]
    fn a_running_voice_recording_wakes_for_its_chat_action_only() {
        let wake = Deadlines {
            acting: true,
            ..NONE
        };
        // Not a 100 ms step: the wait is the action's refresh.
        assert_eq!(wake.next(), Some(ACTION_REFRESH));
        assert!(ACTION_REFRESH >= Duration::from_secs(1));
    }

    #[test]
    fn a_paused_recording_with_no_preview_wakes_for_nothing() {
        assert_eq!(NONE.next(), None);
    }

    #[test]
    fn the_round_video_limit_is_the_nearest_deadline_when_it_is_close() {
        let wake = Deadlines {
            acting: true,
            limit: Some(Duration::from_secs(1)),
            ..NONE
        };
        assert_eq!(wake.next(), Some(Duration::from_secs(1) + LIMIT_GRACE));
        let far = Deadlines {
            limit: Some(Duration::from_secs(50)),
            ..wake
        };
        assert_eq!(far.next(), Some(ACTION_REFRESH));
    }

    #[test]
    fn a_playing_preview_wakes_at_its_end() {
        let wake = Deadlines {
            preview_end: Some(Duration::from_millis(700)),
            ..NONE
        };
        assert_eq!(wake.next(), Some(Duration::from_millis(700)));
    }
}
