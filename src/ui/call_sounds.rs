//! Call sounds (tdesktop `Calls::Call` plays `call_incoming`,
//! `call_outgoing`, `call_connect`, `call_end` and `call_busy`). Telegram's
//! own tones can't ship in an MIT app, so Quill synthesizes its own
//! (`call_tones`) and plays them in-process through `rodio` (CoreAudio,
//! WASAPI, ALSA/PulseAudio) on every platform. The same tones play on all
//! of them for consistency. The "Play sounds" notification preference
//! silences them.
#![deny(clippy::indexing_slicing)]

use std::num::NonZero;
use std::time::{Duration, Instant};

use super::audio::{Hold, OutputPlayer, SharedOutput};

use rodio::buffer::SamplesBuffer;

use super::call_tones;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CallSound {
    /// Someone is calling (loops).
    Incoming,
    /// The peer's phone is ringing (loops).
    Ringback,
    Connect,
    End,
    Busy,
    Mute,
    Unmute,
}

impl CallSound {
    fn samples(self) -> Vec<f32> {
        match self {
            CallSound::Incoming => call_tones::incoming(),
            CallSound::Ringback => call_tones::ringback(),
            CallSound::Connect => call_tones::connect(),
            CallSound::End => call_tones::end(),
            CallSound::Busy => call_tones::busy(),
            CallSound::Mute => call_tones::mute(),
            CallSound::Unmute => call_tones::unmute(),
        }
    }

    /// The pause before a looping sound starts again.
    fn gap(self) -> Duration {
        match self {
            CallSound::Ringback => Duration::from_millis(call_tones::RINGBACK_GAP_MS),
            _ => Duration::from_millis(call_tones::INCOMING_GAP_MS),
        }
    }
}

/// The one call sound playing (a new one cuts the last), and the loop to
/// keep going.
pub(super) struct CallSounds {
    /// The app's shared audio output (opened off the UI thread when a
    /// sound needs it).
    output: SharedOutput,
    player: Option<OutputPlayer>,
    looping: Loop,
    /// The "Play sounds" preference.
    enabled: bool,
}

/// The loop to keep going: none, or one sound with its output hold and
/// where it stands between rings.
enum Loop {
    Off,
    On {
        sound: CallSound,
        /// Keeps the output open between a loop's sounds.
        _hold: Hold,
        phase: LoopPhase,
    },
}

/// Where a loop is in one ring and the gap after it.
#[derive(Clone, Copy)]
enum LoopPhase {
    /// The ring plays (or a cue does); the gap is scheduled once it ends.
    Playing,
    /// The ring may start again at this time.
    Due(Instant),
}

impl Loop {
    fn sound(&self) -> Option<CallSound> {
        match self {
            Loop::Off => None,
            Loop::On { sound, .. } => Some(*sound),
        }
    }

    fn is_on(&self) -> bool {
        matches!(self, Loop::On { .. })
    }
}

impl CallSounds {
    pub(super) fn new(output: SharedOutput) -> Self {
        Self {
            output,
            player: None,
            looping: Loop::Off,
            enabled: true,
        }
    }
}

impl CallSounds {
    /// Follow the "Play sounds" preference; turning it off cuts any sound.
    pub(super) fn set_enabled(&mut self, enabled: bool) {
        if self.enabled && !enabled && !self.looping.is_on() {
            self.stop_player();
        }
        self.enabled = enabled;
    }

    fn spawn(&mut self, sound: CallSound) {
        self.stop_player();
        // Ringing always plays; the "Play sounds" toggle only mutes cues.
        let rings = matches!(sound, CallSound::Incoming | CallSound::Ringback);
        if !self.enabled && !rings {
            return;
        }
        let Some(outlet) = self.output.outlet() else {
            return;
        };
        let player = outlet.player();
        let samples = SamplesBuffer::new(
            NonZero::<u16>::MIN,
            NonZero::new(call_tones::SAMPLE_RATE).unwrap_or(NonZero::<u32>::MIN),
            sound.samples(),
        );
        // A cue is pointless late (an End chime over a voice note minutes
        // later, once some sound opens the device); a ring that loops is
        // stopped with its loop.
        if rings {
            player.append(samples);
        } else {
            player.append_alert(samples);
        }
        self.player = Some(player);
    }

    fn stop_player(&mut self) {
        if let Some(player) = self.player.take() {
            player.stop();
        }
    }

    /// Play once, cutting whatever plays (and any loop).
    pub(super) fn play(&mut self, sound: CallSound) {
        self.looping = Loop::Off;
        self.spawn(sound);
    }

    /// Play a cue once. A ring that loops keeps looping: the cue plays on
    /// its own and the ring comes back after it, not cut by the loop's
    /// next tick.
    pub(super) fn play_cue(&mut self, sound: CallSound) {
        let Loop::On { phase, .. } = &mut self.looping else {
            self.play(sound);
            return;
        };
        *phase = LoopPhase::Playing;
        self.spawn(sound);
    }

    /// The sound kept looping, for tests.
    #[cfg(test)]
    pub(super) fn looping(&self) -> Option<CallSound> {
        self.looping.sound()
    }

    /// Keep `sound` looping, or stop looping with `None`. Call it often
    /// (every call tick): it restarts the loop after its gap.
    pub(super) fn keep_looping(&mut self, sound: Option<CallSound>) {
        if self.looping.sound() != sound {
            if self.looping.is_on() || sound.is_some() {
                self.stop_player();
            }
            self.looping = match sound {
                Some(sound) => Loop::On {
                    sound,
                    _hold: self.output.hold(),
                    phase: LoopPhase::Due(Instant::now()),
                },
                None => Loop::Off,
            };
        }
        let Loop::On { sound, phase, .. } = &mut self.looping else {
            return;
        };
        let sound = *sound;
        // A ring whose device failed to open didn't play: the loop tries
        // again after its gap, as after one that played.
        let finished = self
            .player
            .as_ref()
            .is_none_or(|player| player.empty() || player.lost());
        if !finished {
            return;
        }
        match *phase {
            LoopPhase::Due(at) if Instant::now() >= at => {
                *phase = LoopPhase::Playing;
                self.spawn(sound);
            }
            LoopPhase::Due(_) => {}
            LoopPhase::Playing => *phase = LoopPhase::Due(Instant::now() + sound.gap()),
        }
    }
}

/// What has played for the current call, so each cue plays once.
#[derive(Default)]
pub(super) struct SoundMarks {
    call_id: Option<i32>,
    /// How far this call's cues have played.
    cue: CueStage,
    muted: bool,
}

/// The call-long cues, in the order a call reaches them: the connect chime
/// plays once, then the hang-up one.
#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum CueStage {
    #[default]
    Ringing,
    Connected,
    Ended,
}

impl super::app::QuillApp {
    /// Ring, ring back, chime on connect and on hang-up, click on mute.
    pub(super) fn sync_call_sounds(&mut self) {
        use quill::telegram::envelope::CallState;
        let Some(session) = self.session() else {
            return;
        };
        let (call, summary) = (
            session.calls.active_call.clone(),
            super::call_conference_ui::shown_call_summary(session).cloned(),
        );
        let invite_rings = session.calls.conference_invite.is_some();
        // A second call ringing in during this one rings too, the swap
        // prompt or the lock screen's card showing it.
        let second_rings = session.calls.second_call.prompt().is_some();
        let enabled = session.settings.inapp_sounds_enabled;
        let background_ring = self.background_ring_audible();
        // A call that came in reached no chat window that pressed Call: its
        // card is the main window's.
        if let Some(call) = call.as_ref()
            && !call.is_outgoing
            && self.screen.call_screen.sound_marks.call_id != Some(call.id)
        {
            self.call_boxes.card_window = None;
        }
        let marks = &mut self.screen.call_screen.sound_marks;
        let sounds = &mut self.calls.sounds;
        sounds.set_enabled(enabled);
        match (call, summary) {
            (Some(call), _) => {
                if marks.call_id != Some(call.id) {
                    *marks = SoundMarks {
                        call_id: Some(call.id),
                        muted: call.muted,
                        ..SoundMarks::default()
                    };
                }
                let ringing = match call.state {
                    CallState::Pending { .. } if !call.is_outgoing => Some(CallSound::Incoming),
                    CallState::Pending {
                        is_received: true, ..
                    } => Some(CallSound::Ringback),
                    _ => None,
                }
                .or(second_rings.then_some(CallSound::Incoming));
                sounds.keep_looping(ringing);
                let connected =
                    matches!(call.state, CallState::Ready) && call.transport.is_connected();
                if connected && marks.cue < CueStage::Connected {
                    marks.cue = CueStage::Connected;
                    sounds.play_cue(CallSound::Connect);
                }
                if marks.muted != call.muted {
                    marks.muted = call.muted;
                    sounds.play_cue(if call.muted {
                        CallSound::Mute
                    } else {
                        CallSound::Unmute
                    });
                }
            }
            // An older call's end screen does not silence a call ringing
            // on an account in the background: the summary stays until
            // its own buttons dismiss it, so the ring sounds over it.
            (None, Some(summary)) => {
                sounds.keep_looping(background_ring.then_some(CallSound::Incoming));
                if marks.call_id == Some(summary.call_id) && marks.cue < CueStage::Ended {
                    marks.cue = CueStage::Ended;
                    sounds.play(if summary.busy {
                        CallSound::Busy
                    } else {
                        CallSound::End
                    });
                }
            }
            // A conference invitation rings like an incoming call; a call
            // ringing on an account in the background rings here too
            // (`background_accounts::calls`).
            (None, None) => {
                sounds
                    .keep_looping((invite_rings || background_ring).then_some(CallSound::Incoming));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cue_over_a_ringing_loop_leaves_the_loop_set() {
        let mut sounds = CallSounds::new(SharedOutput::for_test(None));
        sounds.keep_looping(Some(CallSound::Incoming));
        sounds.play_cue(CallSound::Mute);
        assert_eq!(sounds.looping(), Some(CallSound::Incoming));
        // A plain play still cuts the loop.
        sounds.play(CallSound::End);
        assert_eq!(sounds.looping(), None);
    }
}
