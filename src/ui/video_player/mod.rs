//! The video player core: one player over two backends, a transport state
//! machine, and pacing on the display's real frames
//! (`docs/decisions/video-player-core.md`).
//!
//! - macOS: AVPlayer decodes in hardware ([`backend_avplayer`]); pictures
//!   are GPU pixel buffers drawn through GPUI's `surface`.
//! - Linux and Windows (and macOS with `QUILL_VIDEO_BACKEND=ffmpeg`): the
//!   bundled FFmpeg shim ([`backend_ffmpeg`]); pictures are BGRA images.
//!
//! Consumers hold a [`VideoPlayer`] and draw [`element::video_frame`] from
//! it. The element pulls the picture due when its frame will be on screen
//! and asks for the next frame itself while the clip plays
//! (`frame_clock::request_video_frame`), so no consumer runs a timer.
//! The transport ([`Transport`]) is read at render time.
#![deny(clippy::indexing_slicing)]

mod backend_ffmpeg;
#[cfg(feature = "demo-capture")]
mod backend_still;
pub(crate) mod controls;
mod element;
pub(super) mod pacing;
mod transport;

#[cfg(target_os = "macos")]
mod backend_avplayer;

#[cfg(test)]
mod tests;

pub(crate) use element::video_frame;
pub(crate) use transport::{PlayState, Transport};

use gpui_kit::*;
use pacing::PresentEstimator;
use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use transport::{BackendStatus, Command, PlayStart, TransportMachine};

/// What a clip is opened for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Purpose {
    /// The media viewer (and its Picture-in-Picture): full size, with
    /// sound.
    Viewer,
    /// A muted loop in the media viewer (GIFs): full size, no soundtrack
    /// decoded.
    ViewerLoop,
    /// A muted loop in the history or the GIF panel, decoded at most
    /// `max_edge` px on its longer side (FFmpeg; AVPlayer ignores it).
    /// Turning the volume up (a round video played with sound) reopens
    /// it with its soundtrack.
    Inline { max_edge: u32 },
    /// A story in the story viewer: full size, with sound.
    Story,
}

/// A picture to draw.
#[derive(Clone)]
pub(crate) enum Picture {
    /// A GPU pixel buffer (AVPlayer), drawn by GPUI's `surface`.
    #[cfg(target_os = "macos")]
    Surface(core_video::pixel_buffer::CVPixelBuffer),
    /// A decoded BGRA image (FFmpeg, still frames), drawn by `img`.
    Image(Arc<RenderImage>),
}

// `Picture` has one variant off macOS, so its matches are split by
// platform instead of carrying a cfg'd arm.
impl Picture {
    /// The decoded image, when the picture is one (not a GPU surface).
    #[cfg(target_os = "macos")]
    pub(crate) fn image(&self) -> Option<Arc<RenderImage>> {
        match self {
            Picture::Image(image) => Some(image.clone()),
            Picture::Surface(_) => None,
        }
    }

    /// The decoded image (every picture is one off macOS).
    #[cfg(not(target_os = "macos"))]
    pub(crate) fn image(&self) -> Option<Arc<RenderImage>> {
        let Picture::Image(image) = self;
        Some(image.clone())
    }

    /// A GPU surface, which GPUI can't clip (corners are masked instead).
    #[cfg(target_os = "macos")]
    pub(crate) fn is_surface(&self) -> bool {
        matches!(self, Picture::Surface(_))
    }

    #[cfg(not(target_os = "macos"))]
    pub(crate) fn is_surface(&self) -> bool {
        false
    }
}

/// Why a clip can't play.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PlayerError {
    pub(crate) kind: ErrorKind,
    pub(crate) detail: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ErrorKind {
    /// The file could not be opened.
    Open,
    /// Decoding failed mid-stream.
    Decode,
    /// No in-process decoder on this system.
    Unsupported,
}

impl PlayerError {
    fn new(kind: ErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for PlayerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.detail)
    }
}

/// When the frame being drawn reaches the screen.
#[derive(Clone, Copy, Debug)]
pub(super) struct Present {
    pub(super) at: Instant,
    /// The same moment in Mach host ticks, when the display link said so
    /// (only AVPlayer reads it).
    #[cfg(target_os = "macos")]
    pub(super) host_time: Option<u64>,
}

/// What a pull got from the backend.
#[derive(Default)]
pub(super) struct Pulled {
    /// A new picture, when one became due.
    picture: Option<Picture>,
    /// The item time asked for (seconds).
    item_secs: f64,
    /// The new picture's own time (seconds).
    display_secs: Option<f64>,
}

/// A decoder. Every call is made on the main thread.
trait Backend {
    /// Poll: open state, position, stalls, end, buffered ranges.
    fn status(&mut self) -> BackendStatus;
    fn play(&mut self, rate: f64);
    fn pause(&mut self);
    fn seek(&mut self, secs: f64);
    fn set_volume(&mut self, volume: f32);
    fn set_rate(&mut self, rate: f64, playing: bool);
    /// One picture forward or back, while paused.
    fn step_frame(&mut self, forward: bool);
    /// The picture due at `present`, when it changed.
    fn take_picture(&mut self, present: &Present) -> Pulled;
    /// When the next picture becomes due, if known (`interval`: the
    /// display's frame interval).
    fn next_due(&self, now: Instant, interval: Duration) -> Option<Instant>;
    fn error(&self) -> Option<PlayerError>;
}

/// The player. Cheap to clone (a shared handle); the clip stops when the
/// last handle drops.
#[derive(Clone)]
pub(crate) struct VideoPlayer(Rc<PlayerCell>);

/// A handle that does not keep the clip playing (a view that only draws
/// it while someone else owns it).
#[derive(Clone)]
pub(crate) struct WeakVideoPlayer(std::rc::Weak<PlayerCell>);

impl WeakVideoPlayer {
    pub(crate) fn upgrade(&self) -> Option<VideoPlayer> {
        self.0.upgrade().map(VideoPlayer)
    }
}

struct PlayerCell {
    inner: RefCell<Inner>,
    /// Beside `inner`, not in it: a backend call takes it out and puts it
    /// back without touching `inner`'s borrow, so no borrow is held across
    /// a native call and a busy `inner` never loses the backend.
    backend: Cell<Option<Box<dyn Backend>>>,
    /// The frame request in flight: the view it redraws and when it counts
    /// as lost (`request_video_frame`).
    requested: FrameRequested,
    /// How many times the picture was painted inside the visible area
    /// (the inline players' off-screen policy compares it across a pass).
    shown: Cell<u64>,
}

/// Shared with `frame_clock::request_video_frame`.
pub(crate) type FrameRequested = Rc<Cell<Option<(EntityId, Instant)>>>;

struct Inner {
    id: u64,
    machine: TransportMachine,
    estimator: PresentEstimator,
    /// The last picture as decoded, and turned for the orientation.
    raw: Option<Picture>,
    shown: Option<Picture>,
    /// The image `turn` made for `shown` (FFmpeg pictures turned), retired
    /// from the GPU atlas when replaced; `raw`'s own image is the
    /// backend's to retire.
    turned_image: Option<Arc<RenderImage>>,
    turns: u8,
    mirrored: bool,
    error: Option<PlayerError>,
    /// A new picture is wanted since then (opened, seeked, stepped): the
    /// element keeps drawing until it comes, even while paused.
    awaiting: Option<Instant>,
    /// Views redrawn when the transport changes (controls), at most
    /// [`WATCH_INTERVAL`] apart while playing.
    watchers: Vec<EntityId>,
    watched: Option<(PlayState, Instant)>,
    _poll: Option<Task<()>>,
}

/// How often watchers (time labels, the seek bar) are redrawn while the
/// clip plays.
const WATCH_INTERVAL: Duration = Duration::from_millis(100);
/// While opening, the backend is polled this often (AVPlayer reports
/// `readyToPlay` asynchronously; no KVO).
const OPEN_POLL: Duration = Duration::from_millis(50);
/// A paused seek or step waits this long for its picture at most.
const AWAIT_PICTURE: Duration = Duration::from_secs(2);

impl VideoPlayer {
    /// Open a local file; playback starts paused (call [`Self::play`]).
    /// Never blocks the UI thread, except in screenshot captures
    /// (`QUILL_DEMO_CAPTURE`), which show a still first picture decoded
    /// synchronously (at most two seconds) so the capture is
    /// deterministic. `duration_hint` (the message's) is shown until the
    /// file's own is known.
    pub(crate) fn open(
        path: &Path,
        purpose: Purpose,
        duration_hint: Option<f64>,
        cx: &mut App,
    ) -> Result<Self, PlayerError> {
        let player = Self::from_backend(open_backend(path, purpose)?, duration_hint);
        let poll = player.spawn_open_poll(cx);
        if let Ok(mut inner) = player.0.inner.try_borrow_mut() {
            inner._poll = Some(poll);
        }
        Ok(player)
    }

    /// A player over an opened backend (no open poll: `pull` polls too).
    fn from_backend(backend: Box<dyn Backend>, duration_hint: Option<f64>) -> Self {
        Self(Rc::new(PlayerCell {
            inner: RefCell::new(Inner {
                id: next_player_id(),
                machine: TransportMachine::new(duration_hint, Instant::now()),
                estimator: PresentEstimator::default(),
                raw: None,
                shown: None,
                turned_image: None,
                turns: 0,
                mirrored: false,
                error: None,
                awaiting: Some(Instant::now()),
                watchers: Vec::new(),
                watched: None,
                _poll: None,
            }),
            backend: Cell::new(Some(backend)),
            requested: Rc::new(Cell::new(None)),
            shown: Cell::new(0),
        }))
    }

    /// Poll the backend until it is open (or failed), then redraw the
    /// watchers once. The task holds no strong handle: dropping the player
    /// ends it.
    fn spawn_open_poll(&self, cx: &mut App) -> Task<()> {
        let weak = Rc::downgrade(&self.0);
        cx.spawn(async move |cx| {
            loop {
                cx.background_executor().timer(OPEN_POLL).await;
                let Some(cell) = weak.upgrade() else {
                    return;
                };
                let player = VideoPlayer(cell);
                player.update_status(Instant::now());
                if player.state() != PlayState::Opening {
                    let watchers = player.watchers();
                    cx.update(|cx| {
                        for id in watchers {
                            cx.notify(id);
                        }
                    });
                    return;
                }
            }
        })
    }

    /// How many times the element painted this player's picture inside
    /// the visible area so far.
    pub(crate) fn shown_count(&self) -> u64 {
        self.0.shown.get()
    }

    /// The element painted the picture inside the visible area.
    pub(super) fn note_shown(&self) {
        self.0.shown.set(self.0.shown.get().wrapping_add(1));
    }

    pub(crate) fn downgrade(&self) -> WeakVideoPlayer {
        WeakVideoPlayer(Rc::downgrade(&self.0))
    }

    /// Run `f` on the backend with no borrow of the player held. A call
    /// made from inside `f` (none is) would find no backend and do nothing.
    fn with_backend<R>(&self, f: impl FnOnce(&mut dyn Backend) -> R) -> Option<R> {
        let mut backend = self.0.backend.take()?;
        let result = f(backend.as_mut());
        self.0.backend.set(Some(backend));
        Some(result)
    }

    fn with_inner<R>(&self, f: impl FnOnce(&mut Inner) -> R) -> Option<R> {
        self.0
            .inner
            .try_borrow_mut()
            .ok()
            .map(|mut inner| f(&mut inner))
    }

    fn read_inner<R>(&self, f: impl FnOnce(&Inner) -> R) -> Option<R> {
        self.0.inner.try_borrow().ok().map(|inner| f(&inner))
    }

    pub(crate) fn play(&self) {
        let now = Instant::now();
        let Some((start, rate)) =
            self.with_inner(|inner| (inner.machine.play(now), inner.machine.rate()))
        else {
            return;
        };
        match start {
            PlayStart::Now => {
                self.with_backend(|backend| backend.play(rate));
            }
            PlayStart::FromStart => {
                self.with_backend(|backend| {
                    backend.seek(0.0);
                    backend.play(rate);
                });
            }
            PlayStart::Already | PlayStart::Later => {}
        }
    }

    pub(crate) fn pause(&self) {
        let now = Instant::now();
        self.with_inner(|inner| inner.machine.pause(now));
        self.with_backend(|backend| backend.pause());
    }

    /// Seek precisely to `secs` (clamped to the clip).
    pub(crate) fn seek(&self, secs: f64) {
        let now = Instant::now();
        let Some(target) = self.with_inner(|inner| {
            inner.awaiting = Some(now);
            inner.machine.seek(secs, now)
        }) else {
            return;
        };
        self.with_backend(|backend| backend.seek(target));
    }

    /// One picture forward (`true`) or back, while paused (the viewer's
    /// `.` and `,`).
    pub(crate) fn step_frame(&self, forward: bool) {
        if self
            .read_inner(|inner| inner.machine.state().animates())
            .unwrap_or(true)
        {
            return;
        }
        self.with_inner(|inner| inner.awaiting = Some(Instant::now()));
        self.with_backend(|backend| backend.step_frame(forward));
    }

    /// 0.0 (muted) – 1.0.
    pub(crate) fn set_volume(&self, volume: f32) {
        let Some(volume) = self.with_inner(|inner| inner.machine.set_volume(volume)) else {
            return;
        };
        self.with_backend(|backend| backend.set_volume(volume));
    }

    /// Playback speed (1.0 = normal).
    pub(crate) fn set_rate(&self, rate: f64) {
        let now = Instant::now();
        let Some((rate, playing)) = self.with_inner(|inner| {
            let rate = inner.machine.set_rate(rate, now);
            (rate, inner.machine.wants_play())
        }) else {
            return;
        };
        self.with_backend(|backend| backend.set_rate(rate, playing));
    }

    /// Loop at the end instead of ending (GIFs).
    pub(crate) fn set_looping(&self, looping: bool) {
        self.with_inner(|inner| inner.machine.set_looping(looping));
    }

    /// Show the picture turned by `turns` clockwise quarter turns, then
    /// mirrored left to right when `mirrored`.
    pub(crate) fn set_orientation(&self, turns: u8, mirrored: bool) {
        let turns = turns % 4;
        let raw = self.with_inner(|inner| {
            if (inner.turns, inner.mirrored) == (turns, mirrored) {
                return None;
            }
            inner.turns = turns;
            inner.mirrored = mirrored;
            inner.raw.clone()
        });
        let Some(raw) = raw.flatten() else {
            return;
        };
        // Turning copies the picture: no borrow held meanwhile.
        let turned = turn(&raw, turns, mirrored);
        self.with_inner(|inner| inner.show(raw, turned));
    }

    /// Add one view to the watchers (the viewer panel, a PiP window).
    pub(crate) fn watch(&self, view: EntityId) {
        self.with_inner(|inner| {
            if !inner.watchers.contains(&view) {
                inner.watchers.push(view);
            }
        });
    }

    fn watchers(&self) -> Vec<EntityId> {
        self.read_inner(|inner| inner.watchers.clone())
            .unwrap_or_default()
    }

    pub(crate) fn state(&self) -> PlayState {
        self.read_inner(|inner| inner.machine.state())
            .unwrap_or(PlayState::Failed)
    }

    /// The transport now (position extrapolated to this instant).
    pub(crate) fn transport(&self) -> Transport {
        let now = Instant::now();
        self.read_inner(|inner| inner.machine.snapshot(now))
            .unwrap_or_else(|| TransportMachine::new(None, now).snapshot(now))
    }

    /// The picture on screen now, without pulling a new one.
    pub(crate) fn current_picture(&self) -> Option<Picture> {
        self.read_inner(|inner| inner.shown.clone()).flatten()
    }

    /// Why the clip can't play, once (the transport stays `Failed`).
    pub(crate) fn take_error(&self) -> Option<PlayerError> {
        self.with_inner(|inner| inner.error.take()).flatten()
    }

    /// Poll the backend and fold its status into the transport; carry out
    /// what that asks of the backend (start once open, loop at the end).
    fn update_status(&self, now: Instant) {
        let Some(status) = self.with_backend(|backend| {
            let status = backend.status();
            (status, backend.error())
        }) else {
            return;
        };
        let (status, error) = status;
        let command = self.with_inner(|inner| {
            if let Some(error) = error.filter(|_| inner.machine.state() != PlayState::Failed) {
                inner.machine.fail(now);
                inner.error = Some(error);
                return None;
            }
            inner
                .machine
                .observe(&status, now)
                .map(|command| (command, inner.machine.rate()))
        });
        match command.flatten() {
            Some((Command::Play, rate)) => {
                self.with_backend(|backend| backend.play(rate));
            }
            Some((Command::Loop, rate)) => {
                self.with_backend(|backend| {
                    backend.seek(0.0);
                    backend.play(rate);
                });
            }
            None => {}
        }
    }

    /// The draw that contains the picture started at `now`: predict when
    /// it is shown (`hint`: the display link's present time and host
    /// ticks), take the picture due then, and return what to paint.
    /// The display's refresh period as the platform reports it (macOS);
    /// without one the interval is learned from draw spacing.
    pub(super) fn set_refresh_period(&self, period: Option<Duration>) {
        self.with_inner(|inner| inner.estimator.set_period(period));
    }

    pub(super) fn pull(&self, now: Instant, hint: Option<(Instant, u64)>) -> Option<Picture> {
        let present = self.with_inner(|inner| {
            inner.estimator.record_draw(now);
            inner.estimator.set_hint(hint.map(|(at, _)| at));
            let at = inner.estimator.predict(now);
            inner.estimator.note_present(at);
            Present {
                at,
                #[cfg(target_os = "macos")]
                host_time: hint
                    .filter(|(hinted, _)| *hinted == at)
                    .map(|(_, host)| host),
            }
        });
        let Some(present) = present else {
            // Busy (never expected): repaint what is on screen.
            return self.current_picture();
        };
        self.update_status(now);
        let state = self.state();
        let pulled = if matches!(state, PlayState::Opening | PlayState::Failed) {
            Pulled::default()
        } else {
            self.with_backend(|backend| backend.take_picture(&present))
                .unwrap_or_default()
        };
        if trace_video()
            && let Some((id, rate)) = self.read_inner(|inner| (inner.id, trace_rate(inner)))
        {
            eprintln!(
                "{}",
                trace_line(
                    id,
                    now,
                    pulled.item_secs,
                    pulled.picture.is_some(),
                    pulled.display_secs,
                    rate
                )
            );
        }
        if let Some(picture) = pulled.picture {
            let (turns, mirrored) = self
                .read_inner(|inner| (inner.turns, inner.mirrored))
                .unwrap_or((0, false));
            // Turning copies the picture: no borrow held meanwhile.
            let turned = turn(&picture, turns, mirrored);
            self.with_inner(|inner| {
                inner.awaiting = None;
                inner.show(picture, turned);
            });
        }
        self.current_picture()
    }

    /// After the picture was painted: what the next frame request needs.
    pub(super) fn frame_demand(&self, now: Instant) -> FrameDemand {
        let interval = self
            .read_inner(|inner| inner.estimator.interval())
            .unwrap_or(Duration::from_millis(16));
        let next_due = self
            .with_backend(|backend| backend.next_due(now, interval))
            .flatten();
        self.read_inner(|inner| {
            let transport = inner.machine.snapshot(now);
            let awaiting = inner.awaiting.is_some_and(|since| {
                transport.state == PlayState::Opening
                    || now.saturating_duration_since(since) < AWAIT_PICTURE
            });
            FrameDemand {
                animates: (transport.state.animates() || awaiting)
                    && transport.state != PlayState::Failed,
                plays_sound: transport.has_sound && transport.volume > 0.0,
                next_due,
                interval: inner.estimator.interval(),
                lead: inner.estimator.lead(),
                requested: self.0.requested.clone(),
            }
        })
        .unwrap_or_else(|| FrameDemand {
            animates: false,
            plays_sound: false,
            next_due: None,
            interval: Duration::from_millis(16),
            lead: Duration::from_millis(16),
            requested: self.0.requested.clone(),
        })
    }

    /// The watchers to redraw now: on a state change, or every
    /// [`WATCH_INTERVAL`] while playing. Marks them as redrawn.
    pub(super) fn due_watchers(&self, now: Instant) -> Vec<EntityId> {
        self.with_inner(|inner| {
            let state = inner.machine.state();
            let due = match inner.watched {
                None => true,
                Some((seen, at)) => {
                    seen != state
                        || (state.animates() && now.saturating_duration_since(at) >= WATCH_INTERVAL)
                }
            };
            if !due || inner.watchers.is_empty() {
                return Vec::new();
            }
            inner.watched = Some((state, now));
            inner.watchers.clone()
        })
        .unwrap_or_default()
    }
}

/// What the video element needs to ask for the next frame.
pub(super) struct FrameDemand {
    pub(super) animates: bool,
    pub(super) plays_sound: bool,
    pub(super) next_due: Option<Instant>,
    pub(super) interval: Duration,
    /// How far ahead of its draw start a frame is shown.
    pub(super) lead: Duration,
    pub(super) requested: FrameRequested,
}

impl Inner {
    /// Put `raw` on screen, as `turned` when it was turned.
    fn show(&mut self, raw: Picture, turned: Option<Picture>) {
        let image = turned.as_ref().and_then(Picture::image);
        if let Some(old) = std::mem::replace(&mut self.turned_image, image) {
            super::image_budget::retire_all([old]);
        }
        self.shown = Some(turned.unwrap_or_else(|| raw.clone()));
        self.raw = Some(raw);
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        if let Some(old) = self.turned_image.take() {
            super::image_budget::retire_all([old]);
        }
    }
}

/// The picture turned for the orientation; `None` when it needs no turn
/// or turning failed (the picture is shown as it is).
#[cfg(target_os = "macos")]
fn turn(picture: &Picture, turns: u8, mirrored: bool) -> Option<Picture> {
    if turns == 0 && !mirrored {
        return None;
    }
    match picture {
        Picture::Image(image) => {
            super::native_video_rotate::turn_render_image(image, turns, mirrored)
                .map(Picture::Image)
        }
        Picture::Surface(buffer) => {
            super::native_video_rotate::turn_surface(buffer, turns, mirrored).map(Picture::Surface)
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn turn(picture: &Picture, turns: u8, mirrored: bool) -> Option<Picture> {
    if turns == 0 && !mirrored {
        return None;
    }
    let Picture::Image(image) = picture;
    super::native_video_rotate::turn_render_image(image, turns, mirrored).map(Picture::Image)
}

/// The backend for this file: a still first picture in screenshot
/// captures; AVPlayer on macOS (unless `QUILL_VIDEO_BACKEND=ffmpeg`); the
/// FFmpeg shim elsewhere.
fn open_backend(path: &Path, purpose: Purpose) -> Result<Box<dyn Backend>, PlayerError> {
    #[cfg(test)]
    if let Some(backend) = tests::take_scripted_backend() {
        return Ok(backend);
    }
    // Only capture builds render a still (a blocking decode of up to two
    // seconds); the variable means nothing to other builds.
    #[cfg(feature = "demo-capture")]
    if std::env::var_os("QUILL_DEMO_CAPTURE").is_some() {
        return backend_still::StillBackend::open(path).map(|b| Box::new(b) as Box<dyn Backend>);
    }
    #[cfg(target_os = "macos")]
    if !backend_ffmpeg::preferred_on_macos() {
        return backend_avplayer::AvPlayerBackend::open(path)
            .map(|b| Box::new(b) as Box<dyn Backend>);
    }
    backend_ffmpeg::FfmpegBackend::open(path, purpose).map(|b| Box::new(b) as Box<dyn Backend>)
}

/// Whether clips can play in this process: always on macOS (AVPlayer);
/// elsewhere once the bundled FFmpeg shim loads. In tests, also once the
/// test scripted a backend (CI has no shim).
pub(crate) fn supported() -> bool {
    #[cfg(test)]
    if tests::scripted_backends() {
        return true;
    }
    cfg!(target_os = "macos") || quill::video_decode::available()
}

/// Whether pictures are decoded in software into BGRA images in this
/// process (FFmpeg): inline tiles then decode at their own size, leave
/// clips above 1080p still and share a memory budget. AVPlayer decodes in
/// hardware into buffers of its own.
pub(crate) fn decodes_in_process() -> bool {
    cfg!(not(target_os = "macos")) || backend_ffmpeg::preferred_on_macos()
}

/// Whether a `.gif` file can play through the player: only through FFmpeg
/// (AVPlayer does not read GIF).
pub(crate) fn plays_gif_files() -> bool {
    quill::video_decode::available()
        && (cfg!(not(target_os = "macos")) || backend_ffmpeg::preferred_on_macos())
}

/// A process-unique player id (the `QUILL_TRACE_VIDEO` column).
pub(crate) fn next_player_id() -> u64 {
    thread_local! {
        static NEXT: Cell<u64> = const { Cell::new(1) };
    }
    NEXT.with(|next| {
        let id = next.get();
        next.set(id.wrapping_add(1));
        id
    })
}

/// `QUILL_TRACE_VIDEO=1`: one CSV line per pull on stderr,
/// `VT,<player>,<wall_us>,<item_ms>,<new 0/1>,<display_ms>,<rate>`: the
/// wall time since the first traced pull, the item time asked for, whether
/// a new picture came, its own time (-1000 when none) and the rate while
/// playing (0 otherwise). `quill-tools` and the decision doc have the
/// analysis script.
pub(crate) fn trace_video() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("QUILL_TRACE_VIDEO").is_some())
}

/// The trace's rate column: the rate while playing, else 0.
fn trace_rate(inner: &Inner) -> f64 {
    if inner.machine.state().animates() {
        inner.machine.rate()
    } else {
        0.0
    }
}

/// One `QUILL_TRACE_VIDEO` line (shared with the legacy inline player).
pub(crate) fn trace_line(
    player: u64,
    now: Instant,
    item_secs: f64,
    fresh: bool,
    display_secs: Option<f64>,
    rate: f64,
) -> String {
    static EPOCH: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    let epoch = *EPOCH.get_or_init(|| now);
    let wall_us = now.saturating_duration_since(epoch).as_micros();
    format!(
        "VT,{player},{wall_us},{:.3},{},{:.3},{rate:.2}",
        item_secs * 1000.0,
        u8::from(fresh),
        display_secs.map_or(-1000.0, |secs| secs * 1000.0),
    )
}
