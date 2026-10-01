//! Bounded sticker decoding; GPUI owns the display clock.
use std::ffi::{CString, c_char, c_void};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{
    Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};

pub const STICKER_EDGE: usize = 128;
pub const MAX_STICKER_FRAMES: usize = 120;

pub struct StickerFrames {
    /// Native BGRA, matching GPUI's RenderImage format.
    pub frames: Vec<Vec<u8>>,
    pub fps: f64,
}

fn tgs_json(path: &Path) -> Result<CString, String> {
    let file = std::fs::File::open(path).map_err(|_| "Could not read animated sticker")?;
    if file
        .metadata()
        .map_err(|_| "Could not read animated sticker")?
        .len()
        > 1_048_576
    {
        return Err("Animated sticker is too large".into());
    }
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(file)
        .take(8_388_609)
        .read_to_end(&mut bytes)
        .map_err(|_| "Invalid animated sticker compression")?;
    if bytes.len() > 8_388_608 {
        return Err("Animated sticker is too large".into());
    }
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| "Invalid animated sticker")?;
    for dimension in ["w", "h"] {
        if !value
            .get(dimension)
            .and_then(|v| v.as_u64())
            .is_some_and(|n| (1..=4096).contains(&n))
        {
            return Err("Invalid animated sticker size".into());
        }
    }
    // TGS is vector-only. Never let native decoding read external image resources.
    if value
        .get("assets")
        .and_then(|v| v.as_array())
        .is_some_and(|assets| {
            assets
                .iter()
                .any(|asset| asset.get("p").is_some() || asset.get("u").is_some())
        })
    {
        return Err("External images are not allowed in animated stickers".into());
    }
    let fps = value.get("fr").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let start = value.get("ip").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let end = value.get("op").and_then(|v| v.as_f64()).unwrap_or(0.0);
    if !(1.0..=120.0).contains(&fps)
        || !(0.0..=3600.0).contains(&start)
        || end <= start
        || end > 7200.0
        || (end - start) / fps > 30.0
    {
        return Err("Invalid animated sticker duration".into());
    }
    CString::new(bytes).map_err(|_| "Invalid animated sticker".into())
}

struct Rlottie {
    _library: libloading::Library,
    from_data: unsafe extern "C" fn(*const c_char, *const c_char, *const c_char) -> *mut c_void,
    destroy: unsafe extern "C" fn(*mut c_void),
    total_frames: unsafe extern "C" fn(*const c_void) -> usize,
    fps: unsafe extern "C" fn(*const c_void) -> f64,
    render: unsafe extern "C" fn(*mut c_void, usize, *mut u32, usize, usize, usize),
}

impl Rlottie {
    fn load() -> Result<Self, String> {
        let name = if cfg!(target_os = "macos") {
            "librlottie.dylib"
        } else if cfg!(target_os = "windows") {
            "rlottie.dll"
        } else {
            "librlottie.so"
        };
        let mut candidates: Vec<PathBuf> = std::env::var_os("QUILL_RLOTTIE_PATH")
            .map(PathBuf::from)
            .into_iter()
            .collect();
        if let Some(dir) = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
        {
            candidates.push(dir.join(name));
            candidates.push(dir.join("../Frameworks").join(name));
        }
        for path in candidates {
            // SAFETY: only explicit owner-configured or bundled native libraries are loaded.
            let Ok(library) = (unsafe { libloading::Library::new(path) }) else {
                continue;
            };
            // SAFETY: signatures match the pinned rlottie C API. The library remains alive
            // for the process, so initialization and function pointers never outlive it.
            unsafe {
                let init = *library
                    .get::<unsafe extern "C" fn()>(b"lottie_init\0")
                    .map_err(|_| "Incompatible rlottie library")?;
                let cache = *library
                    .get::<unsafe extern "C" fn(usize)>(b"lottie_configure_model_cache_size\0")
                    .map_err(|_| "Incompatible rlottie library")?;
                let renderer = Self {
                    from_data: *library
                        .get(b"lottie_animation_from_data\0")
                        .map_err(|_| "Incompatible rlottie library")?,
                    destroy: *library
                        .get(b"lottie_animation_destroy\0")
                        .map_err(|_| "Incompatible rlottie library")?,
                    total_frames: *library
                        .get(b"lottie_animation_get_totalframe\0")
                        .map_err(|_| "Incompatible rlottie library")?,
                    fps: *library
                        .get(b"lottie_animation_get_framerate\0")
                        .map_err(|_| "Incompatible rlottie library")?,
                    render: *library
                        .get(b"lottie_animation_render\0")
                        .map_err(|_| "Incompatible rlottie library")?,
                    _library: library,
                };
                init();
                cache(0);
                return Ok(renderer);
            }
        }
        Err("Animated stickers need the bundled rlottie renderer".into())
    }
}

pub fn decode_tgs(path: &Path, cancelled: &AtomicBool) -> Result<StickerFrames, String> {
    let data = tgs_json(path)?;
    // ponytail: serialize native renders behind one lock; separate renderer processes if throughput matters.
    static RENDERER: OnceLock<Mutex<Option<Rlottie>>> = OnceLock::new();
    let mut slot = RENDERER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "Animated sticker renderer failed")?;
    if cancelled.load(Ordering::SeqCst) {
        return Err("Sticker playback cancelled".into());
    }
    if slot.is_none() {
        *slot = Some(Rlottie::load()?);
    }
    let renderer = slot.as_ref().expect("loaded");
    // SAFETY: the validated, NUL-terminated JSON and empty resource strings stay alive;
    // this animation is used only while the library's global lock is held.
    let animation = unsafe { (renderer.from_data)(data.as_ptr(), c"".as_ptr(), c"".as_ptr()) };
    if animation.is_null() {
        return Err("Could not render animated sticker".into());
    }
    struct Animation<'a>(*mut c_void, &'a Rlottie);
    impl Drop for Animation<'_> {
        fn drop(&mut self) {
            // SAFETY: this pointer belongs to one live animation and is destroyed once.
            unsafe { (self.1.destroy)(self.0) };
        }
    }
    let animation = Animation(animation, renderer);
    // SAFETY: animation and its owning library are alive and exclusively held.
    let (total, original_fps) = unsafe {
        (
            (renderer.total_frames)(animation.0),
            (renderer.fps)(animation.0),
        )
    };
    if total == 0
        || total > 7200
        || !(1.0..=120.0).contains(&original_fps)
        || total as f64 / original_fps > 30.0
    {
        return Err("Invalid animated sticker duration".into());
    }
    let count = total.min(MAX_STICKER_FRAMES);
    let mut frames = Vec::with_capacity(count);
    for index in 0..count {
        if cancelled.load(Ordering::SeqCst) {
            return Err("Sticker playback cancelled".into());
        }
        let mut pixels = vec![0u32; STICKER_EDGE * STICKER_EDGE];
        // SAFETY: renderer writes exactly the 128x128 surface with its correct byte stride.
        unsafe {
            (renderer.render)(
                animation.0,
                index * total / count,
                pixels.as_mut_ptr(),
                STICKER_EDGE,
                STICKER_EDGE,
                STICKER_EDGE * 4,
            )
        };
        frames.push(pixels.into_iter().flat_map(u32::to_ne_bytes).collect());
    }
    Ok(StickerFrames {
        frames,
        fps: original_fps * count as f64 / total as f64,
    })
}

pub fn decode_webm(
    path: &Path,
    cache: &Path,
    child: &std::sync::Arc<Mutex<Option<std::process::Child>>>,
    cancelled: &AtomicBool,
) -> Result<crate::video::ViewerFrames, String> {
    let probe =
        crate::video::probe_with_ffprobe(path, false).ok_or("Could not read video sticker")?;
    if probe.duration > 30 {
        return Err("Video sticker is too long".into());
    }
    let fps = (MAX_STICKER_FRAMES as f64 / f64::from(probe.duration.max(1) + 1)).min(24.0);
    // VP9's native ffmpeg decoder drops alpha; libvpx preserves the sticker surface.
    let frames = crate::video::extract_frames(
        path,
        cache,
        0,
        fps,
        STICKER_EDGE as i32,
        MAX_STICKER_FRAMES as i32,
        Some((child, cancelled)),
        Some("libvpx-vp9"),
    )?;
    if frames.is_empty() {
        return Err("Video sticker has no frames".into());
    }
    Ok(crate::video::ViewerFrames { frames, fps })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    #[ignore = "requires the pinned rlottie runtime and ffmpeg; run scripts/sticker-fixtures.py first"]
    fn native_stickers_animate_with_transparent_backgrounds() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/screenshots/fixtures");
        let cancelled = AtomicBool::new(false);
        let tgs = decode_tgs(&root.join("demo-sticker.tgs"), &cancelled).unwrap();
        assert!((60..=61).contains(&tgs.frames.len()));
        assert_eq!(tgs.fps, 30.0);
        assert_ne!(tgs.frames[0], tgs.frames[15]);
        assert_eq!(tgs.frames[0][3], 0);
        assert!(tgs.frames[0].as_chunks::<4>().0.iter().any(|p| p[3] == 255));
        cancelled.store(true, Ordering::SeqCst);
        assert!(decode_tgs(&root.join("demo-sticker.tgs"), &cancelled).is_err());
        cancelled.store(false, Ordering::SeqCst);
        let cache = std::env::temp_dir().join(format!("quill-webm-sticker-{}", std::process::id()));
        let child = std::sync::Arc::new(Mutex::new(None));
        let webm =
            decode_webm(&root.join("demo-sticker.webm"), &cache, &child, &cancelled).unwrap();
        assert!(webm.frames.len() > 1 && webm.frames.len() <= MAX_STICKER_FRAMES);
        assert_ne!(
            std::fs::read(&webm.frames[0]).unwrap(),
            std::fs::read(&webm.frames[10]).unwrap()
        );
        assert!(child.lock().unwrap().is_none());
        std::fs::remove_dir_all(cache).unwrap();
        let mut legacy = serde_json::to_value(crate::settings::MediaPrefs::default()).unwrap();
        legacy
            .as_object_mut()
            .unwrap()
            .remove("loop_animated_stickers");
        assert!(
            serde_json::from_value::<crate::settings::MediaPrefs>(legacy)
                .unwrap()
                .loop_animated_stickers
        );
    }
    fn fixture(value: &serde_json::Value) -> PathBuf {
        let path = std::env::temp_dir().join(format!("quill-sticker-{}.tgs", std::process::id()));
        let mut gzip = flate2::write::GzEncoder::new(
            std::fs::File::create(&path).unwrap(),
            flate2::Compression::default(),
        );
        gzip.write_all(value.to_string().as_bytes()).unwrap();
        gzip.finish().unwrap();
        path
    }

    #[test]
    fn tgs_validation_rejects_external_resources_and_bad_bounds() {
        let base =
            serde_json::json!({"w":512,"h":512,"fr":60,"ip":0,"op":120,"assets":[],"layers":[]});
        let path = fixture(&base);
        assert!(tgs_json(&path).is_ok());
        for patch in [
            serde_json::json!({"assets":[{"p":"/etc/passwd"}]}),
            serde_json::json!({"w":0}),
            serde_json::json!({"fr":0}),
            serde_json::json!({"op":7201}),
        ] {
            let mut value = base.clone();
            for (key, replacement) in patch.as_object().unwrap() {
                value[key] = replacement.clone();
            }
            assert!(tgs_json(&fixture(&value)).is_err());
        }
        std::fs::write(&path, b"bad gzip").unwrap();
        assert!(tgs_json(&path).is_err());
        let _ = std::fs::remove_file(path);
    }
}
