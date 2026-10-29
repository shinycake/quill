//! The 1:1 call's sound watch, the stream one's rule
//! (`calls::stream_audio::watch`) applied to a call: a connected call
//! whose sound plays through Quill but brings no frames within 5 s, or
//! whose output stops, gets Quill's output reopened, once. What the
//! watch saw goes to the call log either way, so a silent call can be
//! told apart from a silent output afterwards.
use super::*;
use crate::advanced::debug_mode::call_log;
use crate::calls::engine::StreamState;
use crate::calls::stream_audio::watch::PlaybackWatch;
use std::time::Instant;

impl<S: JsonSender> ConnectDriver<S> {
    pub(crate) fn watch_call_sound(&mut self, now: Instant) {
        let Some((call_id, connected)) = self
            .session
            .calls
            .active_call
            .as_ref()
            .map(|call| (call.id, call.transport == Some(TransportState::Connected)))
        else {
            self.call_sound_watch = None;
            return;
        };
        let Some(engine) = self.call_engine.as_deref_mut() else {
            return;
        };
        let Some(health) = engine.call_sound_health(call_id) else {
            return;
        };
        if self.call_sound_watch.as_ref().map(|(id, _)| *id) != Some(call_id) {
            self.call_sound_watch = Some((call_id, PlaybackWatch::default()));
        }
        let Some((_, watch)) = self.call_sound_watch.as_mut() else {
            return;
        };
        let state = if connected {
            StreamState::Live
        } else {
            StreamState::Connecting
        };
        let Some(reason) = watch.observe(now, state, health) else {
            return;
        };
        call_log(&format!(
            "[call] {call_id} sound: {reason:?} after connecting (frames {}, peak {}); reopening the output",
            health.frames, health.peak
        ));
        if let Err(err) = engine.reopen_call_sound(call_id) {
            call_log(&format!(
                "[call] {call_id} sound: reopening the output failed: {err}"
            ));
        }
    }

    /// The call log's last word on a call's sound, before its engine
    /// call goes away. Logged once: hanging up logs it, the later end
    /// update finds nothing left to say.
    pub(crate) fn log_call_sound_summary(&mut self, call_id: i32) {
        let watched = self
            .call_sound_watch
            .take()
            .is_some_and(|(id, _)| id == call_id);
        let health = self
            .call_engine
            .as_deref()
            .and_then(|engine| engine.call_sound_health(call_id));
        let line = match (health, watched) {
            (Some(health), _) => format!(
                "[call] {call_id} ended: {} sound frames, peak {}, output failed {}",
                health.frames, health.peak, health.output_failed
            ),
            (None, true) => format!("[call] {call_id} ended: no sound output open"),
            (None, false) => return,
        };
        call_log(&line);
    }
}
