//! The video editor's stage: reading the file, ready to edit, or encoding
//! the edited copy. One value, so a timeline never exists without the info
//! it was cut from, the player never outlives an encode, and a late answer
//! is matched by the serial of the stage that asked.
#![deny(clippy::indexing_slicing)]

use super::video_editor::{VideoEditorDialog, next_editor_serial};
use super::video_player::VideoPlayer;
use quill::lang::{keys, tr};
use quill::video_edit::VideoSource;
use quill::video_timeline::Timeline;

pub(super) enum EditorStage {
    /// The probe with this serial is reading the file.
    Reading { serial: u64 },
    /// The probe found no video.
    Unreadable,
    /// Read: the strip's selection, the looping player (when it plays) and
    /// the last failure's line.
    Ready {
        info: VideoSource,
        timeline: Timeline,
        player: Option<VideoPlayer>,
        note: Option<String>,
    },
    /// The edited copy is being encoded; the encode with this serial may
    /// answer. A failure goes back to `Ready` with its line.
    Encoding {
        info: VideoSource,
        timeline: Timeline,
        serial: u64,
    },
}

impl EditorStage {
    pub(super) fn reading() -> Self {
        Self::Reading {
            serial: next_editor_serial(),
        }
    }
}

impl VideoEditorDialog {
    /// The serial of the probe or encode in flight, 0 when none is.
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn serial(&self) -> u64 {
        match self.stage {
            EditorStage::Reading { serial } | EditorStage::Encoding { serial, .. } => serial,
            EditorStage::Unreadable | EditorStage::Ready { .. } => 0,
        }
    }

    pub(super) fn is_reading(&self, serial: u64) -> bool {
        matches!(self.stage, EditorStage::Reading { serial: now } if now == serial)
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

    pub(super) fn set_timeline(&mut self, to: Timeline) {
        match &mut self.stage {
            EditorStage::Ready { timeline, .. } | EditorStage::Encoding { timeline, .. } => {
                *timeline = to;
            }
            EditorStage::Reading { .. } | EditorStage::Unreadable => {}
        }
    }

    /// The edited copy is being encoded.
    pub(super) fn busy(&self) -> bool {
        matches!(self.stage, EditorStage::Encoding { .. })
    }

    pub(super) fn player(&self) -> Option<&VideoPlayer> {
        match &self.stage {
            EditorStage::Ready { player, .. } => player.as_ref(),
            _ => None,
        }
    }

    pub(super) fn set_player(&mut self, to: Option<VideoPlayer>) {
        if let EditorStage::Ready { player, .. } = &mut self.stage {
            *player = to;
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

    /// The probe read `info`: the editor is ready.
    pub(super) fn become_ready(&mut self, info: VideoSource, timeline: Timeline) {
        self.stage = EditorStage::Ready {
            info,
            timeline,
            player: None,
            note: None,
        };
    }

    /// Start encoding: drops the player and returns the info and the new
    /// serial that the answer must carry. `None` unless ready.
    pub(super) fn begin_encode(&mut self) -> Option<(VideoSource, u64)> {
        let EditorStage::Ready { info, timeline, .. } = self.stage else {
            return None;
        };
        let serial = next_editor_serial();
        self.stage = EditorStage::Encoding {
            info,
            timeline,
            serial,
        };
        Some((info, serial))
    }

    /// The encode failed: back to ready, with its line.
    pub(super) fn encode_failed(&mut self, line: String) {
        if let EditorStage::Encoding { info, timeline, .. } = self.stage {
            self.stage = EditorStage::Ready {
                info,
                timeline,
                player: None,
                note: Some(line),
            };
        }
    }

    /// Tests: make the probe in flight stale, as a later one would.
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn make_probe_stale(&mut self) {
        if let EditorStage::Reading { serial } = &mut self.stage {
            *serial = next_editor_serial();
        }
    }
}
