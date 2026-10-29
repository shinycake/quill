//! "You are on mute. Click here to speak.": the hint tdesktop sticks over
//! a group call's mute button when you talk while muted
//! (`calls_group_panel.cpp` `showStickedTooltip`, `calls_group_settings.cpp`
//! `MicLevelTester`). Every 500 ms the loudest microphone peak since the
//! last check is compared with 0.2 of full scale; three loud checks show
//! the hint, and five quiet ones after a loud one start the count over.
//! It shows once per call: unmuting or closing it ends it for the call.
//! No I/O or clocks here: the driver feeds peaks and its own milliseconds.
#![deny(clippy::indexing_slicing)]

/// tdesktop `kMicrophoneTooltipCheckInterval`.
pub const CHECK_EVERY_MS: u64 = 500;
/// tdesktop `kMicrophoneTooltipLevelThreshold`: a peak of a fifth of full
/// scale (`Webrtc::AudioInputTester`'s level).
const LOUD_PEAK: f32 = 0.2;
/// tdesktop `kMicrophoneTooltipAfterLoudCount`.
const SHOW_AFTER_LOUD: u32 = 3;
/// tdesktop `kDropLoudAfterQuietCount`.
const DROP_AFTER_QUIET: u32 = 5;

/// tdesktop `MicLevelTester`'s counting.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct MicLevelTester {
    loud: u32,
    quiet: u32,
}

impl MicLevelTester {
    /// One check with the loudest peak since the last; whether to show.
    fn check(&mut self, peak: f32) -> bool {
        if peak >= LOUD_PEAK {
            self.quiet = 0;
            self.loud += 1;
        } else if self.loud > 0 {
            self.quiet += 1;
            if self.quiet >= DROP_AFTER_QUIET {
                *self = Self::default();
            }
        }
        self.loud >= SHOW_AFTER_LOUD
    }
}

/// Where the call stands for the hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HintCall {
    pub call_id: i32,
    /// Joined, muted by yourself and free to unmute: the hint may show
    /// (tdesktop: the call is ready and not muted by an admin).
    pub muted: bool,
    /// Your microphone is live: the hint is done for this call
    /// (tdesktop `StickedTooltipHide::Activated`).
    pub live: bool,
}

/// The hint's state for the joined group call.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct MutedSpeechHint {
    call_id: Option<i32>,
    /// Listening: the counts and when the next check is due.
    tester: Option<(MicLevelTester, u64)>,
    /// The loudest peak since the last check.
    peak: f32,
    showing: bool,
    /// Unmuted or closed: not again in this call.
    done: bool,
    /// Whether the microphone was live at the last update: only a change
    /// to live ends the hint, never the state the call joined in
    /// (tdesktop `mutedValue() | rpl::skip(1)`).
    was_live: Option<bool>,
}

impl MutedSpeechHint {
    /// Follow the call and the microphone at `now_ms`. `peak` is the raw
    /// microphone's loudest sample since the last update (0 to 1 of full
    /// scale), when one was measured. Returns whether the hint appeared
    /// or went away.
    pub fn update(&mut self, call: Option<HintCall>, peak: Option<f32>, now_ms: u64) -> bool {
        let was = self.showing;
        if self.call_id != call.map(|call| call.call_id) {
            *self = Self {
                call_id: call.map(|call| call.call_id),
                ..Self::default()
            };
        }
        let live = call.map(|call| call.live);
        if self.was_live == Some(false) && live == Some(true) {
            self.done = true;
        }
        self.was_live = live;
        // Not ready, muted by an admin, unmuted or closed: stop listening
        // (an admin's mute keeps `done` as it was, tdesktop's
        // `StickedTooltipHide::Unavailable`).
        let listening = call.is_some_and(|call| call.muted) && !self.done;
        if !listening {
            self.tester = None;
            self.showing = false;
        }
        if listening && !self.showing {
            match self.tester.as_mut() {
                None => {
                    // The tester starts now: what was heard before is not counted.
                    self.tester = Some((MicLevelTester::default(), now_ms + CHECK_EVERY_MS));
                    self.peak = 0.0;
                }
                Some((tester, due)) => {
                    self.peak = self.peak.max(peak.unwrap_or(0.0));
                    if now_ms >= *due {
                        let show = tester.check(self.peak);
                        self.peak = 0.0;
                        *due = now_ms + CHECK_EVERY_MS;
                        if show {
                            self.showing = true;
                            self.tester = None;
                        }
                    }
                }
            }
        }
        was != self.showing
    }

    /// The close button: gone for this call (tdesktop
    /// `StickedTooltipHide::Discarded`).
    pub fn dismiss(&mut self) -> bool {
        let was = self.showing;
        self.showing = false;
        self.tester = None;
        self.done = true;
        was
    }

    pub fn showing(&self) -> bool {
        self.showing
    }

    /// When the next check is due, while listening.
    pub fn next_check_ms(&self) -> Option<u64> {
        self.tester.map(|(_, due)| due)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MUTED: HintCall = HintCall {
        call_id: 7,
        muted: true,
        live: false,
    };

    /// Updates every 100 ms with `peak` from `from` until the hint state
    /// changes or `until`; the time it changed.
    fn run(
        hint: &mut MutedSpeechHint,
        call: HintCall,
        peak: f32,
        from: u64,
        until: u64,
    ) -> Option<u64> {
        (from..=until)
            .step_by(100)
            .find(|&now| hint.update(Some(call), Some(peak), now))
    }

    #[test]
    fn three_loud_checks_show_the_hint() {
        let mut hint = MutedSpeechHint::default();
        assert!(!hint.update(Some(MUTED), Some(0.9), 0), "starts listening");
        assert_eq!(hint.next_check_ms(), Some(500));
        // Checks at 500, 1000 and 1500 ms.
        assert_eq!(run(&mut hint, MUTED, 0.5, 100, 3000), Some(1500));
        assert!(hint.showing());
        assert_eq!(hint.next_check_ms(), None);
    }

    #[test]
    fn quiet_talk_never_shows_it_and_pauses_reset_the_count() {
        let mut hint = MutedSpeechHint::default();
        hint.update(Some(MUTED), None, 0);
        assert_eq!(run(&mut hint, MUTED, 0.1, 100, 10_000), None);
        // Two loud checks, then five quiet ones: the count starts over.
        let mut tester = MicLevelTester::default();
        assert!(!tester.check(0.3) && !tester.check(0.3));
        for _ in 0..5 {
            assert!(!tester.check(0.0));
        }
        assert_eq!(tester, MicLevelTester::default());
        // Four quiet checks keep it.
        assert!(!tester.check(0.3) && !tester.check(0.3));
        for _ in 0..4 {
            tester.check(0.0);
        }
        assert!(tester.check(0.3));
    }

    #[test]
    fn unmuting_or_closing_ends_it_for_the_call() {
        let mut hint = MutedSpeechHint::default();
        hint.update(Some(MUTED), None, 0);
        run(&mut hint, MUTED, 0.5, 100, 2000);
        assert!(hint.showing());
        let live = HintCall {
            muted: false,
            live: true,
            ..MUTED
        };
        assert!(hint.update(Some(live), None, 2100), "unmuted: it goes");
        hint.update(Some(MUTED), None, 2200);
        assert_eq!(run(&mut hint, MUTED, 0.9, 2300, 6000), None);
        assert_eq!(hint.next_check_ms(), None, "nothing to wake for");

        // A new call starts over; closing works like unmuting.
        let next = HintCall {
            call_id: 8,
            ..MUTED
        };
        hint.update(Some(next), None, 7000);
        run(&mut hint, next, 0.9, 7100, 9000);
        assert!(hint.showing());
        assert!(hint.dismiss());
        assert_eq!(run(&mut hint, next, 0.9, 9100, 12_000), None);
    }

    #[test]
    fn joining_unmuted_does_not_end_it() {
        let mut hint = MutedSpeechHint::default();
        let live = HintCall {
            muted: false,
            live: true,
            ..MUTED
        };
        hint.update(Some(live), None, 0);
        hint.update(Some(MUTED), None, 100);
        assert_eq!(run(&mut hint, MUTED, 0.5, 200, 3000), Some(1600));
    }

    #[test]
    fn an_admin_mute_hides_it_until_you_may_speak_again() {
        let mut hint = MutedSpeechHint::default();
        hint.update(Some(MUTED), None, 0);
        run(&mut hint, MUTED, 0.5, 100, 2000);
        let held = HintCall {
            muted: false,
            live: false,
            ..MUTED
        };
        assert!(hint.update(Some(held), Some(0.9), 2100));
        assert_eq!(hint.next_check_ms(), None);
        // Allowed to speak again, still muted: it listens anew.
        hint.update(Some(MUTED), None, 3000);
        assert_eq!(run(&mut hint, MUTED, 0.5, 3100, 6000), Some(4500));
    }
}
