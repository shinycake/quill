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

/// The one recording lifecycle of the app: a press starts it one stage at a
/// time (the system prompt for the camera or microphone, then the capture
/// coming up off the UI thread), and it ends as a recording in progress.
/// Asking and Opening are not a recording yet (no bar, nothing to send), but
/// no other press may start meanwhile. Each starting stage holds the task
/// that waits for its answer, so cancelling goes straight back to `Idle` and
/// drops the wait: a device that never answers must not keep the next press
/// from starting. A recording and a start cannot both exist.
pub(super) enum RecordStage {
    Idle,
    /// The system prompt for the camera or microphone is up for one press.
    /// Its answer starts the press that asked only while the call that
    /// press was for is still the active one (`capture_access.rs`).
    Asking {
        /// Told apart from any start that came after it.
        id: StartId,
        _wait: gpui_kit::Task<()>,
    },
    Opening {
        start: StartId,
        video: bool,
        origin: Option<ChatId>,
        purpose: CaptureFor,
        _opening: gpui_kit::Task<()>,
    },
    /// The voice or round video message being recorded, with the window it
    /// belongs to. One at a time, for the whole app.
    Recording(Capture),
}

/// Which start a device answer belongs to. An answer for any start but the
/// one waiting now is stale (cancelled, maybe replaced) and is dropped.
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
    /// Lock, the discard row, "Play once" and the preview of a paused
    /// voice message belong to this recording: whatever ends it (send,
    /// story reply, cancel, a closed window) drops them with it, so none
    /// carries over into the next one.
    mode: RecordMode,
    discard: DiscardRow,
    delivery: Delivery,
    /// The paused recording is being played back (position of the
    /// preview).
    preview: Option<PlaybackClock>,
    /// The live camera image while a video message records.
    round: super::round_record::RoundPreview,
    /// The wake-up for the capture's own deadlines: it belongs to the
    /// recording, so whatever ends it ends the wake-up too, and it runs while
    /// no window draws (a background recording keeps its chat action,
    /// reports a failed device and sends a round video at its limit).
    wake: RecordWake,
}

/// MED2: lock-to-record — a locked recording ignores Esc; only Send or
/// Cancel (with confirmation) ends it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecordMode {
    Held,
    Locked,
}

/// MED2: the record bar's discard-confirmation row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiscardRow {
    Hidden,
    Confirming,
}

/// "Play once": the voice message goes out as a one-time message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Delivery {
    Normal,
    PlayOnce,
}

pub(crate) struct RecordingUi {
    stage: RecordStage,
    next_start: u64,
    pub(super) open_voice: Arc<dyn Fn() -> Result<VoiceCapture, String> + Send + Sync>,
    /// What the OS says about the microphone and camera, and its prompt.
    pub(super) device_os: super::capture_access::DeviceOs,
}

/// The recording's wake-up: not armed, or the task that runs the next one,
/// with the instant it is due (a deadline of the capture, `record_wake.rs`).
/// The recording holds it, so it ends with the recording.
enum RecordWake {
    Stopped,
    Armed {
        /// Read by the tests that watch when the wake-up falls.
        #[cfg_attr(not(test), allow(dead_code))]
        due: std::time::Instant,
        _task: gpui_kit::Task<()>,
    },
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
    /// A fresh id for the next start.
    pub(super) fn next_start_id(&mut self) -> StartId {
        let id = StartId(self.next_start);
        self.next_start = self.next_start.wrapping_add(1);
        id
    }

    /// The prompt for `id` is up, waited on by `wait`.
    /// A recording in progress is not replaced: the wait is dropped.
    pub(super) fn begin_asking(&mut self, id: StartId, wait: gpui_kit::Task<()>) {
        if self.capture().is_none() {
            self.stage = RecordStage::Asking { id, _wait: wait };
        }
    }

    /// The capture for `start` is coming up, waited on by `opening`.
    pub(super) fn begin_opening(
        &mut self,
        start: StartId,
        video: bool,
        origin: Option<ChatId>,
        purpose: CaptureFor,
        opening: gpui_kit::Task<()>,
    ) {
        if self.capture().is_none() {
            self.stage = RecordStage::Opening {
                start,
                video,
                origin,
                purpose,
                _opening: opening,
            };
        }
    }

    /// The prompt answered for `id`: true when that prompt is still the one
    /// waited for, which then ends.
    pub(super) fn finish_asking(&mut self, id: StartId) -> bool {
        let current = matches!(self.stage, RecordStage::Asking { id: asked, .. } if asked == id);
        if current {
            self.stage = RecordStage::Idle;
        }
        current
    }

    /// A press is starting: waiting on the prompt, or on the capture to
    /// open.
    pub(super) fn starting(&self) -> bool {
        matches!(
            self.stage,
            RecordStage::Asking { .. } | RecordStage::Opening { .. }
        )
    }

    /// A start is waiting on the prompt: another press must not wipe it.
    pub(super) fn asking(&self) -> bool {
        matches!(self.stage, RecordStage::Asking { .. })
    }

    pub(super) fn new() -> Self {
        Self {
            stage: RecordStage::Idle,
            next_start: 0,
            open_voice: Arc::new(VoiceCapture::start),
            device_os: Default::default(),
        }
    }
}

impl RecordingUi {
    fn capture(&self) -> Option<&Capture> {
        match &self.stage {
            RecordStage::Recording(capture) => Some(capture),
            _ => None,
        }
    }

    fn capture_mut(&mut self) -> Option<&mut Capture> {
        match &mut self.stage {
            RecordStage::Recording(capture) => Some(capture),
            _ => None,
        }
    }

    /// Take the recording out, leaving the stage idle.
    fn take_capture(&mut self) -> Option<Capture> {
        match std::mem::replace(&mut self.stage, RecordStage::Idle) {
            RecordStage::Recording(capture) => Some(capture),
            other => {
                self.stage = other;
                None
            }
        }
    }

    pub(super) fn locked(&self) -> bool {
        self.capture()
            .is_some_and(|capture| capture.mode == RecordMode::Locked)
    }

    /// Locks or unlocks the recording; nothing records, nothing changes.
    pub(super) fn set_locked(&mut self, locked: bool) {
        if let Some(capture) = self.capture_mut() {
            capture.mode = if locked {
                RecordMode::Locked
            } else {
                RecordMode::Held
            };
        }
    }

    pub(super) fn discard_confirming(&self) -> bool {
        self.capture()
            .is_some_and(|capture| capture.discard == DiscardRow::Confirming)
    }

    pub(super) fn set_discard_confirming(&mut self, confirming: bool) {
        if let Some(capture) = self.capture_mut() {
            capture.discard = if confirming {
                DiscardRow::Confirming
            } else {
                DiscardRow::Hidden
            };
        }
    }

    pub(super) fn play_once(&self) -> bool {
        self.capture()
            .is_some_and(|capture| capture.delivery == Delivery::PlayOnce)
    }

    /// Switches "Play once"; the new value, `false` when nothing records.
    pub(super) fn toggle_play_once(&mut self) -> bool {
        let Some(capture) = self.capture_mut() else {
            return false;
        };
        capture.delivery = match capture.delivery {
            Delivery::Normal => Delivery::PlayOnce,
            Delivery::PlayOnce => Delivery::Normal,
        };
        capture.delivery == Delivery::PlayOnce
    }

    pub(super) fn preview(&self) -> Option<&PlaybackClock> {
        self.capture()?.preview.as_ref()
    }

    pub(super) fn preview_mut(&mut self) -> Option<&mut PlaybackClock> {
        self.capture_mut()?.preview.as_mut()
    }

    pub(super) fn set_preview(&mut self, clock: PlaybackClock) {
        if let Some(capture) = self.capture_mut() {
            capture.preview = Some(clock);
        }
    }

    pub(super) fn take_preview(&mut self) -> Option<PlaybackClock> {
        self.capture_mut()?.preview.take()
    }

    pub(super) fn active(&self) -> bool {
        self.capture().is_some()
    }

    /// The recording's next wake-up is the task `task`, due at `due`; the
    /// one that ran before is dropped. `false` when nothing records.
    pub(super) fn set_wake(&mut self, due: std::time::Instant, task: gpui_kit::Task<()>) -> bool {
        let Some(capture) = self.capture_mut() else {
            return false;
        };
        capture.wake = RecordWake::Armed { due, _task: task };
        true
    }

    /// The wait until the capture's next deadline, `None` when it has none.
    pub(super) fn wake_in(&self) -> Option<std::time::Duration> {
        let capture = self.capture()?;
        let now = std::time::Instant::now();
        let (acting, limit) = match &capture.media {
            CaptureMedia::Voice(voice) => (!voice.is_paused(), None),
            CaptureMedia::Video(video) => (true, Some(video.time_left())),
        };
        super::record_wake::Deadlines {
            acting,
            limit,
            preview_end: capture
                .preview
                .as_ref()
                .and_then(|clock| clock.ends_in_at(now)),
        }
        .next()
    }

    /// The wake-up ended: no next one is armed.
    pub(super) fn stop_wake(&mut self) {
        if let Some(capture) = self.capture_mut() {
            capture.wake = RecordWake::Stopped;
        }
    }

    /// When the next wake-up is due, if one is armed.
    #[cfg(test)]
    pub(super) fn wake_due(&self) -> Option<std::time::Instant> {
        match &self.capture()?.wake {
            RecordWake::Armed { due, .. } => Some(*due),
            RecordWake::Stopped => None,
        }
    }

    /// A recording runs or is about to: another one must not start.
    pub(super) fn busy(&self) -> bool {
        !matches!(self.stage, RecordStage::Idle)
    }

    pub(super) fn opening_video(&self) -> bool {
        matches!(self.stage, RecordStage::Opening { video: true, .. })
    }

    pub(super) fn opening_origin(&self) -> Option<Option<ChatId>> {
        match &self.stage {
            RecordStage::Opening { origin, .. } => Some(*origin),
            _ => None,
        }
    }

    pub(super) fn opening_for(&self, wanted: CaptureFor) -> bool {
        matches!(&self.stage, RecordStage::Opening { purpose, .. } if *purpose == wanted)
    }

    /// A starting press is cancelled; a recording that runs stays.
    pub(super) fn cancel_start(&mut self) {
        if self.starting() {
            self.stage = RecordStage::Idle;
        }
    }

    /// The device answered for `start`: true when that start is still the
    /// one wanted, which then ends. A stale answer leaves a newer start be.
    pub(super) fn finish_start(&mut self, start: StartId) -> bool {
        let current = matches!(&self.stage, RecordStage::Opening { start: s, .. } if *s == start);
        if current {
            self.stage = RecordStage::Idle;
        }
        current
    }

    /// The window the recording belongs to (`Some(None)`: the main
    /// window); `None` when nothing records.
    pub(super) fn origin(&self) -> Option<Option<ChatId>> {
        self.capture().map(|capture| capture.origin)
    }

    pub(super) fn purpose(&self) -> Option<CaptureFor> {
        self.capture().map(|capture| capture.purpose)
    }

    pub(super) fn voice(&self) -> Option<&VoiceCapture> {
        match self.capture().map(|capture| &capture.media) {
            Some(CaptureMedia::Voice(voice)) => Some(voice),
            _ => None,
        }
    }

    pub(super) fn voice_mut(&mut self) -> Option<&mut VoiceCapture> {
        match self.capture_mut().map(|capture| &mut capture.media) {
            Some(CaptureMedia::Voice(voice)) => Some(voice),
            _ => None,
        }
    }

    pub(super) fn video(&self) -> Option<&VideoNoteCapture> {
        match self.capture().map(|capture| &capture.media) {
            Some(CaptureMedia::Video(video)) => Some(video),
            _ => None,
        }
    }

    pub(super) fn video_mut(&mut self) -> Option<&mut VideoNoteCapture> {
        match self.capture_mut().map(|capture| &mut capture.media) {
            Some(CaptureMedia::Video(video)) => Some(video),
            _ => None,
        }
    }

    /// Take the voice recording, with its window; nothing when a video
    /// message records.
    pub(super) fn take_voice(&mut self) -> Option<VoiceCapture> {
        match self.take_capture()? {
            Capture {
                media: CaptureMedia::Voice(voice),
                ..
            } => Some(voice),
            other => {
                self.stage = RecordStage::Recording(other);
                None
            }
        }
    }

    /// Take the video message recording, with its window; nothing when a
    /// voice message records.
    pub(super) fn take_video(&mut self) -> Option<VideoNoteCapture> {
        match self.take_capture()? {
            Capture {
                media: CaptureMedia::Video(video),
                ..
            } => Some(video),
            other => {
                self.stage = RecordStage::Recording(other);
                None
            }
        }
    }

    /// Take whatever records, with its window.
    pub(super) fn take(&mut self) -> Option<CaptureMedia> {
        self.take_capture().map(|capture| capture.media)
    }

    /// The camera's image of the video message recording, with the
    /// recording: it belongs to this one and goes with it.
    pub(super) fn video_with_preview_mut(
        &mut self,
    ) -> Option<(&VideoNoteCapture, &mut super::round_record::RoundPreview)> {
        let capture = self.capture_mut()?;
        match &capture.media {
            CaptureMedia::Video(video) => Some((video, &mut capture.round)),
            CaptureMedia::Voice(_) => None,
        }
    }
}

impl QuillApp {
    /// The capture for `start` is coming up, waited on by `opening`, for the
    /// window whose state is in place.
    pub(super) fn begin_capture_start(
        &mut self,
        start: StartId,
        video: bool,
        purpose: CaptureFor,
        opening: gpui_kit::Task<()>,
    ) {
        let origin = self.windows.scope;
        self.recording
            .begin_opening(start, video, origin, purpose, opening);
    }

    /// Hold a recording that just started, as the window whose state is
    /// in place (the one whose composer or story viewer asked for it).
    pub(super) fn hold_capture(&mut self, media: CaptureMedia, purpose: CaptureFor) {
        self.recording.stage = RecordStage::Recording(Capture {
            media,
            origin: self.windows.scope,
            purpose,
            mode: RecordMode::Held,
            discard: DiscardRow::Hidden,
            delivery: Delivery::Normal,
            preview: None,
            round: Default::default(),
            wake: RecordWake::Stopped,
        });
    }
}
