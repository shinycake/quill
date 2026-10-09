//! Bounded image memory for the main window.
//!
//! GPUI keeps every image an `img(path)` ever loaded — decoded in RAM and
//! uploaded to the window's sprite atlas — for the life of the app, and an
//! `Arc<RenderImage>` painted once stays in the atlas until someone calls
//! `drop_image`. Scrolling through avatars and photos therefore only ever
//! grew the footprint. This module bounds both:
//!
//! - [`BoundedImageCache`] loads the window's path images (an `ImageCache`
//!   installed over `QuillApp` by [`CacheScope`]) and, past a byte budget,
//!   evicts the least recently used images that can no longer be on screen,
//!   dropping them from the atlas too. Once nothing new has been shown for
//!   a moment (`IDLE_AFTER`), it trims to a much smaller idle budget
//!   ([`BoundedImageCache::idle_trim`]), so the memory of a scroll comes
//!   back.
//! - [`sized_image`] loads small pictures (avatars, chat-list thumbnails)
//!   decoded at the size they are drawn at, times the window's scale
//!   factor, instead of at file size.
//! - [`retire_all`] takes `RenderImage`s Quill made itself (sticker frames,
//!   blurred previews…) once their cache lets go of them; [`sweep`] drops
//!   them from the atlas when no frame can still show them.
//!
//! "Can no longer be on screen" accounts for cached slices (`app_slice`):
//! a slice replays the frame it last rendered, so an image it used is only
//! gone once that slice rendered again without it. Images used outside any
//! slice are re-requested by `QuillApp` every frame.
//!
//! Atlas textures (1024² and up, on every GPUI backend: Metal, wgpu,
//! DirectX) are released only once every tile in them is gone. Tiles are
//! allocated in the newest texture with room, so images loaded together
//! share textures; evicting oldest-first empties whole textures rather
//! than punching holes in all of them.

use gpui_kit::*;
use quill::telegram::envelope::MiniThumbnail;
use smallvec::SmallVec;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

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
    /// Window frames drawn so far (`begin_frame`).
    static FRAME: Cell<u64> = const { Cell::new(0) };
    /// Each slice's last render frame, the last frame it was shown, and
    /// the render before the last one.
    static SLICES: RefCell<HashMap<EntityId, SliceFrames>> = RefCell::new(HashMap::new());
    /// Quill-made images waiting to leave the atlas, with the frame they
    /// were retired in.
    static RETIRED: RefCell<Vec<(Arc<RenderImage>, u64)>> = const { RefCell::new(Vec::new()) };
    /// When the cache last started showing an image it didn't show just
    /// before (scrolling, new content), and the activity the last idle
    /// trim covered.
    static ACTIVITY: Cell<Option<Instant>> = const { Cell::new(None) };
    static IDLE_TRIMMED: Cell<Option<Instant>> = const { Cell::new(None) };
    /// The slice being drawn (`app_slice`), for paint stamps.
    static SCOPE: RefCell<Option<Rc<Cell<Option<EntityId>>>>> = const { RefCell::new(None) };
    /// The cache of the `CacheScope` being drawn (display-size loads).
    static SCOPED: RefCell<Vec<Entity<BoundedImageCache>>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone, Copy, Debug, Default)]
struct SliceFrames {
    rendered: u64,
    shown: u64,
    previous_render: u64,
}

/// Start a window frame (`QuillApp::render`, before anything renders).
pub(super) fn begin_frame() -> u64 {
    FRAME.with(|frame| {
        frame.set(frame.get() + 1);
        frame.get()
    })
}

fn frame() -> u64 {
    FRAME.with(Cell::get)
}

/// A slice rendered its content this frame.
pub(super) fn slice_rendered(id: EntityId) {
    let now = frame();
    SLICES.with(|slices| {
        let mut slices = slices.borrow_mut();
        let entry = slices.entry(id).or_default();
        if entry.rendered != now {
            entry.previous_render = entry.rendered;
        }
        entry.rendered = now;
        entry.shown = now;
    });
}

/// A slice is part of this frame (rendered or replayed).
pub(super) fn slice_shown(id: EntityId) {
    let now = frame();
    SLICES.with(|slices| {
        slices.borrow_mut().entry(id).or_default().shown = now;
    });
}

/// Whether an image last used in `scope` at `used` can still be on
/// screen, as of the start of the current frame.
fn may_be_visible(scope: Option<EntityId>, used: u64) -> bool {
    let now = frame();
    match scope {
        // `QuillApp` itself renders every frame: the last one shows all.
        None => used + 1 >= now,
        Some(id) => SLICES.with(|slices| {
            slices
                .borrow()
                .get(&id)
                // A slice that wasn't shown last frame replays nothing.
                .is_some_and(|s| s.shown + 1 >= now && used >= s.rendered)
        }),
    }
}

/// Where (slice, or `QuillApp`) and in which frame something was painted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PaintStamp {
    scope: Option<EntityId>,
    frame: u64,
}

/// A stamp for content painted now.
pub(super) fn paint_stamp() -> PaintStamp {
    PaintStamp {
        scope: SCOPE.with(|scope| scope.borrow().as_ref().and_then(|scope| scope.get())),
        frame: frame(),
    }
}

/// Whether content painted at `stamp` can still be on screen (a cached
/// slice may replay it), as of the start of the current frame.
pub(super) fn may_still_show(stamp: PaintStamp) -> bool {
    may_be_visible(stamp.scope, stamp.frame)
}

/// Whether an image used now in `scope`, last used there at `previous`,
/// wasn't on screen just before (it scrolled in, or is new).
fn newly_shown(scope: Option<EntityId>, previous: Option<u64>) -> bool {
    let Some(previous) = previous else {
        return true;
    };
    match scope {
        None => previous + 1 < frame(),
        Some(id) => SLICES.with(|slices| {
            slices
                .borrow()
                .get(&id)
                .is_none_or(|s| previous < s.previous_render)
        }),
    }
}

fn note_activity() {
    ACTIVITY.with(|activity| activity.set(Some(Instant::now())));
}

/// When the idle trim is due: `IDLE_AFTER` past the last change of what
/// is on screen, unless the last idle trim already covered it.
pub(super) fn idle_trim_due() -> Option<Instant> {
    let activity = ACTIVITY.with(Cell::get)?;
    let trimmed = IDLE_TRIMMED.with(Cell::get);
    (trimmed != Some(activity)).then_some(activity + IDLE_AFTER)
}

/// The oldest frame a cached slice shown last frame may still replay.
fn oldest_replayable() -> u64 {
    let now = frame();
    SLICES.with(|slices| {
        slices
            .borrow()
            .values()
            .filter(|s| s.shown + 1 >= now)
            .map(|s| s.rendered)
            .min()
            .unwrap_or(now)
            // `QuillApp` re-renders each frame, so last frame's images
            // are the newest anything still shows.
            .min(now.saturating_sub(1))
    })
}

/// Hand images Quill made (and painted) to the atlas sweeper once their
/// cache lets go of them: each is dropped from the atlas when no frame can
/// show it and nothing else holds it.
pub(super) fn retire_all(images: impl IntoIterator<Item = Arc<RenderImage>>) {
    let now = frame();
    RETIRED.with(|retired| {
        retired
            .borrow_mut()
            .extend(images.into_iter().map(|image| (image, now)));
    });
}

/// Drop retired images no frame can show any more from the atlas. Call at
/// the start of a frame, before anything renders.
pub(super) fn sweep(window: &mut Window, cx: &mut App) {
    let safe_before = oldest_replayable();
    let gone: Vec<Arc<RenderImage>> = RETIRED.with(|retired| {
        let mut retired = retired.borrow_mut();
        let mut gone = Vec::new();
        retired.retain(|(image, at)| {
            // Still held elsewhere (a cache that took it back, the chat
            // list's animation layer): it may be painted again.
            if *at >= safe_before || Arc::strong_count(image) > 1 {
                return true;
            }
            gone.push(image.clone());
            false
        });
        gone
    });
    for image in gone {
        cx.drop_image(image, Some(&mut *window));
    }
}

/// Hand the allocator's free pages back to the system once a trim freed a
/// lot (decoded images are large, short-lived blocks; without this the
/// freed space stays resident as allocator free lists).
/// - macOS: `malloc_zone_pressure_relief` on every zone.
/// - Linux (glibc): `malloc_trim`.
/// - Windows and other libcs: nothing; the Windows heap decommits large
///   free ranges on its own.
fn release_free_heap() {
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            fn malloc_zone_pressure_relief(zone: *mut std::ffi::c_void, goal: usize) -> usize;
        }
        // SAFETY: a null zone means "all zones"; goal 0 means "as much as
        // possible". The call only releases free pages.
        unsafe {
            malloc_zone_pressure_relief(std::ptr::null_mut(), 0);
        }
    }
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        // SAFETY: glibc's `malloc_trim` only returns free heap memory.
        unsafe {
            libc::malloc_trim(0);
        }
    }
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
/// Inside the main window's `CacheScope` it goes through the bounded
/// cache; elsewhere (other windows) through GPUI's asset cache as before.
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
        match SCOPED.with(|scoped| scoped.borrow().last().cloned()) {
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
/// aspect differs from the picture's. GIF files stay on GPUI's loader
/// (it plays their frames); the media viewer never comes here.
pub(super) fn sized_media(
    path: &Path,
    frame: (Pixels, Pixels),
    dims: Option<(i32, i32)>,
    fit: Fit,
) -> ImageSource {
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gif"))
    {
        return ImageSource::Resource(Resource::Path(Arc::from(path)));
    }
    let logical = shorter_side_in_frame((frame.0 / px(1.), frame.1 / px(1.)), dims, fit);
    sized_image_with(SizedSource::Path(Arc::from(path)), move |scale_factor| {
        bucket_edge(display_edge(px(logical), scale_factor))
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
        .filter(|&edge| edge >= wanted && edge * REUSE_UP_TO.1 <= wanted * REUSE_UP_TO.0)
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

/// The decoded size for an image of `size` whose shorter side should be at
/// most `edge` (aspect kept; never enlarged).
fn scaled_size((width, height): (u32, u32), edge: u32) -> (u32, u32) {
    let short = width.min(height);
    if short <= edge || short == 0 {
        return (width, height);
    }
    let scale = f64::from(edge) / f64::from(short);
    let fit = |side: u32| ((f64::from(side) * scale).round() as u32).max(1);
    (fit(width), fit(height))
}

fn decode_sized(key: &SizedKey) -> Result<Arc<RenderImage>, ImageCacheError> {
    let read;
    let bytes: &[u8] = match &key.source {
        SizedSource::Path(path) => {
            read = std::fs::read(path).map_err(|err| ImageCacheError::Io(Arc::new(err)))?;
            &read
        }
        SizedSource::Mini(mini) => &mini.data,
    };
    let decoded =
        image::load_from_memory(bytes).map_err(|err| ImageCacheError::Image(Arc::new(err)))?;
    let filter = image::imageops::FilterType::Triangle;
    let mut image = if decoded.color().has_alpha() {
        let image = decoded.into_rgba8();
        let (width, height) = scaled_size(image.dimensions(), key.edge);
        if (width, height) == image.dimensions() {
            image
        } else {
            image::imageops::resize(&image, width, height, filter)
        }
    } else {
        // Photos: shrink the three-channel picture and widen only the
        // small result (an RGBA copy of a 2560 px photo is 20 MB, held
        // by every decode in flight).
        let image = decoded.into_rgb8();
        let (width, height) = scaled_size(image.dimensions(), key.edge);
        let image = if (width, height) == image.dimensions() {
            image
        } else {
            image::imageops::resize(&image, width, height, filter)
        };
        image::DynamicImage::ImageRgb8(image).into_rgba8()
    };
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

/// Side of the atlas textures GPUI allocates (larger images get a texture
/// of their own size).
const ATLAS_TEXTURE: u32 = 1024;

/// What a decoded image costs: its pixels, or, for a small one, the share
/// of an atlas texture it takes up. A texture holds `cols x rows` tiles of
/// that size at best, so a 768x576 image uses a whole 4 MB texture for
/// 1.7 MB of pixels (and the texture only goes once its last tile does).
fn atlas_cost(width: u32, height: u32) -> usize {
    let pixels = width as usize * height as usize * 4;
    if width == 0 || height == 0 || width > ATLAS_TEXTURE || height > ATLAS_TEXTURE {
        return pixels;
    }
    let tiles = (ATLAS_TEXTURE / width) as usize * (ATLAS_TEXTURE / height) as usize;
    let texture = ATLAS_TEXTURE as usize * ATLAS_TEXTURE as usize * 4;
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

    fn may_be_visible(&self) -> bool {
        self.uses
            .iter()
            .any(|(scope, used)| may_be_visible(*scope, *used))
    }
}

/// The main window's image cache: least recently used images that can't
/// be on screen go once the decoded total passes the budget.
pub(super) struct BoundedImageCache {
    entries: HashMap<u64, Entry>,
    /// The display-size decodes of each picture: source hash -> (edge,
    /// entry key). May list evicted entries; checked against `entries`.
    variants: HashMap<u64, SmallVec<[(u32, u64); 2]>>,
    bytes: usize,
    /// The slice being drawn (shared with `app_slice`).
    scope: Rc<Cell<Option<EntityId>>>,
}

impl BoundedImageCache {
    pub(super) fn new(scope: Rc<Cell<Option<EntityId>>>) -> Self {
        SCOPE.with(|current| *current.borrow_mut() = Some(scope.clone()));
        Self {
            entries: HashMap::new(),
            variants: HashMap::new(),
            bytes: 0,
            scope,
        }
    }

    /// Evict down to the scrolling budget. Call at the start of a frame,
    /// before anything renders, so last frame's loads are all counted.
    pub(super) fn trim(&mut self, window: &mut Window, cx: &mut App) {
        for image in self.evict(ACTIVE_BUDGET) {
            cx.drop_image(image, Some(&mut *window));
        }
    }

    /// Nothing new showed for `IDLE_AFTER`: evict down to the idle budget.
    /// Runs between frames (no window is being drawn), so the images leave
    /// every window's atlas.
    pub(super) fn idle_trim(&mut self, cx: &mut App) {
        IDLE_TRIMMED.with(|trimmed| trimmed.set(ACTIVITY.with(Cell::get)));
        let before = (self.bytes, self.entries.len());
        let images = self.evict(IDLE_BUDGET);
        let evicted = !images.is_empty();
        if super::frame_clock::trace_ticks() {
            eprintln!(
                "image idle trim: {} images, {:.1} MB -> {} images, {:.1} MB",
                before.1,
                before.0 as f64 / 1048576.0,
                self.entries.len(),
                self.bytes as f64 / 1048576.0
            );
        }
        for image in images {
            cx.drop_image(image, None);
        }
        if evicted {
            release_free_heap();
        }
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
            .filter(|(_, entry)| !entry.may_be_visible())
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
        images
    }

    fn load_entry(
        &mut self,
        key: u64,
        start: impl FnOnce(&mut App) -> Load,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<LoadResult> {
        let scope = self.scope.get();
        let now = frame();
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
        if previous != Some(now) && newly_shown(scope, previous) {
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
            // Forget the evicted ones as new ones are added.
            tracked.retain(|(_, h)| loaded.iter().any(|(_, l)| l == h));
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

/// Installs the bounded image cache over its child for every drawing
/// phase. GPUI's own `image_cache` element skips prepaint, where virtual
/// lists and cached views build their content.
pub(super) struct CacheScope {
    cache: Entity<BoundedImageCache>,
    child: AnyElement,
}

impl CacheScope {
    pub(super) fn new(cache: Entity<BoundedImageCache>, child: impl IntoElement) -> Self {
        Self {
            cache,
            child: child.into_any_element(),
        }
    }

    fn enter<R>(&self, window: &mut Window, f: impl FnOnce(&mut Window) -> R) -> R {
        SCOPED.with(|scoped| scoped.borrow_mut().push(self.cache.clone()));
        let result = window.with_image_cache(Some(self.cache.clone().into()), f);
        SCOPED.with(|scoped| scoped.borrow_mut().pop());
        result
    }
}

impl IntoElement for CacheScope {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for CacheScope {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut child = std::mem::replace(&mut self.child, Empty.into_any_element());
        let layout_id = self.enter(window, |window| child.request_layout(window, cx));
        self.child = child;
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let mut child = std::mem::replace(&mut self.child, Empty.into_any_element());
        self.enter(window, |window| child.prepaint(window, cx));
        self.child = child;
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let mut child = std::mem::replace(&mut self.child, Empty.into_any_element());
        self.enter(window, |window| child.paint(window, cx));
        self.child = child;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Budget, IDLE_AFTER, begin_frame, idle_trim_due, may_be_visible, newly_shown, note_activity,
        plan_eviction, scaled_size, slice_rendered, slice_shown,
    };
    use super::{Fit, VariantPick, atlas_cost, bucket_edge, pick_variant, shorter_side_in_frame};
    use gpui_kit::EntityId;

    #[test]
    fn app_images_stay_for_one_frame() {
        let used = begin_frame();
        assert!(may_be_visible(None, used));
        begin_frame();
        assert!(may_be_visible(None, used), "last frame may still show it");
        begin_frame();
        assert!(!may_be_visible(None, used));
    }

    #[test]
    fn slice_images_stay_until_the_slice_renders_again() {
        let slice = EntityId::from((1_u64 << 32) | 7);
        let used = begin_frame();
        slice_rendered(slice);
        // Replayed (shown, not rendered) for a while: still on screen.
        for _ in 0..5 {
            begin_frame();
            assert!(may_be_visible(Some(slice), used));
            slice_shown(slice);
        }
        // Rendered again without it: gone from the next frame on.
        slice_rendered(slice);
        begin_frame();
        assert!(!may_be_visible(Some(slice), used));
    }

    #[test]
    fn hidden_slices_show_nothing() {
        let slice = EntityId::from((1_u64 << 32) | 9);
        let used = begin_frame();
        slice_rendered(slice);
        begin_frame();
        begin_frame();
        assert!(!may_be_visible(Some(slice), used));
    }

    #[test]
    fn rerendering_the_same_images_is_not_activity() {
        let slice = EntityId::from((1_u64 << 32) | 11);
        let first = begin_frame();
        slice_rendered(slice);
        assert!(newly_shown(Some(slice), None), "a new image");
        // Replayed for a while, then rendered again showing it: unchanged.
        for _ in 0..10 {
            begin_frame();
            slice_shown(slice);
        }
        let second = begin_frame();
        slice_rendered(slice);
        assert!(!newly_shown(Some(slice), Some(first)));
        // An image last shown two renders ago scrolled back in.
        begin_frame();
        slice_rendered(slice);
        assert!(newly_shown(Some(slice), Some(first)));
        assert!(!newly_shown(Some(slice), Some(second)));
        // App-level images: shown last frame, or not.
        let now = begin_frame();
        assert!(!newly_shown(None, Some(now - 1)));
        assert!(newly_shown(None, Some(now - 2)));
    }

    #[test]
    fn idle_trim_waits_for_quiet_and_runs_once_per_change() {
        note_activity();
        let due = idle_trim_due().expect("activity schedules a trim");
        assert!(
            due >= std::time::Instant::now() + IDLE_AFTER - std::time::Duration::from_millis(50)
        );
        super::IDLE_TRIMMED.with(|trimmed| trimmed.set(super::ACTIVITY.with(|a| a.get())));
        assert_eq!(idle_trim_due(), None, "already trimmed");
        note_activity();
        assert!(idle_trim_due().is_some());
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
    fn sized_decode_keeps_aspect_and_never_enlarges() {
        assert_eq!(scaled_size((160, 160), 92), (92, 92));
        assert_eq!(scaled_size((160, 160), 200), (160, 160));
        assert_eq!(scaled_size((1280, 960), 36), (48, 36));
        assert_eq!(scaled_size((40, 30), 36), (40, 30));
        assert_eq!(scaled_size((0, 10), 5), (0, 10));
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
        // Avatars pack tightly: about their pixels.
        assert_eq!(atlas_cost(92, 92), texture / (11 * 11));
        assert!((36 * 36 * 4..36 * 36 * 4 * 11 / 10).contains(&atlas_cost(36, 36)));
        assert_eq!(atlas_cost(0, 5), 0);
    }
}
