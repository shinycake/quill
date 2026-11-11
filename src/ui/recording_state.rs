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
    pub(super) fn discard(self) {
        match self {
            Self::Voice(capture) => capture.discard(),
            Self::Video(capture) => capture.discard(),
        }
    }
}

/// A capture on its way up, off the UI thread. It is not a recording yet
/// (no bar, nothing to send), but no other one may start meanwhile.
/// Cancelling it goes straight back to `Idle`: a device that never answers
/// must not keep the next press of record from starting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CaptureStart {
    Idle,
    Opening {
        start: StartId,
        video: bool,
        origin: Option<ChatId>,
        purpose: CaptureFor,
    },
}

/// The system prompt for the camera or microphone, asked for one press. A
/// second press while it is up is ignored, and its answer starts the press
/// that asked only while the call that press was for is still the active
/// one (`capture_access.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PermissionAsk {
    Idle,
    Asking {
        /// Told apart from any ask that came after it.
        id: u64,
        /// The call the press was for (answer, camera, dial), if any.
        call: Option<i32>,
    },
}

/// Which start a device answer belongs to. An answer for any start but the
/// one opening now is stale (cancelled, maybe replaced) and is dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct StartId(u64);

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
    next_start: u64,
    /// The camera or microphone prompt now showing (one per press).
    pub(super) permission: PermissionAsk,
    pub(super) open_voice: Arc<dyn Fn() -> Result<VoiceCapture, String> + Send + Sync>,
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
    /// The live camera image while a video message records.
    pub(super) round_preview: std::cell::RefCell<super::round_record::RoundPreview>,
}

/// A recording still running, or a camera starting, is cancelled: its
/// file would be the old account's message.
impl super::on_screen::Farewell for RecordingUi {
    fn farewell(
        app: &mut QuillApp,
        _: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<QuillApp>,
    ) {
        if app.recording_busy() {
            app.cancel_recording(cx);
        }
    }
}

impl RecordingUi {
    /// A fresh id for the next permission ask.
    pub(super) fn next_permission_ask(&mut self) -> u64 {
        self.next_start = self.next_start.wrapping_add(1);
        self.next_start
    }

    pub(super) fn new() -> Self {
        Self {
            capture: None,
            start: CaptureStart::Idle,
            next_start: 0,
            permission: PermissionAsk::Idle,
            open_voice: Arc::new(VoiceCapture::start),
            locked: false,
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

    pub(super) fn opening_video(&self) -> bool {
        matches!(self.start, CaptureStart::Opening { video: true, .. })
    }

    pub(super) fn opening_origin(&self) -> Option<Option<ChatId>> {
        match self.start {
            CaptureStart::Opening { origin, .. } => Some(origin),
            _ => None,
        }
    }

    pub(super) fn opening_for(&self, wanted: CaptureFor) -> bool {
        matches!(self.start, CaptureStart::Opening { purpose, .. } if purpose == wanted)
    }

    pub(super) fn cancel_start(&mut self) {
        self.start = CaptureStart::Idle;
    }

    /// The device answered for `start`: true when that start is still the
    /// one wanted, which then ends. A stale answer leaves a newer start be.
    pub(super) fn finish_start(&mut self, start: StartId) -> bool {
        let current = matches!(self.start, CaptureStart::Opening { start: s, .. } if s == start);
        if current {
            self.start = CaptureStart::Idle;
        }
        current
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
            } => {
                self.end_capture_flags();
                Some(voice)
            }
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
            } => {
                self.end_capture_flags();
                Some(video)
            }
            other => {
                self.capture = Some(other);
                None
            }
        }
    }

    /// Take whatever records, with its window.
    pub(super) fn take(&mut self) -> Option<CaptureMedia> {
        let media = self.capture.take().map(|capture| capture.media);
        if media.is_some() {
            self.end_capture_flags();
        }
        media
    }

    /// Lock, the discard row, "Play once" and the camera image belong to
    /// one recording: whatever ends it (send, story reply, cancel, a closed
    /// window) ends them too, so none carries over into the next one.
    fn end_capture_flags(&mut self) {
        self.locked = false;
        self.discard_confirm = false;
        self.once = false;
        if let Ok(mut preview) = self.round_preview.try_borrow_mut() {
            preview.end();
        }
    }
}

impl QuillApp {
    pub(super) fn begin_capture_start(&mut self, video: bool, purpose: CaptureFor) -> StartId {
        let start = StartId(self.recording.next_start);
        self.recording.next_start = self.recording.next_start.wrapping_add(1);
        self.recording.start = CaptureStart::Opening {
            start,
            video,
            origin: self.windows.scope,
            purpose,
        };
        start
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
