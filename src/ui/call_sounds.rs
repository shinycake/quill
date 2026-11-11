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
    voice: Voice,
    /// The "Play sounds" preference.
    enabled: bool,
}

/// What the call sounds are doing: one value, so the player belongs to the
/// stage that started it.
enum Voice {
    /// No loop. `cue` is the one-shot still playing, if any.
    Idle { cue: Option<OutputPlayer> },
    /// One sound loops, with its output hold and where it stands between
    /// rings.
    Looping {
        sound: CallSound,
        /// Keeps the output open between a loop's sounds.
        _hold: Hold,
        stage: RingStage,
    },
}

/// Where a loop is in one ring and the gap after it.
enum RingStage {
    /// The ring plays (or a cue does, or its device failed to open and
    /// nothing plays); the gap is scheduled once it ends.
    Playing(Option<OutputPlayer>),
    /// The ring may start again at this time.
    Due(Instant),
}

impl Voice {
    fn sound(&self) -> Option<CallSound> {
        match self {
            Voice::Idle { .. } => None,
            Voice::Looping { sound, .. } => Some(*sound),
        }
    }

    fn is_looping(&self) -> bool {
        matches!(self, Voice::Looping { .. })
    }

    /// Cut whatever plays and leave the stage idle.
    fn stop(&mut self) {
        let old = std::mem::replace(self, Voice::Idle { cue: None });
        let player = match old {
            Voice::Idle { cue } => cue,
            Voice::Looping { stage, .. } => match stage {
                RingStage::Playing(player) => player,
                RingStage::Due(_) => None,
            },
        };
        if let Some(player) = player {
            player.stop();
        }
    }
}

impl CallSounds {
    pub(super) fn new(output: SharedOutput) -> Self {
        Self {
            output,
            voice: Voice::Idle { cue: None },
            enabled: true,
        }
    }
}

impl CallSounds {
    /// Follow the "Play sounds" preference; turning it off cuts any sound.
    pub(super) fn set_enabled(&mut self, enabled: bool) {
        if self.enabled && !enabled && !self.voice.is_looping() {
            self.voice.stop();
        }
        self.enabled = enabled;
    }

    /// Whether `sound` is audible: ringing always plays, the "Play sounds"
    /// toggle only mutes cues.
    fn audible(&self, sound: CallSound) -> bool {
        self.enabled || matches!(sound, CallSound::Incoming | CallSound::Ringback)
    }

    /// Start `sound` on a fresh player; `None` when it is muted or the
    /// output is not there. It cuts nothing by itself: the caller decides
    /// what the old player's stage becomes.
    fn spawn(&mut self, sound: CallSound) -> Option<OutputPlayer> {
        let rings = matches!(sound, CallSound::Incoming | CallSound::Ringback);
        let outlet = self.output.outlet()?;
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
        Some(player)
    }

    /// Play once, cutting whatever plays (and any loop).
    pub(super) fn play(&mut self, sound: CallSound) {
        self.voice.stop();
        if self.audible(sound) {
            self.voice = Voice::Idle {
                cue: self.spawn(sound),
            };
        }
    }

    /// Play a cue once. A ring that loops keeps looping: the cue plays on
    /// its own and the ring comes back after it, not cut by the loop's
    /// next tick. A cue that is muted leaves the ring as it is.
    pub(super) fn play_cue(&mut self, sound: CallSound) {
        if !self.voice.is_looping() {
            self.play(sound);
            return;
        }
        if !self.audible(sound) {
            return;
        }
        let player = self.spawn(sound);
        if let Voice::Looping { stage, .. } = &mut self.voice
            && let RingStage::Playing(Some(old)) =
                std::mem::replace(stage, RingStage::Playing(player))
        {
            old.stop();
        }
    }

    /// The sound kept looping, for tests.
    #[cfg(test)]
    pub(super) fn looping(&self) -> Option<CallSound> {
        self.voice.sound()
    }

    /// Keep `sound` looping, or stop looping with `None`. Call it often
    /// (every call tick): it restarts the loop after its gap.
    pub(super) fn keep_looping(&mut self, sound: Option<CallSound>) {
        if self.voice.sound() != sound {
            if self.voice.is_looping() || sound.is_some() {
                self.voice.stop();
            }
            if let Some(sound) = sound {
                self.voice = Voice::Looping {
                    sound,
                    _hold: self.output.hold(),
                    stage: RingStage::Due(Instant::now()),
                };
            }
        }
        let Voice::Looping { sound, stage, .. } = &self.voice else {
            return;
        };
        let sound = *sound;
        // A ring whose device failed to open didn't play: the loop tries
        // again after its gap, as after one that played.
        let ring_due = match stage {
            RingStage::Playing(player) => {
                let finished = player
                    .as_ref()
                    .is_none_or(|player| player.empty() || player.lost());
                if !finished {
                    return;
                }
                false
            }
            RingStage::Due(at) => {
                if Instant::now() < *at {
                    return;
                }
                true
            }
        };
        let next = if ring_due {
            RingStage::Playing(self.spawn(sound))
        } else {
            RingStage::Due(Instant::now() + sound.gap())
        };
        if let Voice::Looping { stage, .. } = &mut self.voice {
            *stage = next;
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
                    matches!(call.state, CallState::Ready(_)) && call.transport.is_connected();
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
