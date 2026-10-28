//! The vendored ThorVG (`third_party/thorvg`, compiled by build.rs): the
//! Lottie renderer behind animated stickers.
//!
//! Threading: ThorVG is built without `THORVG_THREAD_SUPPORT`, so its task
//! scheduler runs every task inline on the calling thread. The process-wide
//! state a canvas touches (renderer count, memory pools, engine init) sits
//! behind ThorVG's `StrictKey` mutexes; build.rs fails the build if that key
//! would compile to a no-op. Data is loaded as a copy, which keeps it out of
//! ThorVG's shared loader cache. So each decode owns its canvas and animation
//! and may run on any thread, concurrently with others.
#![deny(clippy::indexing_slicing)]

use std::ffi::{c_char, c_int, c_void};
use std::sync::OnceLock;

/// `Tvg_Result`.
const SUCCESS: c_int = 0;
const INSUFFICIENT_CONDITION: c_int = 2;
/// `TVG_ENGINE_OPTION_DEFAULT`.
const ENGINE_OPTION_DEFAULT: c_int = 1;
/// `TVG_COLORSPACE_ARGB8888S`: straight alpha, each pixel a `u32`
/// `0xAARRGGBB`, so BGRA bytes in little-endian order.
const COLORSPACE_ARGB8888S: c_int = 3;

// The subset of thorvg_capi.h Quill calls. Handles are opaque pointers; the
// enums are C `int`s.
unsafe extern "C" {
    fn tvg_engine_init(threads: u32) -> c_int;
    fn tvg_swcanvas_create(op: c_int) -> *mut c_void;
    fn tvg_swcanvas_set_target(
        canvas: *mut c_void,
        buffer: *mut u32,
        stride: u32,
        w: u32,
        h: u32,
        cs: c_int,
    ) -> c_int;
    fn tvg_canvas_destroy(canvas: *mut c_void) -> c_int;
    fn tvg_canvas_add(canvas: *mut c_void, paint: *mut c_void) -> c_int;
    fn tvg_canvas_update(canvas: *mut c_void) -> c_int;
    fn tvg_canvas_draw(canvas: *mut c_void, clear: bool) -> c_int;
    fn tvg_canvas_sync(canvas: *mut c_void) -> c_int;
    fn tvg_animation_new() -> *mut c_void;
    fn tvg_animation_del(animation: *mut c_void) -> c_int;
    fn tvg_animation_get_picture(animation: *mut c_void) -> *mut c_void;
    fn tvg_animation_set_frame(animation: *mut c_void, no: f32) -> c_int;
    fn tvg_animation_get_total_frame(animation: *mut c_void, cnt: *mut f32) -> c_int;
    fn tvg_picture_load_data(
        picture: *mut c_void,
        data: *const c_char,
        size: u32,
        mimetype: *const c_char,
        rpath: *const c_char,
        copy: bool,
    ) -> c_int;
    fn tvg_picture_set_size(picture: *mut c_void, w: f32, h: f32) -> c_int;
}

/// Starts the engine once per process. It is never terminated: termination
/// requires that no canvas is alive anywhere, and the process exit frees it.
fn engine() -> Result<(), String> {
    static STARTED: OnceLock<bool> = OnceLock::new();
    // SAFETY: tvg_engine_init takes no pointers; ThorVG guards it with its
    // own init lock and counts calls, and OnceLock makes this the only one.
    let started = *STARTED.get_or_init(|| unsafe { tvg_engine_init(0) } == SUCCESS);
    if started {
        Ok(())
    } else {
        Err("Could not start the animated sticker renderer".into())
    }
}

/// One Lottie animation on its own canvas, drawn at `edge` × `edge` px.
pub(super) struct Lottie {
    canvas: *mut c_void,
    animation: *mut c_void,
    /// The canvas's target. Its heap buffer never reallocates, so the
    /// pointer the canvas holds stays valid until `drop` destroys the canvas.
    pixels: Vec<u32>,
    total_frames: f32,
}

impl Lottie {
    /// Loads Lottie `json` (already validated by `tgs_json`).
    pub(super) fn load(json: &[u8], edge: usize) -> Result<Self, String> {
        engine()?;
        let invalid = || String::from("Could not render animated sticker");
        let size = u32::try_from(json.len()).map_err(|_| invalid())?;
        let side = u32::try_from(edge)
            .ok()
            .filter(|side| *side > 0)
            .ok_or_else(invalid)?;
        let area = edge.checked_mul(edge).ok_or_else(invalid)?;
        // SAFETY: both constructors take no pointers and return null on
        // failure; `Drop` accepts null handles (ThorVG rejects them).
        let mut lottie = unsafe {
            Self {
                canvas: tvg_swcanvas_create(ENGINE_OPTION_DEFAULT),
                animation: tvg_animation_new(),
                pixels: vec![0; area],
                total_frames: 0.0,
            }
        };
        if lottie.canvas.is_null() || lottie.animation.is_null() {
            return Err(invalid());
        }
        let mut total = 0.0f32;
        // SAFETY: the animation is live and owns the picture it returns. The
        // data pointer and length describe `json`, which ThorVG copies
        // (`copy` = true) before returning; the MIME type is a NUL-terminated
        // literal and a null rpath is allowed. The target is `pixels`, exactly
        // `side` × `side` u32s with a stride of `side`, owned by `lottie` and
        // outliving the canvas. `total` is a valid f32 out-pointer.
        let ok = unsafe {
            let picture = tvg_animation_get_picture(lottie.animation);
            !picture.is_null()
                && tvg_picture_load_data(
                    picture,
                    json.as_ptr().cast(),
                    size,
                    c"lottie+json".as_ptr(),
                    std::ptr::null(),
                    true,
                ) == SUCCESS
                && tvg_picture_set_size(picture, side as f32, side as f32) == SUCCESS
                && tvg_swcanvas_set_target(
                    lottie.canvas,
                    lottie.pixels.as_mut_ptr(),
                    side,
                    side,
                    side,
                    COLORSPACE_ARGB8888S,
                ) == SUCCESS
                && tvg_canvas_add(lottie.canvas, picture) == SUCCESS
                && tvg_animation_get_total_frame(lottie.animation, &mut total) == SUCCESS
        };
        if !ok || !total.is_finite() {
            return Err(invalid());
        }
        lottie.total_frames = total;
        Ok(lottie)
    }

    /// Frames in the animation (`op - ip`); frame numbers run from 0.
    pub(super) fn total_frames(&self) -> f32 {
        self.total_frames
    }

    /// Draws frame `frame` (fractions allowed) and returns its pixels as
    /// straight-alpha BGRA bytes, GPUI's RenderImage layout.
    pub(super) fn render(&mut self, frame: f32) -> Result<Vec<u8>, String> {
        // SAFETY: canvas and animation are live and used by this thread only
        // (`&mut self`); the target buffer is `self.pixels`, still allocated.
        // Setting the frame it already shows reports INSUFFICIENT_CONDITION,
        // which leaves the picture as it is.
        let drawn = unsafe {
            let set = tvg_animation_set_frame(self.animation, frame);
            (set == SUCCESS || set == INSUFFICIENT_CONDITION)
                && tvg_canvas_update(self.canvas) == SUCCESS
                && tvg_canvas_draw(self.canvas, true) == SUCCESS
                && tvg_canvas_sync(self.canvas) == SUCCESS
        };
        if !drawn {
            return Err("Could not render animated sticker".into());
        }
        // `0xAARRGGBB` as little-endian bytes is B, G, R, A on every host.
        let mut out = vec![0u8; self.pixels.len() * 4];
        for (bytes, pixel) in out.as_chunks_mut::<4>().0.iter_mut().zip(&self.pixels) {
            *bytes = pixel.to_le_bytes();
        }
        Ok(out)
    }
}

impl Drop for Lottie {
    fn drop(&mut self) {
        // SAFETY: each handle came from its constructor (or is null, which
        // ThorVG rejects without touching it) and is released once, here.
        // The canvas goes first, while its target buffer is still allocated;
        // the picture is reference counted by both, so either order frees it.
        unsafe {
            tvg_canvas_destroy(self.canvas);
            tvg_animation_del(self.animation);
        }
    }
}
