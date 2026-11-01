//! Voice and round video recording.
#![deny(clippy::indexing_slicing)]

use std::sync::Arc;

use quill::ids::ChatId;
use quill::playback::PlaybackClock;
use quill::video::VideoNoteCapture;
use quill::voice::VoiceCapture;

use super::app::QuillApp;

/// What is being recorded.
pub(crate) enum CaptureMedia {
    Voice(VoiceCapture),
    Video(VideoNoteCapture),
}

impl CaptureMedia {
    /// End it and remove its file. Waits on the device and the disk, so the
    /// UI runs this off the UI thread.
    pub(super) fn discard(self) {
        match self {
            Self::Voice(capture) => capture.discard(),
            Self::Video(capture) => capture.discard(),
        }
    }
}

/// A capture on its way up, off the UI thread. It is not a recording yet
/// (no bar, nothing to send), but no other one may start meanwhile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CaptureStart {
    Idle,
    Opening {
        video: bool,
        origin: Option<ChatId>,
        purpose: CaptureFor,
    },
    /// Cancelled while opening: what arrives is dropped.
    Cancelled,
}

/// Where the recording goes once sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CaptureFor {
    /// A message in the chat of the window it was started in.
    Message,
    /// A reply to the story open in the viewer.
    StoryReply,
}

/// A recording in progress. Its window and purpose travel with it: taking
/// or dropping the capture takes them too, so a finished recording leaves
/// nothing behind for the next one to inherit. Built only by
/// [`QuillApp::hold_capture`].
pub(crate) struct Capture {
    media: CaptureMedia,
    /// The chat window it was started in (`None`: the main window). It is
    /// sent to that window's chat and ends when that window closes
    /// (`chat_window::recording`).
    origin: Option<ChatId>,
    purpose: CaptureFor,
}

pub(crate) struct RecordingUi {
    /// The voice or round video message being recorded, with the window it
    /// belongs to. One at a time, for the whole app.
    capture: Option<Capture>,
    pub(super) start: CaptureStart,
    /// Opens the microphone. Runs off the UI thread.
    pub(super) open_voice: Arc<dyn Fn() -> Result<VoiceCapture, String> + Send + Sync>,
    /// MED2: lock-to-record — a locked recording ignores Esc; only Send
    /// or Cancel (with confirmation) ends it.
    pub(super) locked: bool,
    /// The next video message starts locked (the "Record Round Message"
    /// shortcut, `startRecordingAndLock`).
    pub(super) lock_next: bool,
    /// MED2: the record bar is showing the discard-confirmation row.
    pub(super) discard_confirm: bool,
    /// The paused recording is being played back (position of the preview).
    pub(super) preview: Option<PlaybackClock>,
    /// "Play once": the voice message goes out as a one-time message.
    pub(super) once: bool,
    pub(super) voice_tick: bool,
    /// The live camera image while a video message records.
    pub(super) round_preview: std::cell::RefCell<super::round_record::RoundPreview>,
}

impl RecordingUi {
    pub(super) fn new() -> Self {
        Self {
            capture: None,
            start: CaptureStart::Idle,
            open_voice: Arc::new(VoiceCapture::start),
            locked: false,
            lock_next: false,
            discard_confirm: false,
            preview: None,
            once: false,
            voice_tick: false,
            round_preview: Default::default(),
        }
    }
}

impl RecordingUi {
    pub(super) fn active(&self) -> bool {
        self.capture.is_some()
    }

    /// A recording runs or is about to: another one must not start.
    pub(super) fn busy(&self) -> bool {
        self.capture.is_some() || self.start != CaptureStart::Idle
    }

    /// A round video's camera is being started.
    pub(super) fn opening_video(&self) -> bool {
        matches!(self.start, CaptureStart::Opening { video: true, .. })
    }

    /// The window of a capture still on its way up.
    pub(super) fn opening_origin(&self) -> Option<Option<ChatId>> {
        match self.start {
            CaptureStart::Opening { origin, .. } => Some(origin),
            _ => None,
        }
    }

    /// A capture for `wanted` is still on its way up.
    pub(super) fn opening_for(&self, wanted: CaptureFor) -> bool {
        matches!(self.start, CaptureStart::Opening { purpose, .. } if purpose == wanted)
    }

    /// The capture on its way up is no longer wanted.
    pub(super) fn cancel_start(&mut self) {
        if matches!(self.start, CaptureStart::Opening { .. }) {
            self.start = CaptureStart::Cancelled;
        }
    }

    /// The start ended: whether what it brought is still wanted.
    pub(super) fn finish_start(&mut self) -> bool {
        std::mem::replace(&mut self.start, CaptureStart::Idle) != CaptureStart::Cancelled
    }

    /// The window the recording belongs to (`Some(None)`: the main
    /// window); `None` when nothing records.
    pub(super) fn origin(&self) -> Option<Option<ChatId>> {
        self.capture.as_ref().map(|capture| capture.origin)
    }

    pub(super) fn purpose(&self) -> Option<CaptureFor> {
        self.capture.as_ref().map(|capture| capture.purpose)
    }

    pub(super) fn voice(&self) -> Option<&VoiceCapture> {
        match self.capture.as_ref().map(|capture| &capture.media) {
            Some(CaptureMedia::Voice(voice)) => Some(voice),
            _ => None,
        }
    }

    pub(super) fn voice_mut(&mut self) -> Option<&mut VoiceCapture> {
        match self.capture.as_mut().map(|capture| &mut capture.media) {
            Some(CaptureMedia::Voice(voice)) => Some(voice),
            _ => None,
        }
    }

    pub(super) fn video(&self) -> Option<&VideoNoteCapture> {
        match self.capture.as_ref().map(|capture| &capture.media) {
            Some(CaptureMedia::Video(video)) => Some(video),
            _ => None,
        }
    }

    pub(super) fn video_mut(&mut self) -> Option<&mut VideoNoteCapture> {
        match self.capture.as_mut().map(|capture| &mut capture.media) {
            Some(CaptureMedia::Video(video)) => Some(video),
            _ => None,
        }
    }

    /// Take the voice recording, with its window; nothing when a video
    /// message records.
    pub(super) fn take_voice(&mut self) -> Option<VoiceCapture> {
        match self.capture.take()? {
            Capture {
                media: CaptureMedia::Voice(voice),
                ..
            } => Some(voice),
            other => {
                self.capture = Some(other);
                None
            }
        }
    }

    /// Take the video message recording, with its window; nothing when a
    /// voice message records.
    pub(super) fn take_video(&mut self) -> Option<VideoNoteCapture> {
        match self.capture.take()? {
            Capture {
                media: CaptureMedia::Video(video),
                ..
            } => Some(video),
            other => {
                self.capture = Some(other);
                None
            }
        }
    }

    /// Take whatever records, with its window.
    pub(super) fn take(&mut self) -> Option<CaptureMedia> {
        self.capture.take().map(|capture| capture.media)
    }
}

impl QuillApp {
    /// A capture starts off the UI thread, for the window whose state is
    /// in place.
    pub(super) fn begin_capture_start(&mut self, video: bool, purpose: CaptureFor) {
        self.recording.start = CaptureStart::Opening {
            video,
            origin: self.windows.scope,
            purpose,
        };
    }

    /// Hold a recording that just started, as the window whose state is
    /// in place (the one whose composer or story viewer asked for it).
    pub(super) fn hold_capture(&mut self, media: CaptureMedia, purpose: CaptureFor) {
        self.recording.capture = Some(Capture {
            media,
            origin: self.windows.scope,
            purpose,
        });
    }
}
