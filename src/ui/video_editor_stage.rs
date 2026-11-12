//! The video editor's stage: reading the file, ready to edit, or encoding
//! the edited copy. One value, so a timeline never exists without the info
//! it was cut from, the player never outlives an encode, and a late answer
//! is matched by the serial of the stage that asked.
#![deny(clippy::indexing_slicing)]

use super::video_editor::{VideoEditorDialog, next_editor_serial};
use super::video_player::VideoPlayer;
use gpui_kit::Task;
use quill::lang::{keys, tr};
use quill::video_edit::{Crop, VideoEdits, VideoSource};
use quill::video_timeline::Timeline;

pub(super) enum EditorStage {
    /// The probe with this serial is reading the file; its task is held
    /// here, so closing the editor stops waiting for it.
    Reading { serial: u64, probe: Task<()> },
    /// The probe found no video.
    Unreadable,
    /// Read: the picture edits, the strip's selection (the one place the
    /// kept range and the cover live), the looping player and its pause,
    /// and the last failure's line.
    Ready {
        info: VideoSource,
        look: Look,
        timeline: Timeline,
        looping: Looping,
        note: Option<String>,
    },
    /// The edited copy is being encoded with these edits; the encode with
    /// this serial may answer. Nothing edits meanwhile. A failure goes back
    /// to `Ready` with its line.
    Encoding {
        info: VideoSource,
        look: Look,
        timeline: Timeline,
        serial: u64,
        /// The encode's task: leaving the stage (the editor closes, the
        /// screen is replaced) stops waiting for it.
        _job: Task<()>,
    },
}

/// The edits that are not the kept range: quality, GIF, turns, flip and
/// crop. The range and the cover are the timeline's, so they are not here.
#[derive(Clone, Copy)]
pub(super) struct Look {
    pub quality: u32,
    pub gif: bool,
    pub turns: u8,
    pub flipped: bool,
    pub crop: Option<Crop>,
}

impl Look {
    fn of(edits: VideoEdits) -> Self {
        Self {
            quality: edits.quality,
            gif: edits.gif,
            turns: edits.turns,
            flipped: edits.flipped,
            crop: edits.crop,
        }
    }

    fn with_range(self, timeline: &Timeline) -> VideoEdits {
        VideoEdits {
            from_ms: timeline.from,
            till_ms: timeline.till,
            cover_ms: timeline.cover,
            quality: self.quality,
            gif: self.gif,
            turns: self.turns,
            flipped: self.flipped,
            crop: self.crop,
        }
    }
}

/// The muted loop of the kept range: not playing here, playing, or held by
/// a click on the picture (tdesktop `_userPaused`). The playhead's
/// position is the player's.
pub(super) enum Looping {
    /// No player: it could not open or it failed (the still shows).
    Off,
    Playing {
        player: VideoPlayer,
        at_ms: Option<i64>,
    },
    PausedByUser {
        player: VideoPlayer,
        at_ms: Option<i64>,
    },
}

impl VideoEditorDialog {
    /// The serial of the probe or encode in flight, 0 when none is.
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn serial(&self) -> u64 {
        match self.stage {
            EditorStage::Reading { serial, .. } | EditorStage::Encoding { serial, .. } => serial,
            EditorStage::Unreadable | EditorStage::Ready { .. } => 0,
        }
    }

    pub(super) fn is_reading(&self, serial: u64) -> bool {
        matches!(self.stage, EditorStage::Reading { serial: now, .. } if now == serial)
    }

    pub(super) fn is_encoding(&self, serial: u64) -> bool {
        matches!(self.stage, EditorStage::Encoding { serial: now, .. } if now == serial)
    }

    /// Probed size, length and sound, once read.
    pub(super) fn info(&self) -> Option<VideoSource> {
        match self.stage {
            EditorStage::Ready { info, .. } | EditorStage::Encoding { info, .. } => Some(info),
            EditorStage::Reading { .. } | EditorStage::Unreadable => None,
        }
    }

    /// The strip's selection, once the video is read.
    pub(super) fn timeline(&self) -> Option<Timeline> {
        match self.stage {
            EditorStage::Ready { timeline, .. } | EditorStage::Encoding { timeline, .. } => {
                Some(timeline)
            }
            EditorStage::Reading { .. } | EditorStage::Unreadable => None,
        }
    }

    /// A new selection; only while the editor is ready (nothing edits
    /// while the encode runs).
    pub(super) fn set_timeline(&mut self, to: Timeline) {
        if let EditorStage::Ready { timeline, .. } = &mut self.stage {
            *timeline = to;
        }
    }

    /// The edits as they stand: the picture edits with the kept range and
    /// the cover taken from the strip's selection. Before the video is
    /// read, the whole of nothing.
    pub(super) fn edits(&self) -> VideoEdits {
        match &self.stage {
            EditorStage::Ready { look, timeline, .. }
            | EditorStage::Encoding { look, timeline, .. } => look.with_range(timeline),
            EditorStage::Reading { .. } | EditorStage::Unreadable => VideoEdits::full(0),
        }
    }

    /// The picture edits to change (turns, flip, crop, quality, GIF);
    /// `None` unless the editor is ready (nothing edits while it encodes).
    pub(super) fn edits_mut(&mut self) -> Option<&mut Look> {
        match &mut self.stage {
            EditorStage::Ready { look, .. } => Some(look),
            _ => None,
        }
    }

    /// The kept range in ms of the source.
    pub(super) fn range(&self) -> (i64, i64) {
        let edits = self.edits();
        (edits.from_ms, edits.till_ms)
    }

    /// The edited copy is being encoded.
    pub(super) fn busy(&self) -> bool {
        matches!(self.stage, EditorStage::Encoding { .. })
    }

    pub(super) fn player(&self) -> Option<&VideoPlayer> {
        match &self.stage {
            EditorStage::Ready {
                looping: Looping::Playing { player, .. } | Looping::PausedByUser { player, .. },
                ..
            } => Some(player),
            _ => None,
        }
    }

    /// The player to loop with, from `at_ms`; `None` drops it (it could not
    /// open or it failed).
    pub(super) fn set_player(&mut self, to: Option<(VideoPlayer, i64)>) {
        if let EditorStage::Ready { looping, .. } = &mut self.stage {
            *looping = match to {
                Some((player, at)) => Looping::Playing {
                    player,
                    at_ms: Some(at),
                },
                None => Looping::Off,
            };
        }
    }

    /// Paused by a click on the picture.
    pub(super) fn user_paused(&self) -> bool {
        matches!(
            self.stage,
            EditorStage::Ready {
                looping: Looping::PausedByUser { .. },
                ..
            }
        )
    }

    /// A click on the picture: pause the loop, or resume it. `None` with
    /// no player.
    pub(super) fn toggle_user_pause(&mut self) -> Option<bool> {
        let EditorStage::Ready { looping, .. } = &mut self.stage else {
            return None;
        };
        let (next, paused) = match std::mem::replace(looping, Looping::Off) {
            Looping::Playing { player, at_ms } => (Looping::PausedByUser { player, at_ms }, true),
            Looping::PausedByUser { player, at_ms } => (Looping::Playing { player, at_ms }, false),
            Looping::Off => return None,
        };
        *looping = next;
        Some(paused)
    }

    /// Where the player is, for the playhead.
    pub(super) fn playback_ms(&self) -> Option<i64> {
        match &self.stage {
            EditorStage::Ready {
                looping: Looping::Playing { at_ms, .. } | Looping::PausedByUser { at_ms, .. },
                ..
            } => *at_ms,
            _ => None,
        }
    }

    pub(super) fn set_playback_ms(&mut self, to: i64) {
        if let EditorStage::Ready {
            looping: Looping::Playing { at_ms, .. } | Looping::PausedByUser { at_ms, .. },
            ..
        } = &mut self.stage
        {
            *at_ms = Some(to);
        }
    }

    /// The line under the strip.
    pub(super) fn note(&self) -> Option<String> {
        match &self.stage {
            EditorStage::Reading { .. } => Some(tr(keys::Q_AV_READING_THE_VIDEO).into()),
            EditorStage::Unreadable => Some(tr(keys::Q_AV_THIS_ISNT_A_VIDEO_QUILL).into()),
            EditorStage::Ready { note, .. } => note.clone(),
            EditorStage::Encoding { .. } => Some(tr(keys::Q_AV_PREPARING_THE_VIDEO).into()),
        }
    }

    /// Say why something failed; read editors only.
    pub(super) fn set_note(&mut self, line: String) {
        if let EditorStage::Ready { note, .. } = &mut self.stage {
            *note = Some(line);
        }
    }

    /// The probe answered: its task is the one running this, so it is let
    /// go, not cancelled from inside itself, and the editor is `Unreadable`
    /// until `become_ready` says otherwise.
    pub(super) fn end_reading(&mut self) {
        if let EditorStage::Reading { probe, .. } =
            std::mem::replace(&mut self.stage, EditorStage::Unreadable)
        {
            // Detach, not drop: dropping would cancel the task that is
            // running this very call.
            probe.detach();
        }
    }

    /// The probe read `info`: the editor is ready.
    pub(super) fn become_ready(
        &mut self,
        info: VideoSource,
        edits: VideoEdits,
        timeline: Timeline,
    ) {
        self.stage = EditorStage::Ready {
            info,
            look: Look::of(edits),
            timeline,
            looping: Looping::Off,
            note: None,
        };
    }

    /// The serial an encode started now would carry. `None` unless ready.
    pub(super) fn encode_serial(&self) -> Option<u64> {
        matches!(self.stage, EditorStage::Ready { .. }).then(next_editor_serial)
    }

    /// Start encoding: drops the player; the stage holds `job` and the
    /// serial its answer must carry (`encode_serial`). A job handed to an
    /// editor that is not ready is dropped, which cancels it.
    pub(super) fn start_encode(&mut self, serial: u64, info: VideoSource, job: Task<()>) {
        let EditorStage::Ready { look, timeline, .. } = self.stage else {
            return;
        };
        self.stage = EditorStage::Encoding {
            info,
            look,
            timeline,
            serial,
            _job: job,
        };
    }

    /// The encode failed: back to ready with the edits it tried, and its
    /// line.
    pub(super) fn encode_failed(&mut self, line: String) {
        if let EditorStage::Encoding {
            info,
            look,
            timeline,
            ..
        } = self.stage
        {
            self.stage = EditorStage::Ready {
                info,
                look,
                timeline,
                looping: Looping::Off,
                note: Some(line),
            };
        }
    }

    /// Tests: make the probe in flight stale, as a later one would.
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn make_probe_stale(&mut self) {
        if let EditorStage::Reading { serial, .. } = &mut self.stage {
            *serial = next_editor_serial();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The range and the cover come from the timeline alone; the picture
    /// edits survive the round trip through `Look`.
    #[test]
    fn the_range_is_the_timelines_and_the_rest_is_the_looks() {
        let edits = VideoEdits {
            quality: 480,
            gif: true,
            turns: 3,
            flipped: true,
            crop: Some(Crop::FULL),
            ..VideoEdits::full(10_000)
        };
        let timeline = Timeline::new(10_000, 10_000, 0, &edits);
        let moved = Timeline {
            from: 1_000,
            till: 4_000,
            cover: 2_000,
            ..timeline
        };
        let whole = Look::of(edits).with_range(&moved);
        assert_eq!(
            (whole.from_ms, whole.till_ms, whole.cover_ms),
            (1_000, 4_000, 2_000)
        );
        assert_eq!(
            (
                whole.quality,
                whole.gif,
                whole.turns,
                whole.flipped,
                whole.crop
            ),
            (480, true, 3, true, Some(Crop::FULL))
        );
    }
}
