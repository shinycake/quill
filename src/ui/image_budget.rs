//! Bounded image memory for the main window and chat windows.
//!
//! GPUI keeps every image an `img(path)` ever loaded — decoded in RAM and
//! uploaded to the window's sprite atlas — for the life of the app, and an
//! `Arc<RenderImage>` painted once stays in the atlas until someone calls
//! `drop_image`. Scrolling through avatars and photos therefore only ever
//! grew the footprint. This module bounds both:
//!
//! - [`BoundedImageCache`] loads a window's path images (an `ImageCache`
//!   installed over `QuillApp`, or over a chat window's body, by
//!   [`CacheScope`]) and, past a byte budget,
//!   evicts the least recently used images that can no longer be on screen,
//!   dropping them from the atlas too. Once nothing new has been shown for
//!   a moment (`IDLE_AFTER`), it trims to a much smaller idle budget
//!   ([`BoundedImageCache::idle_trim`]), so the memory of a scroll comes
//!   back.
//! - [`sized_image`] loads small pictures (avatars, chat-list thumbnails)
//!   decoded at the size they are drawn at, times the window's scale
//!   factor, instead of at file size.
//! - [`retire_all`] takes `RenderImage`s Quill made itself (sticker frames,
//!   blurred previews…) once their cache lets go of them; the sweep
//!   (`retired`) drops them from the atlas once no window can show them.
//!
//! "Can no longer be on screen" accounts for cached slices (`app_slice`):
//! a slice replays the frame it last rendered, so an image it used is only
//! gone once that slice rendered again without it. Images used outside any
//! slice are re-requested by the window's view every frame. Frames are
//! numbered across all windows, and each window keeps its own record of
//! what it drew in full.
//!
//! Atlas textures (512² for pictures up to 128 px a side, else 1024² and
//! up, on every GPUI backend: Metal, wgpu, DirectX) are released only once
//! every tile in them is gone. Tiles are allocated in the newest texture
//! with room, so images loaded together share textures; evicting
//! oldest-first empties whole textures rather than punching holes in all
//! of them.
#![deny(clippy::indexing_slicing)]

use super::image_decode::{self, DecodeSize};
use gpui_kit::*;
use quill::ids::ChatId;
use quill::telegram::envelope::MiniThumbnail;
use smallvec::SmallVec;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

mod cache_scope;
pub(in crate::ui) use cache_scope::CacheScope;
#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod gif_still_tests;
mod retired;
#[cfg(test)]
pub(in crate::ui) use retired::take_unshowable;
pub(in crate::ui) use retired::{
    begin_main_frame, drop_images_off_ui_thread, retire_all, start_sweeper,
};

/// What the cache keeps beyond the screen: while the user scrolls (the
/// next rows are probably the ones just seen), and once idle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Budget {
    bytes: usize,
    entries: usize,
}

/// Decoded images kept beyond what is on screen while things change.
const ACTIVE_BUDGET: Budget = Budget {
    bytes: 48 * 1024 * 1024,
    entries: 2048,
};
/// Decoded images kept beyond what is on screen once idle.
const IDLE_BUDGET: Budget = Budget {
    bytes: 8 * 1024 * 1024,
    entries: 256,
};
const _: () =
    assert!(IDLE_BUDGET.bytes < ACTIVE_BUDGET.bytes && IDLE_BUDGET.entries < ACTIVE_BUDGET.entries);
/// How long nothing new must be shown before the idle trim.
pub(super) const IDLE_AFTER: Duration = Duration::from_secs(2);

thread_local! {
    /// Frames drawn so far, in every window ([`next_frame`]).
    static FRAME: Cell<u64> = const { Cell::new(0) };
    /// Each window's frames that drew everything (`None`: the main
    /// window). A window that closed has no entry.
    static WHOLE: RefCell<HashMap<Option<ChatId>, WholeFrames>> = RefCell::new(HashMap::new());
    /// Each slice's window, its last render frame, the last frame it was
    /// shown, and the render before the last one.
    static SLICES: RefCell<HashMap<EntityId, SliceFrames>> = RefCell::new(HashMap::new());
    /// When the cache last started showing an image it didn't show just
    /// before (scrolling, new content) or stopped showing one (another
    /// chat), and the activity the last idle trim covered.
    static ACTIVITY: Cell<Option<Instant>> = const { Cell::new(None) };
    static IDLE_TRIMMED: Cell<Option<Instant>> = const { Cell::new(None) };
    /// The slice being drawn (`app_slice`), for paint stamps.
    static SCOPE: RefCell<Option<Rc<Cell<Option<EntityId>>>>> = const { RefCell::new(None) };
    /// The cache of the `CacheScope` being drawn (display-size loads), and
    /// its window.
    static SCOPED: RefCell<Vec<(Entity<BoundedImageCache>, Option<ChatId>)>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone, Copy, Debug, Default)]
struct SliceFrames {
    window: Option<ChatId>,
    rendered: u64,
    shown: u64,
    previous_render: u64,
    /// Every image the slice's last render painted has an id below this
    /// (`RenderImage` ids only grow): the next image id once the frame of
    /// that render was done. `None` until then.
    images_below: Option<usize>,
}

/// What a window shows beside its animation layers: a patch frame leaves
/// all that as the latest whole frame drew it.
#[derive(Clone, Copy, Debug, Default)]
struct WholeFrames {
    latest: u64,
    /// The whole frame on screen as the current frame started.
    on_screen: u64,
    /// Every image `latest` painted has an id below this, once it is done.
    images_below: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WindowDraw {
    /// Everything: the window's own view renders.
    Whole,
    /// Only the animation layers (`Entity::live`).
    Patch,
}

/// Start a main window frame that draws everything (`begin_main_frame`,
/// before anything renders).
pub(super) fn begin_frame() -> u64 {
    next_frame(None, WindowDraw::Whole)
}

/// Start a main window frame that draws only the animation layers.
pub(super) fn begin_patch_frame() -> u64 {
    next_frame(None, WindowDraw::Patch)
}

/// Start a frame of `chat`'s window, before anything in it renders.
pub(super) fn begin_chat_frame(chat: ChatId, draw: WindowDraw) -> u64 {
    next_frame(Some(chat), draw)
}

fn next_frame(window: Option<ChatId>, draw: WindowDraw) -> u64 {
    if window.is_none() {
        super::frame_clock::trace_slice_render("(window frame)");
    }
    retired::close_frame();
    let frame = FRAME.with(|frame| {
        frame.set(frame.get() + 1);
        frame.get()
    });
    WHOLE.with(|whole| {
        let mut whole = whole.borrow_mut();
        let whole = whole.entry(window).or_default();
        whole.on_screen = whole.latest;
        if draw == WindowDraw::Whole {
            whole.latest = frame;
        }
    });
    frame
}

/// What `window`'s frames drew in full; `None` once it closed.
fn whole_of(window: Option<ChatId>) -> Option<WholeFrames> {
    let whole = WHOLE.with(|whole| whole.borrow().get(&window).copied());
    whole.or_else(|| window.is_none().then(WholeFrames::default))
}

/// `chat`'s window closed: nothing it drew shows any more.
pub(super) fn forget_window(chat: ChatId) {
    WHOLE.with(|whole| whole.borrow_mut().remove(&Some(chat)));
    SLICES.with(|slices| slices.borrow_mut().retain(|_, s| s.window != Some(chat)));
}

/// The id the next `RenderImage` will get. GPUI numbers images from one
/// counter, so an image made later has a larger id; an empty image is the
/// only way to read it (no pixels, nothing allocated).
fn next_image_id() -> usize {
    RenderImage::new(Vec::<image::Frame>::new()).id.0
}

pub(super) fn frame() -> u64 {
    FRAME.with(Cell::get)
}

/// A slice of `window` rendered its content this frame.
pub(super) fn slice_rendered(id: EntityId, window: Option<ChatId>) {
    let now = frame();
    SLICES.with(|slices| {
        let mut slices = slices.borrow_mut();
        let entry = slices.entry(id).or_default();
        if entry.rendered != now {
            entry.previous_render = entry.rendered;
        }
        entry.window = window;
        entry.rendered = now;
        entry.shown = now;
        entry.images_below = None;
    });
}

/// A slice of `window` is part of this frame (rendered or replayed).
pub(super) fn slice_shown(id: EntityId, window: Option<ChatId>) {
    let now = frame();
    SLICES.with(|slices| {
        let mut slices = slices.borrow_mut();
        let entry = slices.entry(id).or_default();
        entry.window = window;
        entry.shown = now;
    });
}

/// Whether an image last used at `used` in `scope` (a slice, or `window`'s
/// own view for `None`) can still be on screen, as of the start of the
/// current frame.
fn may_be_visible(window: Option<ChatId>, scope: Option<EntityId>, used: u64) -> bool {
    match scope {
        None => whole_of(window).is_some_and(|whole| used >= whole.on_screen),
        Some(id) => SLICES.with(|slices| {
            slices.borrow().get(&id).is_some_and(|s| {
                let on_screen = whole_of(s.window).map(|whole| whole.on_screen);
                on_screen.is_some_and(|on_screen| s.shown >= on_screen) && used >= s.rendered
            })
        }),
    }
}

/// Where (a window, and a slice of it or its own view) and in which frame
/// something was painted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PaintStamp {
    window: Option<ChatId>,
    scope: Option<EntityId>,
    frame: u64,
}

impl PaintStamp {
    pub(super) fn same_place(self, other: PaintStamp) -> bool {
        self.window == other.window && self.scope == other.scope
    }
}

/// A stamp for content painted now.
pub(super) fn paint_stamp() -> PaintStamp {
    paint_stamp_in(SCOPED.with(|scoped| scoped.borrow().last().and_then(|(_, window)| *window)))
}

pub(super) fn paint_stamp_in(window: Option<ChatId>) -> PaintStamp {
    PaintStamp {
        window,
        scope: SCOPE.with(|scope| scope.borrow().as_ref().and_then(|scope| scope.get())),
        frame: frame(),
    }
}

/// Whether content painted at `stamp` can still be on screen (a cached
/// slice may replay it), as of the start of the current frame.
pub(super) fn may_still_show(stamp: PaintStamp) -> bool {
    may_be_visible(stamp.window, stamp.scope, stamp.frame)
}

/// Whether content painted at `stamp` is on screen now, between frames or
/// while one paints: its window's latest frame drew it, or replays a slice
/// render that did. A stamp that does not show never will again: the
/// window drew since, or closed. ([`may_still_show`] also counts the frame
/// before.)
pub(super) fn shows_now(stamp: PaintStamp) -> bool {
    let Some(whole) = whole_of(stamp.window) else {
        return false;
    };
    match stamp.scope {
        None => stamp.frame >= whole.latest,
        Some(id) => SLICES.with(|slices| {
            let slices = slices.borrow();
            let slice = slices.get(&id);
            slice.is_some_and(|s| s.shown >= whole.latest && stamp.frame >= s.rendered)
        }),
    }
}

/// Whether an image used now in `scope` (of `window`, for its own view),
/// last used there at `previous`, wasn't on screen just before (it
/// scrolled in, or is new).
fn newly_shown(window: Option<ChatId>, scope: Option<EntityId>, previous: Option<u64>) -> bool {
    let Some(previous) = previous else {
        return true;
    };
    match scope {
        None => whole_of(window).is_none_or(|whole| previous < whole.on_screen),
        Some(id) => SLICES.with(|slices| {
            slices
                .borrow()
                .get(&id)
                .is_none_or(|s| previous < s.previous_render)
        }),
    }
}

pub(super) fn note_activity() {
    ACTIVITY.with(|activity| activity.set(Some(Instant::now())));
}

/// Images leaving the screen (another chat opened, a panel closed) change
/// it as much as new ones arriving, so fewer cached images that may be on
/// screen than at the last frame is activity too. Before this, leaving a
/// chat full of photos for one without kept the cache at its scrolling
/// budget for as long as no new picture showed.
/// `shown` holds the count from the last call.
fn note_departures(shown: &mut usize, now: usize) {
    if now < std::mem::replace(shown, now) {
        note_activity();
    }
}

/// When the idle trim is due: `IDLE_AFTER` past the last change of what
/// is on screen, unless the last idle trim already covered it.
pub(super) fn idle_trim_due() -> Option<Instant> {
    let activity = ACTIVITY.with(Cell::get)?;
    let trimmed = IDLE_TRIMMED.with(Cell::get);
    (trimmed != Some(activity)).then_some(activity + IDLE_AFTER)
}

/// Keys to evict, least recently used first, until `total` fits `budget`.
/// `candidates` are `(last_used, key, bytes)` of entries that can't be on
/// screen; `total` is `(bytes, entries)` of the whole cache.
fn plan_eviction(
    mut candidates: Vec<(u64, u64, usize)>,
    total: (usize, usize),
    budget: Budget,
) -> Vec<u64> {
    let (mut bytes, mut entries) = total;
    if bytes <= budget.bytes && entries <= budget.entries {
        return Vec::new();
    }
    candidates.sort_unstable();
    let mut out = Vec::new();
    for (_, key, size) in candidates {
        if bytes <= budget.bytes && entries <= budget.entries {
            break;
        }
        bytes = bytes.saturating_sub(size);
        entries = entries.saturating_sub(1);
        out.push(key);
    }
    out
}

/// What a display-size image is decoded from.
#[derive(Clone, Debug)]
pub(super) enum SizedSource {
    /// An image file (avatars).
    Path(Arc<Path>),
    /// An inline JPEG (chat-list photo thumbnails).
    Mini(Arc<MiniThumbnail>),
}

impl PartialEq for SizedSource {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Path(a), Self::Path(b)) => a == b,
            (Self::Mini(a), Self::Mini(b)) => Arc::ptr_eq(a, b) || a.data == b.data,
            _ => false,
        }
    }
}

impl Eq for SizedSource {}

impl Hash for SizedSource {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::Path(path) => {
                0_u8.hash(state);
                path.hash(state);
            }
            Self::Mini(mini) => {
                1_u8.hash(state);
                mini.data.hash(state);
            }
        }
    }
}

/// A source decoded so its shorter side is at most `edge` device pixels.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct SizedKey {
    source: SizedSource,
    edge: u32,
}

/// An image drawn `size` wide and high (the shorter side, for non-square
/// ones), decoded at that size in device pixels rather than at file size.
/// Inside a `CacheScope` (the main window's or a chat window's) it goes
/// through that bounded cache; elsewhere through GPUI's asset cache.
pub(super) fn sized_image(source: SizedSource, size: Pixels) -> ImageSource {
    sized_image_with(source, move |scale_factor| display_edge(size, scale_factor))
}

fn sized_image_with(
    source: SizedSource,
    edge: impl Fn(f32) -> u32 + Send + Sync + 'static,
) -> ImageSource {
    ImageSource::Custom(Arc::new(move |window: &mut Window, cx: &mut App| {
        let key = SizedKey {
            source: source.clone(),
            edge: edge(window.scale_factor()),
        };
        match SCOPED.with(|scoped| scoped.borrow().last().map(|(cache, _)| cache.clone())) {
            Some(cache) => cache.update(cx, |cache, cx| cache.load_sized(&key, window, cx)),
            None => window.use_asset::<SizedAsset>(&key, cx),
        }
    }))
}

/// Device pixels for a logical size.
fn display_edge(size: Pixels, scale_factor: f32) -> u32 {
    ((size / px(1.)) * scale_factor).ceil().clamp(1., 4096.) as u32
}

/// How a frame shows its picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fit {
    /// Fills the frame, cropping the overflow (photos, posters, tiles).
    Cover,
    /// Fits whole inside the frame (stickers).
    Contain,
}

/// A history picture (photo, poster, album tile, link preview, sticker)
/// drawn in a `frame` of logical size, decoded no larger than that needs
/// (device pixels), through the bounded cache. `dims` is the picture's
/// own size when known, which keeps the decode tight for frames whose
/// aspect differs from the picture's.
pub(super) fn sized_media(
    path: &Path,
    frame: (Pixels, Pixels),
    dims: Option<(i32, i32)>,
    fit: Fit,
) -> ImageSource {
    let logical = shorter_side_in_frame((frame.0 / px(1.), frame.1 / px(1.)), dims, fit);
    sized_image_with(SizedSource::Path(Arc::from(path)), move |scale_factor| {
        bucket_edge(display_edge(px(logical), scale_factor))
    })
}

/// A picture file at its own size (within `MAX_TEXTURE_EDGE`), for the
/// media viewer: zooming in needs every pixel. Decoded through
/// `image_decode`, so HEIC, AVIF, JPEG XL and colour profiles work.
pub(super) fn full_picture(path: &Path) -> ImageSource {
    sized_image_with(SizedSource::Path(Arc::from(path)), |_| {
        DecodeSize::FULL.short_edge
    })
}

/// Logical size the picture's shorter side is drawn at in the frame.
/// Without `dims` the bound over every possible aspect.
fn shorter_side_in_frame((fw, fh): (f32, f32), dims: Option<(i32, i32)>, fit: Fit) -> f32 {
    match (dims, fit) {
        (Some((iw, ih)), _) if iw > 0 && ih > 0 => {
            let (sx, sy) = (fw / iw as f32, fh / ih as f32);
            let scale = match fit {
                Fit::Cover => sx.max(sy),
                Fit::Contain => sx.min(sy),
            };
            scale * iw.min(ih) as f32
        }
        (_, Fit::Cover) => fw.max(fh),
        (_, Fit::Contain) => fw.min(fh),
    }
}

/// Round a wanted edge up to a step, so a resize within one step (or a
/// tiny rounding difference between two frames) asks for the same decode:
/// 32 px up to 256, 64 px up to 1024, then 128 px.
fn bucket_edge(edge: u32) -> u32 {
    let step = match edge {
        0..=256 => 32,
        257..=1024 => 64,
        _ => 128,
    };
    (edge.div_ceil(step) * step).clamp(step, 4096)
}

/// What to do for a wanted edge when other sizes of the same picture are
/// loaded.
#[derive(Debug, PartialEq, Eq)]
enum VariantPick {
    /// One is sharp enough and not much bigger: use it (a small shrink
    /// doesn't decode again).
    Reuse(u32),
    /// Decode the wanted size; show this loaded one meanwhile, if any.
    Decode(Option<u32>),
}

/// Loaded variants up to this factor (3/2) above the wanted edge are reused.
const REUSE_UP_TO: (u32, u32) = (3, 2);

fn pick_variant(wanted: u32, loaded: &[u32]) -> VariantPick {
    let reuse = loaded
        .iter()
        .copied()
        // u64: the full-size key (`u32::MAX`) must not overflow.
        .filter(|&edge| {
            edge >= wanted
                && u64::from(edge) * u64::from(REUSE_UP_TO.1)
                    <= u64::from(wanted) * u64::from(REUSE_UP_TO.0)
        })
        .min();
    if let Some(edge) = reuse {
        return VariantPick::Reuse(edge);
    }
    VariantPick::Decode(
        loaded
            .iter()
            .copied()
            .min_by_key(|&edge| edge.abs_diff(wanted)),
    )
}

fn decode_sized(key: &SizedKey) -> Result<Arc<RenderImage>, ImageCacheError> {
    let size = DecodeSize {
        short_edge: key.edge,
    };
    let mut image = match &key.source {
        SizedSource::Path(path) => image_decode::decode_file(path, size),
        SizedSource::Mini(mini) => image_decode::decode_image(&mini.data, None, size),
    }
    .map_err(|err| ImageCacheError::Asset(err.to_string().into()))?;
    // GPUI's sprites are BGRA.
    for pixel in image.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Ok(Arc::new(RenderImage::new(SmallVec::from_buf([
        image::Frame::new(image),
    ]))))
}

/// GPUI asset loader for display-size images outside the bounded cache.
enum SizedAsset {}

impl Asset for SizedAsset {
    type Source = SizedKey;
    type Output = Result<Arc<RenderImage>, ImageCacheError>;

    #[allow(clippy::manual_async_fn)]
    fn load(
        source: Self::Source,
        _: &mut App,
    ) -> impl std::future::Future<Output = Self::Output> + Send + 'static {
        async move { decode_sized(&source) }
    }
}

/// The textures the vendored GPUI atlas allocates for images (`TileClass`
/// in its backends): pictures up to `SMALL_IMAGE` px a side share
/// `SMALL_TEXTURE`² textures, larger ones `ATLAS_TEXTURE`² textures, and a
/// picture larger than that gets a texture of its own size.
const SMALL_IMAGE: u32 = 128;
const SMALL_TEXTURE: u32 = 512;
const ATLAS_TEXTURE: u32 = 1024;

/// What a decoded image costs: its pixels, or, for a small one, the share
/// of an atlas texture it takes up. A texture holds `cols x rows` tiles of
/// that size at best, so a 768x576 image uses a whole 4 MB texture for
/// 1.7 MB of pixels (and the texture only goes once its last tile does).
fn atlas_cost(width: u32, height: u32) -> usize {
    let pixels = width as usize * height as usize * 4;
    let side = if width <= SMALL_IMAGE && height <= SMALL_IMAGE {
        SMALL_TEXTURE
    } else {
        ATLAS_TEXTURE
    };
    if width == 0 || height == 0 || width > side || height > side {
        return pixels;
    }
    let tiles = (side / width) as usize * (side / height) as usize;
    let texture = side as usize * side as usize * 4;
    pixels.max(texture / tiles)
}

type LoadResult = Result<Arc<RenderImage>, ImageCacheError>;

/// A display-size decode on the background executor; views that asked
/// while it ran are notified when it lands (as GPUI's own image loads).
struct SizedLoad {
    state: Rc<RefCell<SizedState>>,
    _task: Task<()>,
}

enum SizedState {
    Loading(SmallVec<[EntityId; 2]>),
    Loaded(LoadResult),
}

impl SizedLoad {
    fn new(key: SizedKey, cx: &mut App) -> Self {
        let state = Rc::new(RefCell::new(SizedState::Loading(SmallVec::new())));
        let decode = cx
            .background_executor()
            .spawn(async move { decode_sized(&key) });
        let weak = Rc::downgrade(&state);
        let task = cx.spawn(async move |cx| {
            let result = decode.await;
            let Some(state) = weak.upgrade() else {
                return;
            };
            let previous = std::mem::replace(&mut *state.borrow_mut(), SizedState::Loaded(result));
            if let SizedState::Loading(views) = previous {
                cx.update(|cx| {
                    for view in views {
                        cx.notify(view);
                    }
                });
            }
        });
        Self { state, _task: task }
    }

    fn get(&self) -> Option<LoadResult> {
        match &*self.state.borrow() {
            SizedState::Loading(_) => None,
            SizedState::Loaded(result) => Some(result.clone()),
        }
    }

    fn use_by(&self, view: EntityId) -> Option<LoadResult> {
        match &mut *self.state.borrow_mut() {
            SizedState::Loading(views) => {
                if !views.contains(&view) {
                    views.push(view);
                }
                None
            }
            SizedState::Loaded(result) => Some(result.clone()),
        }
    }
}

enum Load {
    /// GPUI's loader: the file at its own size.
    Full(ImageCacheItem),
    Sized(SizedLoad),
}

impl Load {
    fn get(&self) -> Option<LoadResult> {
        match self {
            Self::Full(item) => item.get(),
            Self::Sized(load) => load.get(),
        }
    }

    fn use_image(&self, window: &Window) -> Option<LoadResult> {
        match self {
            Self::Full(item) => item.use_image(window),
            Self::Sized(load) => load.use_by(window.current_view()),
        }
    }
}

struct Entry {
    load: Load,
    /// Decoded size once loaded (0 for failures).
    bytes: Option<usize>,
    /// The last frame each scope (slice, or `None` for the app) used it.
    uses: SmallVec<[(Option<EntityId>, u64); 2]>,
}

impl Entry {
    fn last_used(&self) -> u64 {
        self.uses.iter().map(|(_, at)| *at).max().unwrap_or(0)
    }

    fn may_be_visible(&self, window: Option<ChatId>) -> bool {
        self.uses
            .iter()
            .any(|(scope, used)| may_be_visible(window, *scope, *used))
    }
}

/// A window's image cache: least recently used images that can't be on
/// screen go once the decoded total passes the budget.
pub(super) struct BoundedImageCache {
    entries: HashMap<u64, Entry>,
    /// The display-size decodes of each picture: source hash -> (edge,
    /// entry key). Only entries still in `entries` (`evict` drops the
    /// rest), loaded or not; check which loaded against `entries`.
    variants: HashMap<u64, SmallVec<[(u32, u64); 2]>>,
    bytes: usize,
    /// How many entries may have been on screen at the last `trim`.
    shown: usize,
    /// The slice being drawn (shared with `app_slice`).
    scope: Rc<Cell<Option<EntityId>>>,
    /// The window it serves (`None`: the main window).
    window: Option<ChatId>,
}

impl BoundedImageCache {
    pub(super) fn new(scope: Rc<Cell<Option<EntityId>>>) -> Self {
        SCOPE.with(|current| *current.borrow_mut() = Some(scope.clone()));
        Self::in_window(scope, None)
    }

    pub(super) fn for_window(chat: ChatId, scope: Rc<Cell<Option<EntityId>>>) -> Self {
        Self::in_window(scope, Some(chat))
    }

    fn in_window(scope: Rc<Cell<Option<EntityId>>>, window: Option<ChatId>) -> Self {
        Self {
            entries: HashMap::new(),
            variants: HashMap::new(),
            bytes: 0,
            shown: 0,
            scope,
            window,
        }
    }

    /// Start a frame of the chat window this cache serves, before anything
    /// in it renders, and evict down to the scrolling budget (`trim`).
    /// What its last frame showed may leave the atlas once this one is
    /// done.
    pub(super) fn begin_window_frame(
        &mut self,
        draw: WindowDraw,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(chat) = self.window {
            begin_chat_frame(chat, draw);
        }
        self.trim(window, cx);
        retired::wake();
    }

    /// Evict down to the scrolling budget. Call at the start of a frame,
    /// before anything renders, so last frame's loads are all counted.
    pub(super) fn trim(&mut self, window: &mut Window, cx: &mut App) {
        let shown = self
            .entries
            .values()
            .filter(|e| e.may_be_visible(self.window))
            .count();
        note_departures(&mut self.shown, shown);
        drop_images_off_ui_thread(self.evict(ACTIVE_BUDGET), Some(window), cx);
    }

    /// Nothing new showed for `IDLE_AFTER`: evict down to the idle budget,
    /// or, with the window in the `background`, to what is on screen, and
    /// take what is kept off screen out of the atlas. Runs between frames
    /// (no window is being drawn), so the images leave every window's
    /// atlas.
    pub(super) fn idle_trim(&mut self, background: bool, cx: &mut App) {
        IDLE_TRIMMED.with(|trimmed| trimmed.set(ACTIVITY.with(Cell::get)));
        let before = (self.bytes, self.entries.len());
        let none = Budget {
            bytes: 0,
            entries: 0,
        };
        let images = self.evict(if background { none } else { IDLE_BUDGET });
        if super::frame_clock::trace_ticks() {
            eprintln!(
                "image idle trim: {} images, {:.1} MB -> {} images, {:.1} MB",
                before.1,
                before.0 as f64 / 1048576.0,
                self.entries.len(),
                self.bytes as f64 / 1048576.0
            );
        }
        // What stays off screen keeps its decode, so coming back to it only
        // uploads it again, but not its atlas tile: a kept photo would
        // otherwise hold a 4 MB texture of its own for as long as it stays.
        let off_screen = self
            .entries
            .values()
            .filter(|entry| !entry.may_be_visible(self.window))
            .filter_map(|entry| entry.load.get()?.ok());
        for image in images.into_iter().chain(off_screen) {
            cx.drop_image(image, None);
        }
        // Even with nothing evicted here: what left the screen (a closed
        // viewer's full-size photo, a chat's sticker frames) was freed
        // meanwhile.
        super::free_heap::release();
    }

    /// Remove entries past `budget`; their loaded images, for the caller
    /// to drop from the atlas.
    fn evict(&mut self, budget: Budget) -> Vec<Arc<RenderImage>> {
        if self.bytes <= budget.bytes && self.entries.len() <= budget.entries {
            return Vec::new();
        }
        let candidates = self
            .entries
            .iter()
            .filter(|(_, entry)| !entry.may_be_visible(self.window))
            .map(|(key, entry)| (entry.last_used(), *key, entry.bytes.unwrap_or(0)))
            .collect();
        let mut images = Vec::new();
        for key in plan_eviction(candidates, (self.bytes, self.entries.len()), budget) {
            let Some(entry) = self.entries.remove(&key) else {
                continue;
            };
            self.bytes -= entry.bytes.unwrap_or(0);
            if let Some(Ok(image)) = entry.load.get() {
                images.push(image);
            }
        }
        // Every picture ever shown at display size left its sizes here.
        let entries = &self.entries;
        self.variants.retain(|_, tracked| {
            tracked.retain(|(_, hash)| entries.contains_key(hash));
            !tracked.is_empty()
        });
        images
    }

    fn load_entry(
        &mut self,
        key: u64,
        start: impl FnOnce(&mut App) -> Load,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<LoadResult> {
        let (scope, now) = (self.scope.get(), frame());
        let entry = self.entries.entry(key).or_insert_with(|| Entry {
            load: start(cx),
            bytes: None,
            uses: SmallVec::new(),
        });
        let previous = match entry.uses.iter_mut().find(|(s, _)| *s == scope) {
            Some(use_) => Some(std::mem::replace(&mut use_.1, now)),
            None => {
                entry.uses.push((scope, now));
                None
            }
        };
        if previous != Some(now) && newly_shown(self.window, scope, previous) {
            note_activity();
        }
        let result = entry.load.use_image(window);
        if entry.bytes.is_none()
            && let Some(result) = &result
        {
            let bytes = match result {
                Ok(image) => (0..image.frame_count())
                    .map(|frame| {
                        let size = image.size(frame);
                        atlas_cost(size.width.0.max(0) as u32, size.height.0.max(0) as u32)
                    })
                    .sum(),
                Err(_) => 0,
            };
            entry.bytes = Some(bytes);
            self.bytes += bytes;
        }
        result
    }

    /// A display-size decode. When the same picture is already decoded at
    /// a nearby larger size, that one serves (so a small change of the
    /// frame doesn't decode again); while a new size decodes, the closest
    /// loaded one stands in, so a resize doesn't blank the picture.
    fn load_sized(
        &mut self,
        key: &SizedKey,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<LoadResult> {
        let source_hash = gpui_kit::hash(&("sized-source", &key.source));
        let loaded: SmallVec<[(u32, u64); 2]> = self
            .variants
            .get(&source_hash)
            .into_iter()
            .flatten()
            .copied()
            .filter(|(_, hash)| {
                self.entries
                    .get(hash)
                    .is_some_and(|entry| matches!(entry.load.get(), Some(Ok(_))))
            })
            .collect();
        let edges: SmallVec<[u32; 2]> = loaded.iter().map(|(edge, _)| *edge).collect();
        let pick = pick_variant(key.edge, &edges);
        let sized_key = |edge: u32| SizedKey {
            source: key.source.clone(),
            edge,
        };
        let variant_hash = |edge: u32| loaded.iter().find(|(e, _)| *e == edge).map(|(_, h)| *h);
        if let VariantPick::Reuse(edge) = pick
            && let Some(hash) = variant_hash(edge)
        {
            let reused = sized_key(edge);
            return self.load_entry(
                hash,
                move |cx| Load::Sized(SizedLoad::new(reused, cx)),
                window,
                cx,
            );
        }
        let hash = gpui_kit::hash(&("sized", key));
        let wanted = key.clone();
        let result = self.load_entry(
            hash,
            move |cx| Load::Sized(SizedLoad::new(wanted, cx)),
            window,
            cx,
        );
        let tracked = self.variants.entry(source_hash).or_default();
        if !tracked.iter().any(|(_, h)| *h == hash) {
            tracked.push((key.edge, hash));
        }
        if result.is_some() {
            return result;
        }
        let VariantPick::Decode(Some(edge)) = pick else {
            return None;
        };
        let hash = variant_hash(edge)?;
        let standing = sized_key(edge);
        self.load_entry(
            hash,
            move |cx| Load::Sized(SizedLoad::new(standing, cx)),
            window,
            cx,
        )
    }
}

impl ImageCache for BoundedImageCache {
    fn load(
        &mut self,
        resource: &Resource,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<LoadResult> {
        self.load_entry(
            hash(resource),
            |cx| Load::Full(ImageCacheItem::new(resource, cx)),
            window,
            cx,
        )
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "demo-capture")]
    impl super::BoundedImageCache {
        /// Tests elsewhere: the image `resource` loaded as, once loaded.
        pub(in crate::ui) fn loaded(
            &self,
            resource: &gpui_kit::Resource,
        ) -> Option<std::sync::Arc<gpui_kit::RenderImage>> {
            self.entries
                .get(&gpui_kit::hash(resource))?
                .load
                .get()?
                .ok()
        }

        /// Tests elsewhere: every image it holds loaded.
        pub(in crate::ui) fn loaded_images(&self) -> Vec<std::sync::Arc<gpui_kit::RenderImage>> {
            self.entries
                .values()
                .filter_map(|entry| entry.load.get()?.ok())
                .collect()
        }
    }

    use super::note_departures;
    use super::{
        Budget, IDLE_AFTER, PaintStamp, WindowDraw, begin_chat_frame, begin_frame,
        begin_patch_frame, forget_window, idle_trim_due, may_be_visible, may_still_show,
        newly_shown, note_activity, plan_eviction, shows_now, slice_rendered, slice_shown,
    };
    use super::{Fit, VariantPick, atlas_cost, bucket_edge, pick_variant, shorter_side_in_frame};
    use gpui_kit::{EntityId, RenderImage};
    use quill::ids::ChatId;
    use smallvec::SmallVec;
    use std::sync::Arc;

    #[test]
    fn app_images_stay_for_one_frame() {
        let used = begin_frame();
        assert!(may_be_visible(None, None, used));
        begin_frame();
        assert!(
            may_be_visible(None, None, used),
            "last frame may still show it"
        );
        begin_frame();
        assert!(!may_be_visible(None, None, used));
    }

    #[test]
    fn slice_images_stay_until_the_slice_renders_again() {
        let slice = EntityId::from((1_u64 << 32) | 7);
        let used = begin_frame();
        slice_rendered(slice, None);
        // Replayed (shown, not rendered) for a while: still on screen.
        for _ in 0..5 {
            begin_frame();
            assert!(may_be_visible(None, Some(slice), used));
            slice_shown(slice, None);
        }
        // Rendered again without it: gone from the next frame on.
        slice_rendered(slice, None);
        begin_frame();
        assert!(!may_be_visible(None, Some(slice), used));
    }

    /// A frame that redraws only an animation layer leaves all else on
    /// screen, as the last whole frame drew it, however many such frames
    /// pass; only the layer's last render goes.
    #[test]
    fn what_a_patch_frame_does_not_draw_stays_on_screen() {
        let slice = EntityId::from((1_u64 << 32) | 23);
        let layer = EntityId::from((1_u64 << 32) | 25);
        let whole = begin_frame();
        slice_rendered(slice, None);
        slice_rendered(layer, None);
        for _ in 0..10 {
            begin_patch_frame();
            slice_rendered(layer, None);
        }
        let painted = |scope| PaintStamp {
            window: None,
            scope,
            frame: whole,
        };
        for scope in [None, Some(slice)] {
            assert!(may_be_visible(None, scope, whole), "{scope:?} may show");
            assert!(shows_now(painted(scope)), "{scope:?} shows");
        }
        assert!(!newly_shown(None, None, Some(whole)), "and was not away");
        assert!(
            !may_be_visible(None, Some(layer), whole),
            "the layer drew since"
        );
        assert!(!shows_now(painted(Some(layer))));
        let next = begin_frame();
        slice_shown(slice, None);
        slice_rendered(layer, None);
        assert!(
            may_be_visible(None, None, whole),
            "until the next one is done"
        );
        assert!(!shows_now(painted(None)));
        assert!(shows_now(painted(Some(slice))), "the slice replays it");
        begin_patch_frame();
        assert!(
            !may_be_visible(None, None, whole),
            "whole frame {next} is on"
        );
        assert!(may_be_visible(None, Some(slice), whole));
    }

    /// Between frames only the latest frame is on screen: what the frame
    /// before drew and the latest did not (a closed panel) is gone.
    #[test]
    fn what_a_chat_windows_slice_replays_stays_on_screen() {
        let chat = ChatId(5);
        let slice = EntityId::from(0x51_u64);
        let used = begin_chat_frame(chat, WindowDraw::Whole);
        slice_rendered(slice, Some(chat));
        for _ in 0..3 {
            begin_frame();
        }
        assert!(may_be_visible(Some(chat), Some(slice), used));
        for _ in 0..3 {
            begin_chat_frame(chat, WindowDraw::Whole);
            slice_shown(slice, Some(chat));
        }
        assert!(may_be_visible(Some(chat), Some(slice), used));
        let rendered = begin_chat_frame(chat, WindowDraw::Whole);
        slice_rendered(slice, Some(chat));
        begin_chat_frame(chat, WindowDraw::Whole);
        slice_shown(slice, Some(chat));
        assert!(!may_be_visible(Some(chat), Some(slice), used));
        assert!(may_be_visible(Some(chat), Some(slice), rendered));
    }

    #[test]
    fn a_chat_window_counts_its_own_whole_frames() {
        let chat = ChatId(5);
        let used = begin_chat_frame(chat, WindowDraw::Whole);
        let stamp = PaintStamp {
            window: Some(chat),
            scope: None,
            frame: used,
        };
        for _ in 0..3 {
            begin_chat_frame(chat, WindowDraw::Patch);
            begin_frame();
        }
        assert!(may_be_visible(Some(chat), None, used));
        assert!(shows_now(stamp));
        begin_chat_frame(chat, WindowDraw::Whole);
        assert!(!shows_now(stamp));
    }

    #[test]
    fn a_closed_window_shows_nothing() {
        let chat = ChatId(5);
        let slice = EntityId::from(0x51_u64);
        let frame = begin_chat_frame(chat, WindowDraw::Whole);
        slice_rendered(slice, Some(chat));
        let (own, in_slice) = (
            PaintStamp {
                window: Some(chat),
                scope: None,
                frame,
            },
            PaintStamp {
                window: Some(chat),
                scope: Some(slice),
                frame,
            },
        );
        assert!(shows_now(own) && shows_now(in_slice));
        forget_window(chat);
        assert!(!shows_now(own) && !shows_now(in_slice));
        assert!(!may_still_show(own) && !may_still_show(in_slice));
    }

    #[test]
    fn shows_now_is_the_latest_frame_or_the_slice_it_replays() {
        let slice = EntityId::from((1_u64 << 32) | 17);
        let first = begin_frame();
        slice_rendered(slice, None);
        let painted = |scope| PaintStamp {
            window: None,
            scope,
            frame: first,
        };
        assert!(shows_now(painted(None)) && shows_now(painted(Some(slice))));
        begin_frame();
        slice_shown(slice, None);
        assert!(
            !shows_now(painted(None)),
            "the latest frame drew without it"
        );
        assert!(
            may_be_visible(None, None, first),
            "unlike at the start of a frame"
        );
        assert!(shows_now(painted(Some(slice))), "the slice replays it");
        begin_frame();
        slice_rendered(slice, None);
        assert!(
            !shows_now(painted(Some(slice))),
            "rendered again without it"
        );
    }

    #[test]
    fn hidden_slices_show_nothing() {
        let slice = EntityId::from((1_u64 << 32) | 9);
        let used = begin_frame();
        slice_rendered(slice, None);
        begin_frame();
        begin_frame();
        assert!(!may_be_visible(None, Some(slice), used));
    }

    #[test]
    fn rerendering_the_same_images_is_not_activity() {
        let slice = EntityId::from((1_u64 << 32) | 11);
        let first = begin_frame();
        slice_rendered(slice, None);
        assert!(newly_shown(None, Some(slice), None), "a new image");
        // Replayed for a while, then rendered again showing it: unchanged.
        for _ in 0..10 {
            begin_frame();
            slice_shown(slice, None);
        }
        let second = begin_frame();
        slice_rendered(slice, None);
        assert!(!newly_shown(None, Some(slice), Some(first)));
        // An image last shown two renders ago scrolled back in.
        begin_frame();
        slice_rendered(slice, None);
        assert!(newly_shown(None, Some(slice), Some(first)));
        assert!(!newly_shown(None, Some(slice), Some(second)));
        // App-level images: shown last frame, or not.
        let now = begin_frame();
        assert!(!newly_shown(None, None, Some(now - 1)));
        assert!(newly_shown(None, None, Some(now - 2)));
    }

    /// The browse finding: photos of a chat left for one without stayed at
    /// the scrolling budget, because only newly shown images counted as
    /// activity and no idle trim was ever scheduled.
    #[test]
    fn leaving_a_chat_of_photos_schedules_the_idle_trim() {
        let chat = EntityId::from((1_u64 << 32) | 17);
        let photos = begin_frame();
        slice_rendered(chat, None);
        begin_frame();
        slice_shown(chat, None);
        let on_screen = |used: &[u64]| {
            used.iter()
                .filter(|u| may_be_visible(None, Some(chat), **u))
                .count()
        };
        let used = [photos; 3];
        let mut shown = 0;
        note_departures(&mut shown, on_screen(&used));
        assert_eq!(shown, 3);
        assert_eq!(
            idle_trim_due(),
            None,
            "arriving photos note activity elsewhere"
        );
        // Replayed for a while: nothing changed.
        begin_frame();
        slice_shown(chat, None);
        note_departures(&mut shown, on_screen(&used));
        assert_eq!(idle_trim_due(), None);
        // Another chat opens: the slice renders without the photos.
        slice_rendered(chat, None);
        begin_frame();
        slice_shown(chat, None);
        note_departures(&mut shown, on_screen(&used));
        assert_eq!(shown, 0);
        assert!(
            idle_trim_due().is_some(),
            "no idle trim after the photos left"
        );
    }

    #[test]
    fn idle_trim_waits_for_quiet_and_runs_once_per_change() {
        note_activity();
        let due = idle_trim_due().expect("activity schedules a trim");
        assert!(
            due >= std::time::Instant::now()
                + IDLE_AFTER.saturating_sub(std::time::Duration::from_millis(50))
        );
        super::IDLE_TRIMMED.with(|trimmed| trimmed.set(super::ACTIVITY.with(|a| a.get())));
        assert_eq!(idle_trim_due(), None, "already trimmed");
        note_activity();
        assert!(idle_trim_due().is_some());
    }

    /// The browse finding: two photos the idle budget kept off screen held
    /// a 4 MB atlas texture each. The idle trim keeps their decode and takes
    /// them out of the atlas; showing one again uploads that same decode.
    /// A picture still on screen keeps its tile.
    #[cfg(feature = "demo-capture")]
    #[gpui_kit::test]
    fn idle_trim_takes_kept_pictures_out_of_the_atlas(cx: &mut gpui_kit::TestAppContext) {
        use super::{BoundedImageCache, CacheScope};
        use gpui_kit::test::TestWindowExt;
        use gpui_kit::{
            AppContext, Context, Entity, IntoElement, ParentElement, Render, Resource, Styled,
            VisualTestContext, Window, div, img, px, size,
        };
        use std::path::PathBuf;

        struct Pictures {
            cache: Entity<BoundedImageCache>,
            on_screen: PathBuf,
            left: PathBuf,
            show_left: bool,
        }
        impl Render for Pictures {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                begin_frame();
                let left = self.show_left.then(|| img(self.left.clone()).size(px(40.)));
                let pictures = div()
                    .size_full()
                    .child(img(self.on_screen.clone()).size(px(40.)))
                    .children(left);
                CacheScope::new(self.cache.clone(), pictures)
            }
        }
        /// The pictures' files, removed however the test ends.
        struct Files(PathBuf);
        impl Drop for Files {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        cx.update(gpui_kit::init);
        let files = Files(std::env::temp_dir().join(format!("quill-atlas-{}", std::process::id())));
        std::fs::create_dir_all(&files.0).expect("a folder for the pictures");
        let [on_screen, left] = ["on-screen.png", "left.png"].map(|name| files.0.join(name));
        for path in [&on_screen, &left] {
            image::RgbaImage::new(8, 8)
                .save(path)
                .expect("write the picture");
        }
        let view_paths = (on_screen.clone(), left.clone());
        let handle = cx.open_window(size(px(200.), px(200.)), move |_, cx| {
            let cache = cx.new(|_| BoundedImageCache::new(Default::default()));
            Pictures {
                cache,
                on_screen: view_paths.0,
                left: view_paths.1,
                show_left: true,
            }
        });
        let view = handle.root(cx).expect("the view");
        let cache = view.read_with(cx, |view, _| view.cache.clone());
        let mut vcx = VisualTestContext::from_window(handle.into(), cx);
        let frames = |vcx: &mut VisualTestContext| {
            for _ in 0..3 {
                vcx.run_until_parked();
                vcx.update(|window, cx| window.render_frame(cx));
            }
            vcx.run_until_parked();
        };
        let loaded = |vcx: &VisualTestContext, path: &PathBuf| {
            let resource = Resource::Path(path.clone().into());
            cache.read_with(vcx, |cache, _| cache.loaded(&resource))
        };
        frames(&mut vcx);
        let kept = loaded(&vcx, &on_screen).expect("loaded");
        let image = loaded(&vcx, &left).expect("loaded");
        let in_atlas = |vcx: &mut VisualTestContext,
                        image: &std::sync::Arc<gpui_kit::RenderImage>| {
            vcx.update(|window, _| window.has_image_atlas_entry(image))
        };
        assert!(in_atlas(&mut vcx, &image));

        view.update(&mut vcx, |view, cx| {
            view.show_left = false;
            cx.notify();
        });
        frames(&mut vcx);
        cache.update(&mut vcx, |cache, cx| cache.idle_trim(false, cx));
        let still = loaded(&vcx, &left);
        assert!(still.is_some_and(|still| still.id == image.id), "kept");
        assert!(!in_atlas(&mut vcx, &image), "out of the atlas");
        assert!(in_atlas(&mut vcx, &kept), "on screen: left alone");

        view.update(&mut vcx, |view, cx| {
            view.show_left = true;
            cx.notify();
        });
        frames(&mut vcx);
        assert!(
            in_atlas(&mut vcx, &image),
            "the same decode, uploaded again"
        );
    }

    /// A chat window's cache counts that window's frames. The main window
    /// drawing on doesn't make what the chat window shows look off screen
    /// (with the main window's count, the idle trim took its pictures),
    /// and what it stopped showing goes at the idle trim.
    #[cfg(feature = "demo-capture")]
    #[gpui_kit::test]
    fn a_chat_window_cache_counts_its_own_frames(cx: &mut gpui_kit::TestAppContext) {
        use super::{BoundedImageCache, CacheScope};
        use gpui_kit::test::TestWindowExt;
        use gpui_kit::{
            AppContext, Context, Entity, IntoElement, ParentElement, Render, Resource, Styled,
            VisualTestContext, Window, div, img, px, size,
        };
        use std::path::PathBuf;

        struct ChatWindow {
            cache: Entity<BoundedImageCache>,
            on_screen: PathBuf,
            left: PathBuf,
            show_left: bool,
        }
        impl Render for ChatWindow {
            fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                self.cache.update(cx, |cache, cx| {
                    cache.begin_window_frame(super::WindowDraw::Whole, window, cx)
                });
                let left = self.show_left.then(|| img(self.left.clone()).size(px(40.)));
                let pictures = div()
                    .size_full()
                    .child(img(self.on_screen.clone()).size(px(40.)))
                    .children(left);
                CacheScope::new(self.cache.clone(), pictures)
            }
        }
        struct Files(PathBuf);
        impl Drop for Files {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        cx.update(gpui_kit::init);
        let files =
            Files(std::env::temp_dir().join(format!("quill-window-{}", std::process::id())));
        std::fs::create_dir_all(&files.0).expect("a folder for the pictures");
        let [on_screen, left] = ["on-screen.png", "left.png"].map(|name| files.0.join(name));
        for path in [&on_screen, &left] {
            image::RgbaImage::new(8, 8)
                .save(path)
                .expect("write the picture");
        }
        let view_paths = (on_screen.clone(), left.clone());
        let handle = cx.open_window(size(px(200.), px(200.)), move |_, cx| ChatWindow {
            cache: cx
                .new(|_| BoundedImageCache::for_window(quill::ids::ChatId(1), Default::default())),
            on_screen: view_paths.0,
            left: view_paths.1,
            show_left: true,
        });
        let view = handle.root(cx).expect("the view");
        let cache = view.read_with(cx, |view, _| view.cache.clone());
        let mut vcx = VisualTestContext::from_window(handle.into(), cx);
        let frames = |vcx: &mut VisualTestContext| {
            for _ in 0..3 {
                vcx.run_until_parked();
                vcx.update(|window, cx| window.render_frame(cx));
            }
            vcx.run_until_parked();
        };
        let loaded = |vcx: &VisualTestContext, path: &PathBuf| {
            let resource = Resource::Path(path.clone().into());
            cache.read_with(vcx, |cache, _| cache.loaded(&resource))
        };
        let in_atlas = |vcx: &mut VisualTestContext, image: &Arc<RenderImage>| {
            vcx.update(|window, _| window.has_image_atlas_entry(image))
        };
        frames(&mut vcx);
        let kept = loaded(&vcx, &on_screen).expect("loaded");
        let image = loaded(&vcx, &left).expect("loaded");

        // The main window plays an animation for a while; this one has
        // nothing new to draw.
        for _ in 0..120 {
            begin_frame();
        }
        cache.update(&mut vcx, |cache, cx| cache.idle_trim(true, cx));
        assert!(loaded(&vcx, &left).is_some(), "still shown: kept");
        assert!(in_atlas(&mut vcx, &kept), "still shown: in the atlas");

        view.update(&mut vcx, |view, cx| {
            view.show_left = false;
            cx.notify();
        });
        frames(&mut vcx);
        cache.update(&mut vcx, |cache, cx| cache.idle_trim(true, cx));
        assert!(loaded(&vcx, &left).is_none(), "no longer shown: gone");
        assert!(!in_atlas(&mut vcx, &image), "and out of the atlas");
        assert!(in_atlas(&mut vcx, &kept), "on screen: left alone");
    }

    #[cfg(feature = "demo-capture")]
    #[gpui_kit::test]
    fn evicted_sizes_leave_the_index(cx: &mut gpui_kit::TestAppContext) {
        use super::{BoundedImageCache, Entry, Load, SizedKey, SizedLoad, SizedSource};
        cx.update(|cx| {
            let mut cache = BoundedImageCache::new(Default::default());
            let source = SizedSource::Path(std::path::Path::new("gone.jpg").into());
            for (edge, key) in [(64, 1), (128, 2)] {
                let sized = SizedKey {
                    source: source.clone(),
                    edge,
                };
                let entry = Entry {
                    load: Load::Sized(SizedLoad::new(sized, cx)),
                    bytes: Some(10),
                    uses: SmallVec::new(),
                };
                cache.entries.insert(key, entry);
                cache.bytes += 10;
            }
            cache
                .variants
                .insert(7, SmallVec::from_slice(&[(64, 1), (128, 2)]));
            let one = Budget {
                bytes: 10,
                entries: 1,
            };
            cache.evict(one);
            assert_eq!(cache.entries.len(), 1);
            assert_eq!(cache.variants.get(&7).map(|v| v.len()), Some(1));
            let none = Budget {
                bytes: 0,
                entries: 0,
            };
            cache.evict(none);
            assert!(cache.variants.is_empty(), "nothing left to index");
        });
    }

    #[test]
    fn eviction_takes_the_oldest_until_within_budget() {
        let budget = Budget {
            bytes: 100,
            entries: 10,
        };
        // (last used, key, bytes)
        let candidates = vec![(5, 50, 40), (1, 10, 40), (3, 30, 40), (9, 90, 40)];
        assert_eq!(
            plan_eviction(candidates.clone(), (180, 6), budget),
            vec![10, 30]
        );
        assert_eq!(
            plan_eviction(candidates.clone(), (90, 6), budget),
            Vec::<u64>::new()
        );
        // Over the entry count only.
        assert_eq!(plan_eviction(candidates, (40, 12), budget), vec![10, 30]);
        // Not enough evictable: everything that can go, goes.
        assert_eq!(plan_eviction(vec![(1, 1, 10)], (500, 3), budget), vec![1]);
    }

    #[test]
    fn edges_round_up_to_steps() {
        assert_eq!(bucket_edge(1), 32);
        assert_eq!(bucket_edge(32), 32);
        assert_eq!(bucket_edge(33), 64);
        assert_eq!(bucket_edge(256), 256);
        assert_eq!(bucket_edge(257), 320);
        assert_eq!(bucket_edge(720), 768);
        assert_eq!(bucket_edge(800), 832);
        assert_eq!(bucket_edge(1025), 1152);
        assert_eq!(bucket_edge(9000), 4096);
        // Never below what was asked (until the cap), never 1.5x over.
        for edge in 1..=4096_u32 {
            let bucket = bucket_edge(edge);
            assert!(bucket >= edge);
            if edge >= 64 {
                assert!(bucket * 100 <= edge * 150, "{edge} -> {bucket}");
            }
        }
    }

    #[test]
    fn a_resize_within_a_step_keeps_the_decode() {
        // 360 pt at 2x, and the same frame a pixel off.
        assert_eq!(bucket_edge(720), bucket_edge(715));
        assert_eq!(bucket_edge(720), bucket_edge(768));
        assert_ne!(bucket_edge(720), bucket_edge(769));
    }

    #[test]
    fn nearby_larger_decodes_are_reused() {
        assert_eq!(pick_variant(768, &[]), VariantPick::Decode(None));
        // Same size, or up to 1.5x bigger: reuse the smallest such.
        assert_eq!(pick_variant(768, &[768]), VariantPick::Reuse(768));
        assert_eq!(pick_variant(768, &[1152, 896]), VariantPick::Reuse(896));
        assert_eq!(pick_variant(768, &[1152]), VariantPick::Reuse(1152));
        // Too big, or too small: decode, and show the closest meanwhile.
        assert_eq!(
            pick_variant(256, &[768, 128]),
            VariantPick::Decode(Some(128))
        );
        assert_eq!(pick_variant(256, &[768]), VariantPick::Decode(Some(768)));
        assert_eq!(pick_variant(768, &[320]), VariantPick::Decode(Some(320)));
        // The full-size key does not overflow.
        assert_eq!(
            pick_variant(u32::MAX, &[u32::MAX]),
            VariantPick::Reuse(u32::MAX)
        );
    }

    #[test]
    fn frame_edge_follows_the_fit() {
        // A photo in a frame of its own aspect: the frame's short side.
        let edge = shorter_side_in_frame((360., 270.), Some((2560, 1920)), Fit::Cover);
        assert!((edge - 270.).abs() < 0.01, "{edge}");
        // A tile cropping a landscape picture: the short side grows to
        // cover the tile.
        let edge = shorter_side_in_frame((100., 100.), Some((400, 300)), Fit::Cover);
        assert!((edge - 100.).abs() < 0.01, "{edge}");
        let edge = shorter_side_in_frame((200., 100.), Some((400, 300)), Fit::Cover);
        assert!((edge - 150.).abs() < 0.01, "{edge}");
        // Contain: the whole picture fits, so its short side is smaller.
        let edge = shorter_side_in_frame((128., 128.), Some((512, 256)), Fit::Contain);
        assert!((edge - 64.).abs() < 0.01, "{edge}");
        // Unknown size: the bound over every aspect.
        assert_eq!(shorter_side_in_frame((200., 100.), None, Fit::Cover), 200.);
        assert_eq!(
            shorter_side_in_frame((128., 128.), None, Fit::Contain),
            128.
        );
        assert_eq!(
            shorter_side_in_frame((200., 100.), Some((0, 0)), Fit::Cover),
            200.
        );
    }

    #[test]
    fn small_images_cost_their_atlas_share() {
        let texture = 1024 * 1024 * 4;
        // Larger than a texture: the pixels.
        assert_eq!(atlas_cost(2560, 1920), 2560 * 1920 * 4);
        assert_eq!(atlas_cost(1280, 960), 1280 * 960 * 4);
        // One per texture: the whole texture.
        assert_eq!(atlas_cost(768, 576), texture);
        assert_eq!(atlas_cost(1024, 1024), texture);
        // Two per row, one row.
        assert_eq!(atlas_cost(512, 576), texture / 2);
        // Just too big to be small: 7 x 7 to a texture.
        assert_eq!(atlas_cost(129, 129), texture / (7 * 7));
        // Small pictures share 1 MB textures; avatars a little above their
        // pixels.
        let small_texture = 512 * 512 * 4;
        assert_eq!(atlas_cost(128, 128), small_texture / (4 * 4));
        assert_eq!(atlas_cost(92, 92), small_texture / (5 * 5));
        assert!((36 * 36 * 4..36 * 36 * 4 * 11 / 10).contains(&atlas_cost(36, 36)));
        assert_eq!(atlas_cost(0, 5), 0);
    }
}
