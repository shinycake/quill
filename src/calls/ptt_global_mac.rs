//! macOS backend: a listen-only `CGEventTap` on its own thread.
//!
//! Input Monitoring (macOS 10.15+) is checked with
//! `CGPreflightListenEventAccess`, which never prompts. The tap is only
//! created after that passes, so Quill never triggers the system prompt by
//! itself; the guidance in Settings sends the user to the right pane instead,
//! as tdesktop does (`lng_group_call_mac_input`).

use super::{KeyFilter, Sink, StartError, mac_keycode};
use std::cell::Cell;
use std::ffi::c_void;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Duration;

type CFTypeRef = *const c_void;

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFRunLoopCommonModes: CFTypeRef;
    static kCFRunLoopDefaultMode: CFTypeRef;
    fn CFRunLoopGetCurrent() -> CFTypeRef;
    fn CFRunLoopAddSource(rl: CFTypeRef, source: CFTypeRef, mode: CFTypeRef);
    fn CFRunLoopRemoveSource(rl: CFTypeRef, source: CFTypeRef, mode: CFTypeRef);
    fn CFRunLoopRunInMode(mode: CFTypeRef, seconds: f64, return_after_source: u8) -> i32;
    fn CFRunLoopStop(rl: CFTypeRef);
    fn CFMachPortCreateRunLoopSource(
        allocator: CFTypeRef,
        port: CFTypeRef,
        order: isize,
    ) -> CFTypeRef;
    fn CFMachPortInvalidate(port: CFTypeRef);
    fn CFRelease(cf: CFTypeRef);
}

type TapCallback = extern "C" fn(*mut c_void, u32, *mut c_void, *mut c_void) -> *mut c_void;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: TapCallback,
        user_info: *mut c_void,
    ) -> CFTypeRef;
    fn CGEventTapEnable(tap: CFTypeRef, enable: bool);
    fn CGEventGetIntegerValueField(event: *mut c_void, field: u32) -> i64;
    fn CGPreflightListenEventAccess() -> bool;
}

const SESSION_EVENT_TAP: u32 = 1;
const HEAD_INSERT: u32 = 0;
const LISTEN_ONLY: u32 = 1;
const KEY_DOWN: u32 = 10;
const KEY_UP: u32 = 11;
const TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
const TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;
const KEYBOARD_EVENT_KEYCODE: u32 = 9;

/// A running tap; dropping it stops the thread.
#[derive(Debug)]
pub struct Handle {
    active: Arc<AtomicBool>,
    run_loop: Arc<AtomicUsize>,
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.active.store(false, Ordering::SeqCst);
        let rl = self.run_loop.load(Ordering::SeqCst);
        if rl != 0 {
            // Wakes the thread; it also re-checks the flag on a short timeout.
            unsafe { CFRunLoopStop(rl as CFTypeRef) };
        }
    }
}

struct TapContext {
    filter: KeyFilter,
    sink: Sink,
    active: Arc<AtomicBool>,
    tap: Cell<CFTypeRef>,
}

extern "C" fn on_event(
    _proxy: *mut c_void,
    event_type: u32,
    event: *mut c_void,
    info: *mut c_void,
) -> *mut c_void {
    // SAFETY: `info` is the `TapContext` boxed by `run_tap`, alive until the
    // run loop returns; the callback only runs on that thread.
    let ctx = unsafe { &mut *(info as *mut TapContext) };
    match event_type {
        TAP_DISABLED_BY_TIMEOUT | TAP_DISABLED_BY_USER_INPUT => unsafe {
            CGEventTapEnable(ctx.tap.get(), true);
        },
        KEY_DOWN | KEY_UP if ctx.active.load(Ordering::SeqCst) => {
            let code = unsafe { CGEventGetIntegerValueField(event, KEYBOARD_EVENT_KEYCODE) };
            if let Some(edge) = u32::try_from(code)
                .ok()
                .and_then(|code| ctx.filter.feed(code, event_type == KEY_DOWN))
            {
                (ctx.sink)(edge);
            }
        }
        _ => {}
    }
    event
}

fn run_tap(
    target: u32,
    sink: Sink,
    active: Arc<AtomicBool>,
    run_loop: Arc<AtomicUsize>,
    ready: mpsc::Sender<Result<(), StartError>>,
) {
    let ctx = Box::into_raw(Box::new(TapContext {
        filter: KeyFilter::new(target),
        sink,
        active: active.clone(),
        tap: Cell::new(std::ptr::null()),
    }));
    let mask = (1u64 << KEY_DOWN) | (1u64 << KEY_UP);
    let tap = unsafe {
        CGEventTapCreate(
            SESSION_EVENT_TAP,
            HEAD_INSERT,
            LISTEN_ONLY,
            mask,
            on_event,
            ctx.cast(),
        )
    };
    if tap.is_null() {
        drop(unsafe { Box::from_raw(ctx) });
        let _ = ready.send(Err(StartError::PermissionDenied));
        return;
    }
    unsafe {
        (*ctx).tap.set(tap);
        let source = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
        let rl = CFRunLoopGetCurrent();
        CFRunLoopAddSource(rl, source, kCFRunLoopCommonModes);
        CGEventTapEnable(tap, true);
        run_loop.store(rl as usize, Ordering::SeqCst);
        let _ = ready.send(Ok(()));
        while active.load(Ordering::SeqCst) {
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.5, 0);
        }
        CGEventTapEnable(tap, false);
        CFRunLoopRemoveSource(rl, source, kCFRunLoopCommonModes);
        CFMachPortInvalidate(tap);
        CFRelease(source);
        CFRelease(tap);
        drop(Box::from_raw(ctx));
    }
}

pub fn start(key: &str, sink: Sink) -> Result<Handle, StartError> {
    let Some(target) = mac_keycode(key) else {
        return Err(StartError::Unsupported(
            "This key can't be used system-wide.".into(),
        ));
    };
    // Never prompt: without Input Monitoring the caller shows guidance.
    if !permission_granted() {
        return Err(StartError::PermissionDenied);
    }
    let active = Arc::new(AtomicBool::new(true));
    let run_loop = Arc::new(AtomicUsize::new(0));
    let (ready_tx, ready_rx) = mpsc::channel();
    {
        let (active, run_loop) = (active.clone(), run_loop.clone());
        std::thread::Builder::new()
            .name("quill-ptt-tap".into())
            .spawn(move || run_tap(target, sink, active, run_loop, ready_tx))
            .map_err(|_| StartError::Failed("Couldn't start the key listener.".into()))?;
    }
    match ready_rx.recv_timeout(Duration::from_secs(2)) {
        Ok(Ok(())) => Ok(Handle { active, run_loop }),
        Ok(Err(err)) => Err(err),
        Err(_) => {
            active.store(false, Ordering::SeqCst);
            Err(StartError::Failed("The key listener did not start.".into()))
        }
    }
}

pub fn static_limit() -> Option<String> {
    None
}

pub fn permission_granted() -> bool {
    unsafe { CGPreflightListenEventAccess() }
}

pub fn open_permission_settings() {
    let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent")
        .spawn();
}

#[cfg(test)]
mod tests {
    use super::{StartError, permission_granted, start};
    use std::sync::Arc;

    #[test]
    fn start_never_prompts_and_matches_the_permission() {
        let result = start("space", Arc::new(|_| {}));
        if permission_granted() {
            assert!(result.is_ok());
        } else {
            assert!(matches!(result, Err(StartError::PermissionDenied)));
        }
        assert!(matches!(
            start("nonsense", Arc::new(|_| {})),
            Err(StartError::Unsupported(_))
        ));
    }
}
