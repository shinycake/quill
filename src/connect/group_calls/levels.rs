//! Everyone's speaking levels in the joined group call, from tgcalls'
//! per-ssrc batches (`calls::speaker_levels`). tdesktop's
//! `GroupCall::audioLevelsUpdated` maps each ssrc to a participant (0 is
//! you; your level counts only while you are live) and
//! `Data::GroupCall::applyLastSpoke` turns the last-spoke times into each
//! row's sounding and speaking. Here the driver keeps those times, tells
//! TDLib when another participant starts or stops speaking
//! (`setGroupCallParticipantIsSpeaking` with their audio source, which is
//! how TDLib orders recent speakers and loads participants it does not
//! know, as tdesktop's `resolveParticipants` does), and the call window
//! reads [`ConnectDriver::group_call_speaker`] while it draws.
//!
//! Only a change of sounding or speaking asks for a redraw; while
//! something sounds the window animates on the frame clock and reads the
//! latest level each frame. Batches that cannot change anything never
//! reach the driver (`LevelGate`, in the engine hook).
use super::*;
use crate::calls::engine::GroupAudioLevel;
use crate::calls::speaker_levels::{SpeakerLevels, SpeakerLook};
use std::time::{Duration, Instant};

/// The driver's levels for one group call, on a clock of its own.
pub(crate) struct GroupLevels {
    /// The group call these levels belong to.
    call_id: Option<i32>,
    levels: SpeakerLevels,
    clock: Instant,
}

impl Default for GroupLevels {
    fn default() -> Self {
        Self {
            call_id: None,
            levels: SpeakerLevels::default(),
            clock: Instant::now(),
        }
    }
}

impl GroupLevels {
    fn ms(&self, at: Instant) -> u64 {
        u64::try_from(at.saturating_duration_since(self.clock).as_millis()).unwrap_or(u64::MAX)
    }
}

/// An audio source as tgcalls reports it (an unsigned ssrc) and TDLib
/// names it (the same 32 bits, signed).
fn ssrc_of(audio_source_id: i32) -> u32 {
    u32::from_ne_bytes(audio_source_id.to_ne_bytes())
}

fn audio_source_of(ssrc: u32) -> i32 {
    i32::from_ne_bytes(ssrc.to_ne_bytes())
}

impl<S: JsonSender> ConnectDriver<S> {
    /// The joined call's id and your own audio ssrc (0 before the server
    /// lists you), and whether your microphone is live.
    fn levels_call(&self) -> Option<(i32, u32, bool)> {
        let call = self
            .session
            .calls
            .active_group_call
            .as_ref()
            .filter(|call| call.is_joined)?;
        let me = call.participants.iter().find(|p| p.is_current_user);
        let my_ssrc = me.map_or(0, |me| ssrc_of(me.audio_source_id));
        let live = !call.is_muted_self && !call.is_force_muted();
        Some((call.id, my_ssrc, live))
    }

    /// A level batch from the engine, applied at `now`. Returns whether a
    /// row started or stopped sounding or speaking (a redraw).
    pub(crate) fn apply_group_levels(
        &mut self,
        group_call_id: i32,
        levels: &[GroupAudioLevel],
        now: Instant,
    ) -> Result<bool, ConnectSendError> {
        let Some((call_id, my_ssrc, live)) = self.levels_call() else {
            return Ok(false);
        };
        if call_id != group_call_id {
            return Ok(false);
        }
        let mut changed = self.follow_levels_call(Some(call_id));
        // tdesktop: 0 is you, and your level is ignored while you are
        // muted (`audioLevelsUpdated` `ignore`).
        let mapped: Vec<GroupAudioLevel> = levels
            .iter()
            .filter_map(|level| {
                let ssrc = if level.ssrc == 0 { my_ssrc } else { level.ssrc };
                if ssrc == 0 {
                    return None;
                }
                let mine = ssrc == my_ssrc;
                Some(GroupAudioLevel {
                    ssrc,
                    level: if mine && !live { 0.0 } else { level.level },
                    voice: level.voice && !(mine && !live),
                })
            })
            .collect();
        let now_ms = self.group_levels.ms(now);
        changed |= self.group_levels.levels.apply(&mapped, now_ms);
        self.tell_speaking(call_id, my_ssrc, now_ms)?;
        Ok(changed)
    }

    /// Per poll: run out sounding and speaking that ended on their own,
    /// tell TDLib, and forget a call that ended. Returns whether to redraw.
    pub(crate) fn tick_group_levels(&mut self, now: Instant) -> bool {
        let call = self.levels_call();
        let mut changed = self.follow_levels_call(call.map(|(id, _, _)| id));
        let Some((call_id, my_ssrc, _)) = call else {
            return changed;
        };
        let now_ms = self.group_levels.ms(now);
        // Tell TDLib before an ssrc can be forgotten; a failed send stays
        // pending for the next poll.
        let _ = self.tell_speaking(call_id, my_ssrc, now_ms);
        changed |= self.group_levels.levels.expire(now_ms);
        changed
    }

    /// Starts over when the joined call changes. Whether rows had sound.
    fn follow_levels_call(&mut self, call_id: Option<i32>) -> bool {
        if self.group_levels.call_id == call_id {
            return false;
        }
        let now_ms = self.group_levels.ms(Instant::now());
        let had_sound = self.group_levels.levels.any_sounding(now_ms);
        self.group_levels.call_id = call_id;
        self.group_levels.levels = SpeakerLevels::default();
        had_sound
    }

    /// `setGroupCallParticipantIsSpeaking` for every other participant
    /// whose speaking changed; your own goes through `call_audio`. A
    /// change stays pending until it was sent, so a failed send is
    /// retried on the next batch or poll.
    fn tell_speaking(
        &mut self,
        call_id: i32,
        my_ssrc: u32,
        now_ms: u64,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        let changes = self
            .group_levels
            .levels
            .speaking_changes(now_ms, |ssrc| ssrc != my_ssrc);
        let mut failed = None;
        for (ssrc, speaking) in changes {
            match self.send_speaking(call_id, audio_source_of(ssrc), speaking) {
                Ok(()) => self.group_levels.levels.mark_told(ssrc, speaking),
                Err(err) => failed = Some(err),
            }
        }
        failed.map_or(Ok(()), Err)
    }

    /// What the row of the participant with `audio_source_id` shows at
    /// `now`.
    pub fn group_call_speaker(&self, audio_source_id: i32, now: Instant) -> SpeakerLook {
        if audio_source_id == 0 {
            return SpeakerLook::default();
        }
        let now_ms = self.group_levels.ms(now);
        self.group_levels
            .levels
            .look(ssrc_of(audio_source_id), now_ms)
    }

    /// Whether any row's blobs are showing, so the window keeps its frame
    /// clock running.
    pub fn group_call_levels_sounding(&self, now: Instant) -> bool {
        let now_ms = self.group_levels.ms(now);
        self.group_levels.levels.any_sounding(now_ms)
    }

    /// When a sounding or speaking state runs out next, for the poll loop.
    pub(crate) fn group_levels_deadline(&self, now: Instant) -> Option<Duration> {
        let now_ms = self.group_levels.ms(now);
        let at = self.group_levels.levels.next_change_ms(now_ms)?;
        Some(Duration::from_millis(at.saturating_sub(now_ms)))
    }
}

#[cfg(test)]
mod tests {
    use super::{audio_source_of, ssrc_of};

    #[test]
    fn audio_sources_round_trip_through_ssrcs() {
        for source in [0, 1, 4242, -1, i32::MIN, i32::MAX] {
            assert_eq!(audio_source_of(ssrc_of(source)), source);
        }
        assert_eq!(ssrc_of(-1), u32::MAX);
    }
}
