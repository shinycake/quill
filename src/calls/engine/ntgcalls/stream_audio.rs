//! Stream-mode calls play their sound through Quill
//! (`calls::stream_audio`): the playback description asks ntgcalls for
//! external frames, and Quill's output plays them at the stream volume.
//! Every other call keeps ntgcalls' device output.
use super::*;
use crate::calls::stream_audio::watch::PlaybackHealth;
use crate::calls::stream_audio::{StreamPlayer, gain::clamp_volume, quill_plays_streams};
use ntgcalls_sys::ntg_audio_description;

/// A stream's open output and the speaker it was opened for.
pub(super) struct StreamOutput {
    speaker: Option<String>,
    player: StreamPlayer,
}

impl NtgcallsEngine {
    /// The playback sound description of a stream Quill plays on
    /// `speaker`, opening (or reopening, for another speaker) its output.
    /// `None`: ntgcalls plays the call on the device, because it is not a
    /// stream, device playback is forced, or the output did not open.
    pub(super) fn stream_sound_description(
        &mut self,
        group_call_id: i32,
        speaker: Option<&str>,
    ) -> Option<ntg_audio_description> {
        let media = self.group_calls.get(&group_call_id)?;
        if !media.mode.is_stream() || media.device_playback || !quill_plays_streams() {
            self.close_stream_output(group_call_id);
            return None;
        }
        let (chat_id, muted) = (media.chat_id, media.playback_muted);
        let open = self
            .stream_outputs
            .get(&group_call_id)
            .is_some_and(|output| output.speaker.as_deref() == speaker);
        if !open {
            let name = self.speaker_name(speaker);
            match StreamPlayer::open(name.as_deref(), self.stream_volume, muted) {
                Ok(player) => {
                    self.callback
                        .route_stream_audio(chat_id, Some(player.audio().clone()));
                    self.stream_outputs.insert(
                        group_call_id,
                        StreamOutput {
                            speaker: speaker.map(str::to_owned),
                            player,
                        },
                    );
                }
                Err(err) => {
                    eprintln!(
                        "quill: warning: stream sound output did not open ({err}); ntgcalls plays group call {group_call_id} on the device"
                    );
                    self.force_device_playback(group_call_id);
                    return None;
                }
            }
        }
        let audio = self.stream_outputs.get(&group_call_id)?.player.audio();
        audio.gain().set_muted(muted);
        let format = audio.format();
        Some(ntg_audio_description {
            media_source: NTG_MEDIA_SOURCE_EXTERNAL,
            sample_rate: format.rate,
            channel_count: format.channels,
            input: null_mut(),
            keep_open: false,
        })
    }

    /// From now on ntgcalls plays this stream on the device; Quill's
    /// output closes.
    pub(super) fn force_device_playback(&mut self, group_call_id: i32) {
        if let Some(media) = self.group_calls.get_mut(&group_call_id) {
            media.device_playback = true;
        }
        self.close_stream_output(group_call_id);
    }

    pub(super) fn close_stream_output(&mut self, group_call_id: i32) {
        if self.stream_outputs.remove(&group_call_id).is_none() {
            return;
        }
        if let Some(media) = self.group_calls.get(&group_call_id) {
            self.callback.route_stream_audio(media.chat_id, None);
        }
    }

    /// A mute or unmute of a stream Quill plays: the gain only, so the
    /// output and the frame count go on. `false` when ntgcalls plays it.
    pub(super) fn mute_stream_sound(&self, group_call_id: i32, muted: bool) -> bool {
        let Some(output) = self.stream_outputs.get(&group_call_id) else {
            return false;
        };
        output.player.audio().gain().set_muted(muted);
        true
    }

    pub(super) fn apply_stream_volume(&mut self, volume: f32) {
        self.stream_volume = clamp_volume(volume);
        for output in self.stream_outputs.values() {
            output.player.audio().gain().set_volume(self.stream_volume);
        }
    }

    pub(super) fn stream_health(&self, group_call_id: i32) -> Option<PlaybackHealth> {
        self.stream_outputs
            .get(&group_call_id)
            .map(|output| output.player.audio().health())
    }

    /// The self-healing fallback: the stream goes to ntgcalls' device
    /// playback, re-issued at once when it is connected.
    pub(super) fn fall_back_to_device(&mut self, group_call_id: i32) -> Result<(), EngineError> {
        self.force_device_playback(group_call_id);
        let connected = self
            .group_calls
            .get(&group_call_id)
            .ok_or(EngineError::NoSuchCall(group_call_id))?
            .connected;
        if connected {
            self.issue_group_playback(group_call_id)
        } else {
            Ok(())
        }
    }

    /// The name cpal knows a speaker by, from its ntgcalls id. `None`
    /// (the system default) for no speaker or an unlisted one.
    fn speaker_name(&self, speaker: Option<&str>) -> Option<String> {
        let id = speaker?;
        self.media_devices()
            .ok()?
            .into_iter()
            .find(|device| device.kind == MediaDeviceKind::Speaker && device.id == id)
            .map(|device| device.name)
    }
}
