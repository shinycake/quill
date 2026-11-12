//! macOS launch: the WindowServer connection and the GPU list are set up on
//! a worker while the main thread reads settings and builds the platform.
//! Both are XPC round trips the main thread otherwise waits on in
//! `App::new` (displays) and the first window (`MTLCopyAllDevices`), as is
//! the screen zoom setting AppKit reads (once per process) when the window
//! first sets its first responder.
//! Then it reads back the glyph bitmaps the last launch drew, so the first
//! frame does not rasterize the chat list's text and emoji again
//! ([`chat_list_drawn`] writes this launch's, and the line layouts
//! `crate::launch_lines` keeps).

#[cfg(target_os = "macos")]
pub fn start() {
    let _ = std::thread::Builder::new()
        .name("quill-launch-prewarm".into())
        .spawn(|| {
            #[link(name = "CoreGraphics", kind = "framework")]
            unsafe extern "C" {
                fn CGGetActiveDisplayList(max: u32, displays: *mut u32, count: *mut u32) -> i32;
            }
            #[link(name = "Metal", kind = "framework")]
            unsafe extern "C" {
                fn MTLCopyAllDevices() -> *mut std::ffi::c_void;
            }
            #[link(name = "CoreFoundation", kind = "framework")]
            unsafe extern "C" {
                fn CFRelease(object: *const std::ffi::c_void);
            }
            #[link(name = "ApplicationServices", kind = "framework")]
            unsafe extern "C" {
                fn UAZoomEnabled() -> u8;
            }
            let mut count = 0u32;
            #[allow(
                unsafe_code,
                reason = "plain CoreGraphics, Metal and Universal Access calls"
            )]
            // SAFETY: a zero-length query writes only `count`; the device
            // array is a +1 CFArray (or null) released here; UAZoomEnabled
            // takes nothing and returns a Boolean.
            unsafe {
                CGGetActiveDisplayList(0, std::ptr::null_mut(), &mut count);
                let devices = MTLCopyAllDevices();
                if !devices.is_null() {
                    CFRelease(devices);
                }
                UAZoomEnabled();
            }
            #[cfg(feature = "ui")]
            if let Some(path) = glyph_cache_path() {
                gpui_macos::glyph_cache::load(&path, &cache_stamp());
            }
        });
}

/// `<app data>/launch-glyphs.bin`.
#[cfg(all(target_os = "macos", feature = "ui"))]
fn glyph_cache_path() -> Option<std::path::PathBuf> {
    crate::settings::safe_app_root().map(|root| root.join("launch-glyphs.bin"))
}

/// The OS build and this executable's modification time: either changing
/// may change the fonts or how lines are shaped and glyphs drawn.
#[cfg(all(target_os = "macos", feature = "ui"))]
pub(crate) fn cache_stamp() -> String {
    let os = sysctl_string(c"kern.osversion").unwrap_or_default();
    let built = std::env::current_exe()
        .and_then(std::fs::metadata)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_secs());
    format!("{os} {built}")
}

#[cfg(all(target_os = "macos", feature = "ui"))]
fn sysctl_string(name: &std::ffi::CStr) -> Option<String> {
    let mut buffer = [0u8; 64];
    let mut len = buffer.len();
    #[allow(unsafe_code, reason = "sysctlbyname into a stack buffer")]
    // SAFETY: `buffer` is writable for `len` bytes and sysctlbyname writes at
    // most that many, storing the written length back in `len`.
    let status = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            buffer.as_mut_ptr().cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 {
        return None;
    }
    let bytes = buffer.get(..len)?;
    let text = bytes.split(|byte| *byte == 0).next()?;
    String::from_utf8(text.to_vec()).ok()
}

/// Where writing this launch's glyphs and line layouts stands.
#[cfg_attr(
    not(all(target_os = "macos", feature = "ui")),
    allow(dead_code, reason = "only the macOS launch writes its caches")
)]
enum GlyphSave {
    /// Not written yet: the first drawn chat list starts it. Also where a
    /// writer that could not be started leaves it, for the next frame to
    /// try again.
    Unwritten,
    /// The writer thread runs; this is its handle. The thread's [`SaveEnd`]
    /// replaces the stage when it ends, however it ends.
    Saving {
        _writer: std::thread::JoinHandle<Option<()>>,
    },
    /// It ran (or panicked, which is not tried again).
    Saved,
}

static GLYPH_SAVE: std::sync::Mutex<GlyphSave> = std::sync::Mutex::new(GlyphSave::Unwritten);

/// Whether this launch's glyphs are still to be written.
pub fn glyphs_pending() -> bool {
    cfg!(all(target_os = "macos", feature = "ui"))
        && matches!(
            *crate::locks::lock_or_recover(&GLYPH_SAVE),
            GlyphSave::Unwritten
        )
}

/// Ends the write, however the writer thread ends.
#[cfg(all(target_os = "macos", feature = "ui"))]
struct SaveEnd;

#[cfg(all(target_os = "macos", feature = "ui"))]
impl Drop for SaveEnd {
    fn drop(&mut self) {
        *crate::locks::lock_or_recover(&GLYPH_SAVE) = GlyphSave::Saved;
    }
}

/// The chat list has been drawn once: the line layouts and glyph bitmaps
/// made so far are written for the next launch, on a worker. Only the first
/// call does anything, unless the worker could not be started.
#[cfg(all(target_os = "macos", feature = "ui"))]
pub fn chat_list_drawn() {
    // The lock is held across the spawn, so a writer that ends at once waits
    // in `SaveEnd` until its handle is in the stage.
    let mut save = crate::locks::lock_or_recover(&GLYPH_SAVE);
    if !matches!(*save, GlyphSave::Unwritten) {
        return;
    }
    let started = crate::thread_guard::spawn_guarded("quill-glyph-cache", || {
        let _end = SaveEnd;
        crate::launch_lines::save();
        if let Some(path) = glyph_cache_path() {
            let _ = gpui_macos::glyph_cache::save(&path, &cache_stamp());
        }
    });
    if let Ok(writer) = started {
        *save = GlyphSave::Saving { _writer: writer };
    }
}

#[cfg(not(all(target_os = "macos", feature = "ui")))]
pub fn chat_list_drawn() {}

#[cfg(not(target_os = "macos"))]
pub fn start() {}
