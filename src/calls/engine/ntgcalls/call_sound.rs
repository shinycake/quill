//! A 1:1 call's sound played by Quill (`calls::stream_audio`) instead of
//! ntgcalls' device output, so Quill knows whether the other side's sound
//! arrives at all. The driver's watch (`connect::calls::sound_watch`)
//! moves a call that gets no sound, or whose output stops, back to the
//! device, once, and writes what it saw to the call log.
use super::stream_audio::{SoundRoute, StreamOutput, route_stream_sound};
use super::*;
use crate::calls::stream_audio::watch::PlaybackHealth;
use crate::calls::stream_audio::{StreamPlayer, quill_plays_streams};
use ntgcalls_sys::ntg_audio_description;

impl NtgcallsEngine {
    /// The playback description for call `call_id`'s sound: Quill's own
    /// output for `speaker` when it plays the call, `None` for the device.
    pub(super) fn open_call_sound(
        &mut self,
        call_id: i32,
        user_id: i64,
        speaker: Option<&str>,
    ) -> Option<ntg_audio_description> {
        let quill_plays = quill_plays_streams() && !self.device_call_sound.contains(&call_id);
        let open_for_speaker = self
            .call_outputs
            .get(&call_id)
            .is_some_and(|output| output.speaker.as_deref() == speaker);
        let route = route_stream_sound(quill_plays, open_for_speaker, || {
            let name = self.cpal_speaker_name(speaker);
            StreamPlayer::open(name.as_deref(), 1.0, false)
        });
        match route {
            SoundRoute::Device => {
                self.close_call_sound(call_id, user_id);
                return None;
            }
            SoundRoute::OpenFailed(err) => {
                crate::advanced::debug_mode::call_log(&format!(
                    "[call] {call_id} sound output did not open ({err}); playing on the device"
                ));
                self.device_call_sound.insert(call_id);
                self.close_call_sound(call_id, user_id);
                return None;
            }
            SoundRoute::Opened(player) => {
                self.callback
                    .route_stream_audio(user_id, Some(player.audio().clone()));
                self.call_outputs.insert(
                    call_id,
                    StreamOutput {
                        speaker: speaker.map(str::to_owned),
                        player,
                    },
                );
            }
            SoundRoute::Keep => {}
        }
        let format = self.call_outputs.get(&call_id)?.player.audio().format();
        Some(ntg_audio_description {
            media_source: NTG_MEDIA_SOURCE_EXTERNAL,
            sample_rate: format.rate,
            channel_count: format.channels,
            input: null_mut(),
            keep_open: false,
        })
    }

    pub(super) fn close_call_sound(&mut self, call_id: i32, user_id: i64) {
        if self.call_outputs.remove(&call_id).is_some() {
            self.callback.route_stream_audio(user_id, None);
        }
    }

    pub(super) fn call_health(&self, call_id: i32) -> Option<PlaybackHealth> {
        self.call_outputs
            .get(&call_id)
            .map(|output| output.player.audio().health())
    }

    /// The self-healing fallback: call `call_id` plays on the device
    /// from now on.
    pub(super) fn call_sound_to_device(&mut self, call_id: i32) -> Result<(), EngineError> {
        let user_id = self.user_id(call_id)?;
        self.device_call_sound.insert(call_id);
        self.close_call_sound(call_id, user_id);
        self.set_media_sources(call_id)
    }
}
