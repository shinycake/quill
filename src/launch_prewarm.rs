//! macOS launch: the WindowServer connection and the GPU list are set up on
//! a worker while the main thread reads settings and builds the platform.
//! Both are XPC round trips the main thread otherwise waits on in
//! `App::new` (displays) and the first window (`MTLCopyAllDevices`), as is
//! the screen zoom setting AppKit reads (once per process) when the window
//! first sets its first responder.
//! Then it reads back the glyph bitmaps the last launch drew, so the first
//! frame does not rasterize the chat list's text and emoji again
//! ([`chat_list_drawn`] writes this launch's).

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
                gpui_macos::glyph_cache::load(&path, &glyph_cache_stamp());
            }
        });
}

/// `<app data>/launch-glyphs.bin`.
#[cfg(all(target_os = "macos", feature = "ui"))]
fn glyph_cache_path() -> Option<std::path::PathBuf> {
    crate::settings::safe_app_root().map(|root| root.join("launch-glyphs.bin"))
}

/// The OS build and this executable's modification time: either changing
/// may change the fonts or how glyphs are drawn.
#[cfg(all(target_os = "macos", feature = "ui"))]
fn glyph_cache_stamp() -> String {
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

static GLYPHS_SAVED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether this launch's glyphs are still to be written.
pub fn glyphs_pending() -> bool {
    cfg!(all(target_os = "macos", feature = "ui"))
        && !GLYPHS_SAVED.load(std::sync::atomic::Ordering::Relaxed)
}

/// The chat list has been drawn once: the glyph bitmaps drawn so far are
/// written for the next launch, on a worker. Only the first call does
/// anything.
#[cfg(all(target_os = "macos", feature = "ui"))]
pub fn chat_list_drawn() {
    if GLYPHS_SAVED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("quill-glyph-cache".into())
        .spawn(|| {
            if let Some(path) = glyph_cache_path() {
                let _ = gpui_macos::glyph_cache::save(&path, &glyph_cache_stamp());
            }
        });
}

#[cfg(not(all(target_os = "macos", feature = "ui")))]
pub fn chat_list_drawn() {}

#[cfg(not(target_os = "macos"))]
pub fn start() {}
