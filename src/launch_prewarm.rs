//! macOS launch: the WindowServer connection and the GPU list are set up on
//! a worker while the main thread reads settings and builds the platform.
//! Both are XPC round trips the main thread otherwise waits on in
//! `App::new` (displays) and the first window (`MTLCopyAllDevices`).

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
            let mut count = 0u32;
            #[allow(unsafe_code, reason = "plain CoreGraphics and Metal calls")]
            // SAFETY: a zero-length query writes only `count`; the device
            // array is a +1 CFArray (or null) released here.
            unsafe {
                CGGetActiveDisplayList(0, std::ptr::null_mut(), &mut count);
                let devices = MTLCopyAllDevices();
                if !devices.is_null() {
                    CFRelease(devices);
                }
            }
        });
}

#[cfg(not(target_os = "macos"))]
pub fn start() {}
