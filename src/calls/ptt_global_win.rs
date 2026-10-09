//! Windows backend: a low-level keyboard hook (`WH_KEYBOARD_LL`) on its own
//! thread with a message loop. The hook only observes: it always calls the
//! next hook, so the key still reaches the focused application.

use super::{KeyFilter, Sink, StartError, windows_vk};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, KBDLLHOOKSTRUCT, MSG, PM_NOREMOVE, PeekMessageW,
    PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_KEYDOWN,
    WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

struct Context {
    filter: KeyFilter,
    sink: Sink,
    active: Arc<AtomicBool>,
}

/// The hook procedure has no user data pointer, so the context is global.
/// Only one hook is live at a time (the controller drops the old one first).
static CONTEXT: Mutex<Option<Context>> = Mutex::new(None);

/// A running hook; dropping it ends the hook thread.
#[derive(Debug)]
pub struct Handle {
    active: Arc<AtomicBool>,
    thread_id: Arc<AtomicU32>,
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.active.store(false, Ordering::SeqCst);
        let thread = self.thread_id.load(Ordering::SeqCst);
        if thread != 0 {
            unsafe { PostThreadMessageW(thread, WM_QUIT, 0, 0) };
        }
    }
}

unsafe extern "system" fn on_key(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // HC_ACTION == 0; negative codes must be passed on untouched.
    if code == 0 && lparam != 0 {
        let info = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        let pressed = match wparam as u32 {
            WM_KEYDOWN | WM_SYSKEYDOWN => Some(true),
            WM_KEYUP | WM_SYSKEYUP => Some(false),
            _ => None,
        };
        if let (Some(pressed), Ok(mut guard)) = (pressed, CONTEXT.try_lock())
            && let Some(ctx) = guard.as_mut()
            && ctx.active.load(Ordering::SeqCst)
            && let Some(edge) = ctx.filter.feed(info.vkCode, pressed)
        {
            (ctx.sink)(edge);
        }
    }
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

fn run_hook(
    target: u32,
    sink: Sink,
    active: Arc<AtomicBool>,
    thread_id: Arc<AtomicU32>,
    ready: mpsc::Sender<Result<(), StartError>>,
) {
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        // Creates this thread's message queue so WM_QUIT can be posted.
        PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_NOREMOVE);
        if let Ok(mut guard) = CONTEXT.lock() {
            *guard = Some(Context {
                filter: KeyFilter::new(target),
                sink,
                active: active.clone(),
            });
        }
        let module = GetModuleHandleW(std::ptr::null());
        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(on_key), module, 0);
        if hook.is_null() {
            clear_context(&active);
            let _ = ready.send(Err(StartError::Failed(
                "Couldn't install the keyboard hook.".into(),
            )));
            return;
        }
        thread_id.store(GetCurrentThreadId(), Ordering::SeqCst);
        let _ = ready.send(Ok(()));
        if active.load(Ordering::SeqCst) {
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {}
        }
        UnhookWindowsHookEx(hook);
        clear_context(&active);
    }
}

/// Drop the global context, unless a newer hook already replaced it.
fn clear_context(active: &Arc<AtomicBool>) {
    if let Ok(mut guard) = CONTEXT.lock()
        && guard
            .as_ref()
            .is_some_and(|ctx| Arc::ptr_eq(&ctx.active, active))
    {
        *guard = None;
    }
}

pub fn start(key: &str, sink: Sink) -> Result<Handle, StartError> {
    let Some(target) = windows_vk(key) else {
        return Err(StartError::Unsupported(
            "This key can't be used system-wide.".into(),
        ));
    };
    let active = Arc::new(AtomicBool::new(true));
    let thread_id = Arc::new(AtomicU32::new(0));
    let (ready_tx, ready_rx) = mpsc::channel();
    {
        let (active, thread_id) = (active.clone(), thread_id.clone());
        std::thread::Builder::new()
            .name("quill-ptt-hook".into())
            .spawn(move || run_hook(target, sink, active, thread_id, ready_tx))
            .map_err(|_| StartError::Failed("Couldn't start the key listener.".into()))?;
    }
    match ready_rx.recv_timeout(Duration::from_secs(2)) {
        Ok(Ok(())) => Ok(Handle { active, thread_id }),
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
    true
}

pub fn open_permission_settings() {}
