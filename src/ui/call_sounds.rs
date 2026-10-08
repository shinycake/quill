//! Call sounds (tdesktop `Calls::Call` plays `call_incoming`,
//! `call_outgoing`, `call_connect`, `call_end` and `call_busy`). Telegram's
//! own tones can't ship in an MIT app, so Quill synthesizes its own
//! (`call_tones`) and plays them in-process through `rodio` (CoreAudio,
//! WASAPI, ALSA/PulseAudio) on every platform. The same tones play on all
//! of them for consistency. The "Play sounds" notification preference
//! silences them.

use std::num::NonZero;
use std::time::{Duration, Instant};

use rodio::buffer::SamplesBuffer;
use rodio::{DeviceSinkBuilder, MixerDeviceSink, Player};

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
            CallSound::Ringback => Duration::from_millis(2500),
            _ => Duration::from_millis(1000),
        }
    }
}

/// The one call sound playing (a new one cuts the last), and the loop to
/// keep going.
pub(super) struct CallSounds {
    /// The audio output, opened on first use; `None` until then and when
    /// the machine has no usable output device.
    output: Option<MixerDeviceSink>,
    /// When opening the output last failed, to not retry on every tick.
    open_failed_at: Option<Instant>,
    player: Option<Player>,
    looping: Option<CallSound>,
    /// When the current loop's sound may start again.
    again_at: Option<Instant>,
    /// The "Play sounds" preference.
    enabled: bool,
}

impl Default for CallSounds {
    fn default() -> Self {
        Self {
            output: None,
            open_failed_at: None,
            player: None,
            looping: None,
            again_at: None,
            enabled: true,
        }
    }
}

impl CallSounds {
    /// Follow the "Play sounds" preference; turning it off cuts any sound.
    pub(super) fn set_enabled(&mut self, enabled: bool) {
        if self.enabled && !enabled {
            self.stop_player();
        }
        self.enabled = enabled;
    }

    fn output(&mut self) -> Option<&MixerDeviceSink> {
        if self.output.is_none()
            && self
                .open_failed_at
                .is_none_or(|at| at.elapsed() > Duration::from_secs(5))
        {
            match DeviceSinkBuilder::open_default_sink() {
                Ok(mut sink) => {
                    sink.log_on_drop(false);
                    self.output = Some(sink);
                }
                Err(_) => self.open_failed_at = Some(Instant::now()),
            }
        }
        self.output.as_ref()
    }

    fn spawn(&mut self, sound: CallSound) {
        self.stop_player();
        if !self.enabled {
            return;
        }
        let Some(output) = self.output() else {
            return;
        };
        let player = Player::connect_new(output.mixer());
        player.append(SamplesBuffer::new(
            NonZero::<u16>::MIN,
            NonZero::new(call_tones::SAMPLE_RATE).unwrap_or(NonZero::<u32>::MIN),
            sound.samples(),
        ));
        self.player = Some(player);
    }

    fn stop_player(&mut self) {
        if let Some(player) = self.player.take() {
            player.stop();
        }
    }

    /// Play once, cutting whatever plays (and any loop).
    pub(super) fn play(&mut self, sound: CallSound) {
        self.looping = None;
        self.again_at = None;
        self.spawn(sound);
    }

    /// Keep `sound` looping, or stop looping with `None`. Call it often
    /// (every call tick): it restarts the loop after its gap.
    pub(super) fn keep_looping(&mut self, sound: Option<CallSound>) {
        if self.looping != sound {
            if self.looping.is_some() || sound.is_some() {
                self.stop_player();
            }
            self.looping = sound;
            self.again_at = Some(Instant::now());
        }
        let Some(sound) = self.looping else {
            return;
        };
        let finished = self.player.as_ref().is_none_or(Player::empty);
        if !finished {
            return;
        }
        match self.again_at {
            Some(at) if Instant::now() >= at => {
                self.again_at = None;
                self.spawn(sound);
            }
            Some(_) => {}
            None => self.again_at = Some(Instant::now() + sound.gap()),
        }
    }
}

/// What has played for the current call, so each cue plays once.
#[derive(Default)]
pub(super) struct SoundMarks {
    call_id: Option<i32>,
    connected: bool,
    ended: bool,
    muted: bool,
}

impl super::app::QuillApp {
    /// Ring, ring back, chime on connect and on hang-up, click on mute.
    pub(super) fn sync_call_sounds(&mut self) {
        use quill::calls::engine::TransportState;
        use quill::telegram::envelope::CallState;
        let Some(session) = self.session() else {
            return;
        };
        let (call, summary) = (session.active_call.clone(), session.call_summary.clone());
        let enabled = session.inapp_sounds_enabled;
        let marks = &mut self.call_sound_marks;
        let sounds = &mut self.call_sounds;
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
                };
                sounds.keep_looping(ringing);
                let connected = matches!(call.state, CallState::Ready)
                    && call.transport == Some(TransportState::Connected);
                if connected && !marks.connected {
                    marks.connected = true;
                    sounds.play(CallSound::Connect);
                }
                if marks.muted != call.muted {
                    marks.muted = call.muted;
                    sounds.play(if call.muted {
                        CallSound::Mute
                    } else {
                        CallSound::Unmute
                    });
                }
            }
            (None, Some(summary)) => {
                sounds.keep_looping(None);
                if marks.call_id == Some(summary.call_id) && !marks.ended {
                    marks.ended = true;
                    sounds.play(if summary.busy {
                        CallSound::Busy
                    } else {
                        CallSound::End
                    });
                }
            }
            (None, None) => sounds.keep_looping(None),
        }
    }
}
