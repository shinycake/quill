//! The "You are on mute. Click here to speak." hint (`calls::muted_speech`)
//! for the joined group call: the call microphone keeps measuring the raw
//! input while muted (`MicControls::muted_level`), and every poll feeds
//! its loudest peak to the hint on the driver's clock. The poll loop
//! wakes for the hint's next 500 ms check while it listens.
use super::*;
use crate::calls::audio_level::peak_from_level;
use crate::calls::muted_speech::{HintCall, MutedSpeechHint};
use std::time::{Duration, Instant};

/// The hint and the clock its checks run on.
pub(crate) struct MutedHint {
    hint: MutedSpeechHint,
    clock: Instant,
}

impl Default for MutedHint {
    fn default() -> Self {
        Self {
            hint: MutedSpeechHint::default(),
            clock: Instant::now(),
        }
    }
}

impl MutedHint {
    fn ms(&self, at: Instant) -> u64 {
        u64::try_from(at.saturating_duration_since(self.clock).as_millis()).unwrap_or(u64::MAX)
    }
}

impl<S: JsonSender> ConnectDriver<S> {
    /// Per poll: listen while you are muted in a joined call. Returns
    /// whether the hint appeared or went away.
    pub(crate) fn tick_muted_speech_hint(&mut self, now: Instant) -> bool {
        let call = self
            .session
            .calls
            .active_group_call
            .as_ref()
            .filter(|call| call.is_joined && !call.is_live_story)
            .map(|call| {
                let held = call.is_force_muted();
                HintCall {
                    call_id: call.id,
                    muted: call.is_muted_self && !held,
                    live: !call.is_muted_self && !held,
                }
            });
        let peak = self.call_capture.take_muted_level().map(peak_from_level);
        let now_ms = self.muted_hint.ms(now);
        self.muted_hint.hint.update(call, peak, now_ms)
    }

    /// Whether the call window shows the hint over the mute button.
    pub fn group_call_muted_speech_hint(&self) -> bool {
        self.muted_hint.hint.showing()
    }

    /// The hint's close button: it does not come back in this call.
    pub fn dismiss_group_call_muted_speech_hint(&mut self) -> bool {
        self.muted_hint.hint.dismiss()
    }

    /// When the hint's next check is due, for the poll loop.
    pub(crate) fn muted_hint_deadline(&self, now: Instant) -> Option<Duration> {
        let now_ms = self.muted_hint.ms(now);
        let at = self.muted_hint.hint.next_check_ms()?;
        Some(Duration::from_millis(at.saturating_sub(now_ms)))
    }
}
