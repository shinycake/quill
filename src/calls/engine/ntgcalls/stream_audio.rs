use super::*;
use crate::calls::stream_audio::watch::PlaybackHealth;
use crate::calls::stream_audio::{StreamPlayer, gain::clamp_volume, quill_plays_streams};
use ntgcalls_sys::ntg_audio_description;

pub(super) struct StreamOutput {
    pub(super) speaker: Option<String>,
    pub(super) player: StreamPlayer,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum SoundRoute<P> {
    Device,
    Keep,
    Opened(P),
    /// The output did not open: device playback from now on.
    OpenFailed(String),
}

/// Where a stream join's sound goes. `open` runs only when Quill plays
/// the stream and no output is open for the call's speaker.
pub(super) fn route_stream_sound<P>(
    quill_plays: bool,
    open_for_speaker: bool,
    open: impl FnOnce() -> Result<P, String>,
) -> SoundRoute<P> {
    if !quill_plays {
        SoundRoute::Device
    } else if open_for_speaker {
        SoundRoute::Keep
    } else {
        match open() {
            Ok(player) => SoundRoute::Opened(player),
            Err(err) => SoundRoute::OpenFailed(err),
        }
    }
}

impl NtgcallsEngine {
    pub(super) fn open_stream_sound(
        &mut self,
        group_call_id: i32,
        speaker: Option<&str>,
    ) -> Option<ntg_audio_description> {
        let media = self.group_calls.get(&group_call_id)?;
        let quill_plays =
            media.mode.is_stream() && !media.forced_device_playback && quill_plays_streams();
        let (chat_id, muted) = (media.chat_id, media.playback_muted);
        let open_for_speaker = self
            .stream_outputs
            .get(&group_call_id)
            .is_some_and(|output| output.speaker.as_deref() == speaker);
        let route = route_stream_sound(quill_plays, open_for_speaker, || {
            let name = self.cpal_speaker_name(speaker);
            StreamPlayer::open(name.as_deref(), self.stream_volume, muted)
        });
        match route {
            SoundRoute::Device => {
                self.close_stream_output(group_call_id);
                return None;
            }
            SoundRoute::OpenFailed(err) => {
                eprintln!(
                    "quill: warning: stream sound output did not open ({err}); ntgcalls plays group call {group_call_id} on the device"
                );
                self.force_device_playback(group_call_id);
                return None;
            }
            SoundRoute::Opened(player) => {
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
            SoundRoute::Keep => {}
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

    pub(super) fn force_device_playback(&mut self, group_call_id: i32) {
        if let Some(media) = self.group_calls.get_mut(&group_call_id) {
            media.forced_device_playback = true;
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

    pub(super) fn try_mute_quill_stream(&self, group_call_id: i32, muted: bool) -> bool {
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

    pub(super) fn cpal_speaker_name(&self, speaker: Option<&str>) -> Option<String> {
        let id = speaker?;
        self.media_devices()
            .ok()?
            .into_iter()
            .find(|device| device.kind == MediaDeviceKind::Speaker && device.id == id)
            .map(|device| device.name)
    }
}

#[cfg(test)]
mod tests {
    use super::{SoundRoute, route_stream_sound};

    #[test]
    fn an_output_that_does_not_open_sends_the_stream_to_the_device() {
        let route: SoundRoute<()> = route_stream_sound(true, false, || Err("no device".to_owned()));
        assert_eq!(route, SoundRoute::OpenFailed("no device".to_owned()));
    }

    #[test]
    fn the_output_opens_only_when_quill_plays_and_none_is_open() {
        let opened = std::cell::Cell::new(0);
        let open = || {
            opened.set(opened.get() + 1);
            Ok(())
        };
        assert_eq!(route_stream_sound(false, false, open), SoundRoute::Device);
        assert_eq!(route_stream_sound(true, true, open), SoundRoute::Keep);
        assert_eq!(opened.get(), 0);
        assert_eq!(
            route_stream_sound(true, false, open),
            SoundRoute::Opened(())
        );
        assert_eq!(opened.get(), 1);
    }
}
