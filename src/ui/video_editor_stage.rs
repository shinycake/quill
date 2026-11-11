//! The video editor's stage: reading the file, ready to edit, or encoding
//! the edited copy. One value, so a timeline never exists without the info
//! it was cut from, the player never outlives an encode, and a late answer
//! is matched by the serial of the stage that asked.
#![deny(clippy::indexing_slicing)]

use super::video_editor::{VideoEditorDialog, next_editor_serial};
use super::video_player::VideoPlayer;
use gpui_kit::Task;
use quill::lang::{keys, tr};
use quill::video_edit::{VideoEdits, VideoSource};
use quill::video_timeline::Timeline;

pub(super) enum EditorStage {
    /// The probe with this serial is reading the file; its task is held
    /// here, so closing the editor stops waiting for it.
    Reading { serial: u64, probe: Task<()> },
    /// The probe found no video.
    Unreadable,
    /// Read: the edits, the strip's selection, the looping player and its
    /// pause, and the last failure's line. The range of `edits` is not
    /// used: the timeline holds it (`VideoEditorDialog::edits`).
    Ready {
        info: VideoSource,
        edits: VideoEdits,
        timeline: Timeline,
        looping: Looping,
        note: Option<String>,
    },
    /// The edited copy is being encoded with these edits; the encode with
    /// this serial may answer. Nothing edits meanwhile. A failure goes back
    /// to `Ready` with its line.
    Encoding {
        info: VideoSource,
        edits: VideoEdits,
        timeline: Timeline,
        serial: u64,
    },
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

    /// The edits as they stand: the stored ones with the kept range and the
    /// cover taken from the strip's selection. Before the video is read,
    /// the whole of nothing.
    pub(super) fn edits(&self) -> VideoEdits {
        match &self.stage {
            EditorStage::Ready {
                edits, timeline, ..
            }
            | EditorStage::Encoding {
                edits, timeline, ..
            } => VideoEdits {
                from_ms: timeline.from,
                till_ms: timeline.till,
                cover_ms: timeline.cover,
                ..*edits
            },
            EditorStage::Reading { .. } | EditorStage::Unreadable => VideoEdits::full(0),
        }
    }

    /// The edits to change (turns, flip, crop, quality, GIF); `None`
    /// unless the editor is ready.
    pub(super) fn edits_mut(&mut self) -> Option<&mut VideoEdits> {
        match &mut self.stage {
            EditorStage::Ready { edits, .. } => Some(edits),
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
            edits,
            timeline,
            looping: Looping::Off,
            note: None,
        };
    }

    /// Start encoding: drops the player and returns the info and the new
    /// serial that the answer must carry. `None` unless ready.
    pub(super) fn begin_encode(&mut self) -> Option<(VideoSource, u64)> {
        let EditorStage::Ready {
            info,
            edits,
            timeline,
            ..
        } = self.stage
        else {
            return None;
        };
        let serial = next_editor_serial();
        self.stage = EditorStage::Encoding {
            info,
            edits,
            timeline,
            serial,
        };
        Some((info, serial))
    }

    /// The encode failed: back to ready with the edits it tried, and its
    /// line.
    pub(super) fn encode_failed(&mut self, line: String) {
        if let EditorStage::Encoding {
            info,
            edits,
            timeline,
            ..
        } = self.stage
        {
            self.stage = EditorStage::Ready {
                info,
                edits,
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
