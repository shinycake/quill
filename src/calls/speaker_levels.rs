//! Everyone's speaking levels in a group call, as tdesktop keeps them:
//! tgcalls reports a batch of per-ssrc levels every 100 ms
//! (`GroupInstanceCustomImpl::beginLevelsTimer`), `GroupCall::
//! audioLevelsUpdated` notes when each ssrc last made sound and voice, and
//! `Data::GroupCall::applyLastSpoke` keeps a participant "sounding" (the
//! userpic's blobs) and "speaking" (the green status) for
//! `kSoundStatusKeptFor` after it. No I/O here: the engine hook asks
//! [`LevelGate`] whether a batch is worth waking the driver for, and the
//! driver feeds [`SpeakerLevels`] and reads it out for the call window.
//! Times are milliseconds on the caller's clock, so tests drive them.
#![deny(clippy::indexing_slicing)]

use super::audio_level::{SOUND_STATUS_KEPT_MS, SPEAK_LEVEL_THRESHOLD};
use super::engine::GroupAudioLevel;
use std::collections::HashMap;

/// tdesktop `kBlobsEnterDuration`: the blobs fade in over this long, and
/// fade out over the last this-long of the kept-for window.
pub const BLOBS_ENTER_MS: u64 = 250;
/// tdesktop `kCheckLastSpokeInterval * 2` (`kKeepInListFor`): an ssrc
/// silent this long is forgotten.
pub const KEEP_IN_LIST_MS: u64 = 2000;
/// tgcalls' level timer: one batch per this long.
const BATCH_MS: u64 = 100;

/// Decides, on the engine's thread, whether a level batch can change
/// anything on screen. Levels at or below tdesktop's threshold change
/// nothing for an ssrc that has been quiet longer than the kept-for
/// window, so a batch of only such levels never wakes the driver.
#[derive(Debug, Default)]
pub struct LevelGate {
    /// When each ssrc was last above the threshold.
    loud_at: HashMap<u32, u64>,
}

impl LevelGate {
    /// Whether to hand `levels` (measured at `now_ms`) to the driver.
    pub fn admit(&mut self, levels: &[GroupAudioLevel], now_ms: u64) -> bool {
        let window = SOUND_STATUS_KEPT_MS + BATCH_MS;
        let mut admit = false;
        for level in levels {
            if level.level > SPEAK_LEVEL_THRESHOLD {
                self.loud_at.insert(level.ssrc, now_ms);
                admit = true;
            } else if self
                .loud_at
                .get(&level.ssrc)
                .is_some_and(|at| at + window >= now_ms)
            {
                // Still sounding: its level keeps moving the blobs, and the
                // batch past the window lets the driver see it go quiet.
                admit = true;
            }
        }
        self.loud_at.retain(|_, at| *at + window >= now_ms);
        admit
    }
}

/// One ssrc's last sound (tdesktop `Data::LastSpokeTimes`) and level.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Spoke {
    level: f32,
    /// Last level above the threshold.
    anything_ms: u64,
    /// Last such level tgcalls called voice; `None` before the first.
    voice_ms: Option<u64>,
    /// When the current sounding stretch began, for the blobs' entry.
    sounding_since_ms: u64,
    /// The speaking state TDLib was last told.
    told_speaking: bool,
}

impl Spoke {
    fn sounding(&self, now_ms: u64) -> bool {
        self.anything_ms + SOUND_STATUS_KEPT_MS >= now_ms
    }

    fn speaking(&self, now_ms: u64) -> bool {
        self.sounding(now_ms)
            && self
                .voice_ms
                .is_some_and(|at| at + SOUND_STATUS_KEPT_MS >= now_ms)
    }
}

/// What a participant row shows for one ssrc right now.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SpeakerLook {
    /// The latest level, 0 to 1.
    pub level: f32,
    /// tdesktop `sounding`: the blobs show.
    pub sounding: bool,
    /// tdesktop `speaking`: "speaking" in green.
    pub speaking: bool,
    /// The blobs' entry, 0 to 1 (`MembersRow::updateBlobAnimation`).
    pub enter: f32,
}

/// The speaking states that changed, for `setGroupCallParticipantIsSpeaking`.
pub type SpeakingChanges = Vec<(u32, bool)>;

/// Every ssrc that made sound lately, in one group call.
#[derive(Debug, Default, Clone)]
pub struct SpeakerLevels {
    by_ssrc: HashMap<u32, Spoke>,
    /// The (ssrc, sounding, speaking) states last reported as changed.
    shown: Vec<(u32, bool, bool)>,
}

impl SpeakerLevels {
    /// Apply one batch at `now_ms`. Levels at or below the threshold only
    /// move the level of an ssrc already listed (tdesktop: they update the
    /// blobs, not the last-spoke times). Returns whether any ssrc started
    /// or stopped sounding or speaking.
    pub fn apply(&mut self, levels: &[GroupAudioLevel], now_ms: u64) -> bool {
        for level in levels {
            let loud = level.level > SPEAK_LEVEL_THRESHOLD;
            match self.by_ssrc.get_mut(&level.ssrc) {
                Some(spoke) => {
                    spoke.level = level.level;
                    if loud {
                        if !spoke.sounding(now_ms) {
                            spoke.sounding_since_ms = now_ms;
                        }
                        spoke.anything_ms = now_ms;
                        if level.voice {
                            spoke.voice_ms = Some(now_ms);
                        }
                    }
                }
                None if loud => {
                    self.by_ssrc.insert(
                        level.ssrc,
                        Spoke {
                            level: level.level,
                            anything_ms: now_ms,
                            voice_ms: level.voice.then_some(now_ms),
                            sounding_since_ms: now_ms,
                            told_speaking: false,
                        },
                    );
                }
                None => {}
            }
        }
        self.refresh(now_ms)
    }

    /// Forget ssrcs quiet past [`KEEP_IN_LIST_MS`] (tdesktop
    /// `checkLastSpoke`). Returns whether any stopped sounding or
    /// speaking since the last report.
    pub fn expire(&mut self, now_ms: u64) -> bool {
        self.by_ssrc
            .retain(|_, spoke| spoke.anything_ms + KEEP_IN_LIST_MS >= now_ms);
        self.refresh(now_ms)
    }

    /// Note the states at `now_ms`; whether they differ from the last.
    fn refresh(&mut self, now_ms: u64) -> bool {
        let states = self.states(now_ms);
        if states == self.shown {
            return false;
        }
        self.shown = states;
        true
    }

    /// The speaking changes TDLib has not been told about, now marked
    /// told. Only ssrcs `tell` accepts are reported (the driver leaves out
    /// your own, which `call_audio` reports).
    pub fn take_speaking_changes(
        &mut self,
        now_ms: u64,
        tell: impl Fn(u32) -> bool,
    ) -> SpeakingChanges {
        let mut changes = Vec::new();
        for (ssrc, spoke) in &mut self.by_ssrc {
            let speaking = spoke.speaking(now_ms);
            if speaking != spoke.told_speaking && tell(*ssrc) {
                spoke.told_speaking = speaking;
                changes.push((*ssrc, speaking));
            }
        }
        // Forgotten ssrcs were silent past the kept-for window, and told
        // `false` on the check before they went.
        changes.sort_unstable_by_key(|(ssrc, _)| *ssrc);
        changes
    }

    /// What `ssrc`'s row shows at `now_ms`.
    pub fn look(&self, ssrc: u32, now_ms: u64) -> SpeakerLook {
        let Some(spoke) = self.by_ssrc.get(&ssrc) else {
            return SpeakerLook::default();
        };
        let sounding = spoke.sounding(now_ms);
        if !sounding {
            return SpeakerLook::default();
        }
        let ends = spoke.anything_ms + SOUND_STATUS_KEPT_MS;
        let entering = now_ms.saturating_sub(spoke.sounding_since_ms) as f32;
        let leaving = ends.saturating_sub(now_ms) as f32;
        let enter = (entering.min(leaving) / BLOBS_ENTER_MS as f32).clamp(0.0, 1.0);
        SpeakerLook {
            level: spoke.level.clamp(0.0, 1.0),
            sounding,
            speaking: spoke.speaking(now_ms),
            enter,
        }
    }

    /// When the next sounding or speaking state runs out on its own (the
    /// driver's poll wakes for it), or `None` with nothing sounding.
    pub fn next_change_ms(&self, now_ms: u64) -> Option<u64> {
        self.by_ssrc
            .values()
            .flat_map(|spoke| {
                let voice = spoke.voice_ms.map(|at| at + SOUND_STATUS_KEPT_MS + 1);
                [
                    Some(spoke.anything_ms + SOUND_STATUS_KEPT_MS + 1),
                    voice,
                    Some(spoke.anything_ms + KEEP_IN_LIST_MS + 1),
                ]
            })
            .flatten()
            .filter(|at| *at > now_ms)
            .min()
    }

    /// Whether any row's blobs are showing.
    pub fn any_sounding(&self, now_ms: u64) -> bool {
        self.by_ssrc.values().any(|spoke| spoke.sounding(now_ms))
    }

    /// Every listed ssrc's (sounding, speaking), sorted, to tell changes.
    fn states(&self, now_ms: u64) -> Vec<(u32, bool, bool)> {
        let mut states: Vec<_> = self
            .by_ssrc
            .iter()
            .map(|(ssrc, spoke)| (*ssrc, spoke.sounding(now_ms), spoke.speaking(now_ms)))
            .filter(|(_, sounding, _)| *sounding)
            .collect();
        states.sort_unstable_by_key(|(ssrc, _, _)| *ssrc);
        states
    }
}

/// tdesktop's row blobs (`calls_group_members_row.cpp` `RowBlobs`): two
/// layers, the inner at 0.9 of the outer's radii, scaled from
/// `kBlobScale` to 1 with the level. Returns `(inner, outer)` radii in
/// pixels for a level and the blobs' entry.
pub fn row_blob_radii(level: f32, enter: f32) -> (f32, f32) {
    const BLOB_SCALE: f32 = 0.605;
    const MINOR: f32 = 0.9;
    const MIN_RADIUS: f32 = 27.0;
    const MAX_RADIUS: f32 = 29.0;
    let level = level.clamp(0.0, 1.0);
    let enter = enter.clamp(0.0, 1.0);
    let radius = |min_scale: f32, min: f32, max: f32| {
        let scale = min_scale + (1.0 - min_scale) * level;
        (min + (max - min) * level) * scale * enter
    };
    (
        radius(BLOB_SCALE * MINOR, MIN_RADIUS * MINOR, MAX_RADIUS * MINOR),
        radius(BLOB_SCALE, MIN_RADIUS, MAX_RADIUS),
    )
}

/// tdesktop `paintScaledUserpic`: the userpic shrinks to
/// `kUserpicMinScale` (0.8) when quiet and grows back with the level,
/// blended in by the blobs' entry.
pub fn row_userpic_scale(level: f32, enter: f32) -> f32 {
    const USERPIC_MIN_SCALE: f32 = 0.8;
    let scaled = USERPIC_MIN_SCALE + (1.0 - USERPIC_MIN_SCALE) * level.clamp(0.0, 1.0);
    let enter = enter.clamp(0.0, 1.0);
    scaled * enter + (1.0 - enter)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level(ssrc: u32, level: f32, voice: bool) -> GroupAudioLevel {
        GroupAudioLevel { ssrc, level, voice }
    }

    #[test]
    fn quiet_batches_never_pass_the_gate() {
        let mut gate = LevelGate::default();
        let quiet = [level(7, 0.05, false), level(9, 0.0, false)];
        assert!(!gate.admit(&quiet, 0));
        assert!(!gate.admit(&quiet, 100));
        assert!(gate.admit(&[level(7, 0.6, true)], 200), "sound passes");
        // While 7 is kept sounding its quiet levels still pass, then stop.
        assert!(gate.admit(&quiet, 300));
        assert!(gate.admit(&quiet, 200 + SOUND_STATUS_KEPT_MS + BATCH_MS));
        assert!(!gate.admit(&quiet, 200 + SOUND_STATUS_KEPT_MS + 2 * BATCH_MS));
    }

    #[test]
    fn sounding_and_speaking_follow_tdesktops_windows() {
        let mut levels = SpeakerLevels::default();
        assert!(!levels.apply(&[level(7, 0.1, false)], 0), "below threshold");
        assert_eq!(levels.look(7, 0), SpeakerLook::default());
        assert!(
            levels.apply(&[level(7, 0.5, false)], 100),
            "starts sounding"
        );
        let look = levels.look(7, 100);
        assert!(look.sounding && !look.speaking);
        assert!(levels.apply(&[level(7, 0.8, true)], 200), "voice: speaking");
        assert!(!levels.apply(&[level(7, 0.9, true)], 300), "no change");
        assert!((levels.look(7, 300).level - 0.9).abs() < 1e-6);
        // Quiet levels move the level without extending the windows.
        assert!(!levels.apply(&[level(7, 0.0, false)], 400));
        assert!(levels.look(7, 300 + SOUND_STATUS_KEPT_MS).speaking);
        assert!(!levels.look(7, 301 + SOUND_STATUS_KEPT_MS).sounding);
        assert_eq!(levels.next_change_ms(400), Some(301 + SOUND_STATUS_KEPT_MS));
        assert!(levels.expire(301 + SOUND_STATUS_KEPT_MS), "it went quiet");
        assert!(!levels.any_sounding(301 + SOUND_STATUS_KEPT_MS));
        assert!(
            !levels.expire(301 + KEEP_IN_LIST_MS),
            "nothing more to show"
        );
        assert_eq!(levels.next_change_ms(301 + KEEP_IN_LIST_MS), None);
    }

    #[test]
    fn tdlib_hears_each_speaking_change_once() {
        let mut levels = SpeakerLevels::default();
        levels.apply(&[level(7, 0.8, true), level(5, 0.8, true)], 0);
        let tell_all = |_| true;
        assert_eq!(
            levels.take_speaking_changes(0, tell_all),
            vec![(5, true), (7, true)]
        );
        assert!(levels.take_speaking_changes(100, tell_all).is_empty());
        let later = SOUND_STATUS_KEPT_MS + 1;
        assert_eq!(
            levels.take_speaking_changes(later, |ssrc| ssrc != 5),
            vec![(7, false)],
            "your own ssrc is left to call_audio"
        );
    }

    #[test]
    fn blobs_enter_and_leave_over_a_quarter_second() {
        let mut levels = SpeakerLevels::default();
        levels.apply(&[level(7, 1.0, true)], 1000);
        assert_eq!(levels.look(7, 1000).enter, 0.0);
        assert!((levels.look(7, 1125).enter - 0.5).abs() < 1e-6);
        assert_eq!(levels.look(7, 1250).enter, 1.0);
        let ends = 1000 + SOUND_STATUS_KEPT_MS;
        assert!((levels.look(7, ends - 125).enter - 0.5).abs() < 1e-6);
    }

    #[test]
    fn blob_geometry_matches_tdesktop() {
        let (inner, outer) = row_blob_radii(1.0, 1.0);
        assert!((outer - 29.0).abs() < 1e-4 && (inner - 26.1).abs() < 1e-4);
        assert_eq!(row_blob_radii(1.0, 0.0), (0.0, 0.0));
        assert!((row_userpic_scale(0.0, 1.0) - 0.8).abs() < 1e-6);
        assert!((row_userpic_scale(0.0, 0.0) - 1.0).abs() < 1e-6);
        assert!((row_userpic_scale(1.0, 1.0) - 1.0).abs() < 1e-6);
    }
}
