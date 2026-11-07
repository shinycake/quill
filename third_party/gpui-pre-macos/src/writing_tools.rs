//! Quill: keeps the Writing Tools framework off the main thread at launch.
//!
//! The first message to `NSTextInputContext` runs its `+initialize`, which
//! registers for Siri availability through `+[NSWritingToolsCoordinator
//! addObserverForEnhancedSiriAvailabilityDidChangeUsingBlock:]`. That method
//! loads WritingToolsUI and about 460 libraries behind it. AppKit's
//! first-responder observer sends that first message on the first run loop
//! pass after a window becomes key, so on a cold launch the main thread spent
//! about 280 ms there, from just after the window showed until
//! `applicationDidFinishLaunching:`, and input and redraws waited. Two more
//! things load the framework on first use: a main menu with an Edit menu
//! (`_addTextInputMenuItems:` asks `isWritingToolsAvailable`) and a view's
//! input context.
//!
//! So until a worker has loaded the framework:
//! - that registration is held (`hold`) and made afterwards, with the same
//!   block. It only listens for changes, so nothing is missed;
//! - `set_main_menu` keeps the menu bar and installs it afterwards;
//! - views answer `isEditable` NO (`text_input_ready_now`), so AppKit makes no
//!   input context for them. A key makes them editable at once
//!   (`text_input_ready`), and that key waits for the rest of the load.
//!
//! The worker loads the framework's images one at a time (`staged_load`):
//! one `dlopen` would hold dyld's and the Objective-C runtime's locks for
//! the whole load, and the main thread would wait on them anyway.
//! `finishLaunching` loads images of its own, so `start_load` runs from
//! `applicationDidFinishLaunching:`.

use dispatch2::DispatchQueue;
use objc2::{
    msg_send,
    rc::{Retained, autoreleasepool},
    runtime::{AnyClass, AnyObject, Imp, Sel},
    sel,
};
use objc2_app_kit::NSMenu;
use parking_lot::Mutex;
use std::{
    cell::Cell,
    ffi::{CStr, c_void},
    sync::{
        Once, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

/// Its install name, the path the shared cache knows it by.
const WRITING_TOOLS_UI: &CStr =
    c"/System/Library/PrivateFrameworks/WritingToolsUI.framework/Versions/A/WritingToolsUI";
const COORDINATOR: &CStr = c"NSWritingToolsCoordinator";

unsafe extern "C" {
    fn _Block_copy(block: *const c_void) -> *mut c_void;
    fn _Block_release(block: *const c_void);
}

type AddObserver = unsafe extern "C-unwind" fn(&AnyClass, Sel, *mut c_void) -> *mut AnyObject;

/// The original `addObserverForEnhancedSiriAvailabilityDidChangeUsingBlock:`.
static ORIGINAL: OnceLock<AddObserver> = OnceLock::new();
/// While the registration is held, the blocks AppKit passed it (copied, as
/// addresses).
static HELD: Mutex<Option<Vec<usize>>> = Mutex::new(None);
/// The worker has loaded the framework (or it could not start).
static LOADED: AtomicBool = AtomicBool::new(false);
/// Views may make their input contexts.
static TEXT_INPUT_READY: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// The menu bar `set_main_menu` got before the framework was loaded.
    static PENDING_MENU: Cell<Option<Retained<NSMenu>>> = const { Cell::new(None) };
    /// What `after_writing_tools_load` got before the framework was loaded.
    static WAITING: Cell<Vec<Box<dyn FnOnce()>>> = const { Cell::new(Vec::new()) };
}

fn add_observer_sel() -> Sel {
    sel!(addObserverForEnhancedSiriAvailabilityDidChangeUsingBlock:)
}

/// Holds AppKit's Siri availability registration until `start_load` is done.
/// Call once, before the first window is created. Systems without
/// `NSWritingToolsCoordinator` (before macOS 15.2) have nothing to hold.
pub(crate) fn hold() {
    let Some(class) = AnyClass::get(COORDINATOR) else {
        return;
    };
    let Some(method) = class.class_method(add_observer_sel()) else {
        return;
    };
    if ORIGINAL.get().is_some() {
        return;
    }
    *HELD.lock() = Some(Vec::new());
    // SAFETY: `held_add_observer` has the method's signature (a class, its
    // selector and a block, returning an observer token).
    let original = unsafe {
        let replacement: AddObserver = held_add_observer;
        method.set_implementation(std::mem::transmute::<AddObserver, Imp>(replacement))
    };
    // SAFETY: `original` is the method's own implementation, with the
    // signature above.
    let _ = ORIGINAL.set(unsafe { std::mem::transmute::<Imp, AddObserver>(original) });
}

unsafe extern "C-unwind" fn held_add_observer(
    class: &AnyClass,
    sel: Sel,
    block: *mut c_void,
) -> *mut AnyObject {
    if let Some(blocks) = HELD.lock().as_mut() {
        if !block.is_null() {
            // SAFETY: AppKit passes a block; the copy is released after
            // `finish_load` has registered it.
            blocks.push(unsafe { _Block_copy(block) } as usize);
        }
        // AppKit drops the token: the observer is never removed.
        return std::ptr::null_mut();
    }
    // A call that raced `finish_load` putting the original back.
    match ORIGINAL.get() {
        // SAFETY: the original, with what AppKit passed.
        Some(original) => unsafe { original(class, sel, block) },
        None => std::ptr::null_mut(),
    }
}

/// Starts the worker that loads the framework. Call from
/// `applicationDidFinishLaunching:`; later calls do nothing.
pub(crate) fn start_load() {
    static STARTED: Once = Once::new();
    STARTED.call_once(|| {
        let spawned = std::thread::Builder::new()
            .name("writing-tools".into())
            .spawn(|| {
                // A missing framework fails to load here, and AppKit's own
                // loads then fail the same way.
                crate::staged_load::load(WRITING_TOOLS_UI);
                DispatchQueue::main().exec_async(finish_load);
            });
        if spawned.is_err() {
            finish_load();
        }
    });
}

/// Main thread only: the framework is loaded, so AppKit gets what waited.
fn finish_load() {
    if LOADED.swap(true, Ordering::AcqRel) {
        return;
    }
    crate::launch_mark("writing_tools");
    let blocks = HELD.lock().take().unwrap_or_default();
    if let Some(&original) = ORIGINAL.get()
        && let Some(class) = AnyClass::get(COORDINATOR)
        && let Some(method) = class.class_method(add_observer_sel())
    {
        // SAFETY: `original` came from this method, so it has its signature;
        // each block was copied in `held_add_observer` and is released once
        // registered.
        unsafe {
            method.set_implementation(std::mem::transmute::<AddObserver, Imp>(original));
            autoreleasepool(|_| {
                for block in blocks {
                    let _ = original(class, add_observer_sel(), block as *mut c_void);
                    _Block_release(block as *const c_void);
                }
            });
        }
    }
    text_input_ready();
    if let Some(menu) = PENDING_MENU.take() {
        install_main_menu(&menu);
    }
    for callback in WAITING.take() {
        callback();
    }
}

/// Runs `callback` on the main thread once the worker has loaded the Writing
/// Tools framework, or now if it has. For launch work that loads frameworks
/// of its own: dyld would make the main thread wait for the worker's load.
/// Main thread only.
pub fn after_writing_tools_load(callback: impl FnOnce() + 'static) {
    if LOADED.load(Ordering::Acquire) {
        callback();
    } else {
        let mut waiting = WAITING.take();
        waiting.push(Box::new(callback));
        WAITING.set(waiting);
    }
}

/// Installs `menu` as the menu bar, or keeps it until the framework is
/// loaded. Main thread only.
pub(crate) fn set_main_menu(menu: Retained<NSMenu>) {
    if LOADED.load(Ordering::Acquire) {
        install_main_menu(&menu);
    } else {
        PENDING_MENU.set(Some(menu));
    }
}

fn install_main_menu(menu: &NSMenu) {
    // SAFETY: on the main thread, with the shared application.
    unsafe {
        let app: *mut AnyObject = msg_send![objc2::class!(NSApplication), sharedApplication];
        let _: () = msg_send![app, setMainMenu: menu];
    }
}

pub(crate) fn text_input_ready_now() -> bool {
    TEXT_INPUT_READY.load(Ordering::Acquire)
}

/// Lets views make their input contexts, and has AppKit make and activate the
/// key view's at once, so the key that called this (or one queued behind a
/// busy main thread) is read by an active context: an inactive one drops a
/// dead key such as Option-E. Main thread only: from a key event, or once the
/// framework is loaded.
pub(crate) fn text_input_ready() {
    if TEXT_INPUT_READY.swap(true, Ordering::AcqRel) {
        return;
    }
    // SAFETY: on the main thread, with the shared application.
    unsafe {
        let app: *mut AnyObject = msg_send![objc2::class!(NSApplication), sharedApplication];
        let _: () = msg_send![app, updateWindows];
    }
}
