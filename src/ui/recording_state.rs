//! Voice and round video recording.

use quill::playback::PlaybackClock;
use quill::video::VideoNoteCapture;
use quill::voice::VoiceCapture;

pub(crate) struct RecordingUi {
    /// Parity slice: in-flight notification-sound workers; capped so a
    /// message burst cannot stack players.
    /// tdesktop `VoiceRecordBar` (click the record button to record in the
    /// current mode; Cancel / Esc asks for confirmation first).
    pub(super) voice_capture: Option<VoiceCapture>,
    /// MED2: in-progress round video-note camera capture (video mode).
    pub(super) video_note_capture: Option<VideoNoteCapture>,
    /// MED2: lock-to-record — a locked recording ignores Esc; only Send
    /// or Cancel (with confirmation) ends it.
    pub(super) locked: bool,
    /// MED2: the record bar is showing the discard-confirmation row.
    pub(super) discard_confirm: bool,
    /// The paused recording is being played back (position of the preview).
    pub(super) preview: Option<PlaybackClock>,
    /// "Play once": the voice message goes out as a one-time message.
    pub(super) once: bool,
    pub(super) voice_tick: bool,
    /// A video message reached its time limit: send it next frame.
    pub(super) auto_send: bool,
    /// The live camera image while a video message records.
    pub(super) round_preview: std::cell::RefCell<super::round_record::RoundPreview>,
}

impl RecordingUi {
    pub(super) fn new() -> Self {
        Self {
            voice_capture: None,
            video_note_capture: None,
            locked: false,
            discard_confirm: false,
            preview: None,
            once: false,
            voice_tick: false,
            auto_send: false,
            round_preview: Default::default(),
        }
    }
}
