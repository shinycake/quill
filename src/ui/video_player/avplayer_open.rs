//! Hearing that an AVPlayer item left `unknown` (ready or failed) without
//! polling it: a key-value observer on the item's `status`.
#![deny(clippy::indexing_slicing)]

use dispatch2::DispatchQueue;
use objc2::Message;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AllocAnyThread, DefinedClass, define_class, msg_send};
use objc2_av_foundation::{AVPlayerItem, AVPlayerItemStatus};
use objc2_foundation::{
    NSDictionary, NSKeyValueObservingOptions, NSObject, NSObjectNSKeyValueObserverRegistration,
    NSObjectProtocol, NSString, ns_string,
};
use std::ffi::c_void;
use std::ptr;
use std::sync::Mutex;

type Wake = Box<dyn FnOnce() + Send>;

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "QuillItemOpenObserver"]
    #[ivars = Mutex<Option<Wake>>]
    struct Observer;

    unsafe impl NSObjectProtocol for Observer {}

    impl Observer {
        #[unsafe(method(observeValueForKeyPath:ofObject:change:context:))]
        fn observe(
            &self,
            _key_path: Option<&NSString>,
            object: Option<&AnyObject>,
            _change: Option<&NSDictionary<NSString, AnyObject>>,
            _context: *mut c_void,
        ) {
            quill::thread_guard::ffi_boundary((), || {
                let Some(item) = object.and_then(|object| object.downcast_ref::<AVPlayerItem>())
                else {
                    return;
                };
                // SAFETY: a plain property read on the item that notified us.
                if unsafe { item.status() } == AVPlayerItemStatus::Unknown {
                    return;
                }
                let wake = self
                    .ivars()
                    .lock()
                    .ok()
                    .and_then(|mut slot| slot.take());
                if let Some(wake) = wake {
                    wake();
                }
            });
        }
    }
);

/// The observer on one item, registered at [`Self::start`] and removed when
/// the watch drops: no owner has to remember to end it, so an early return
/// or an unwind cannot leave a registered observer freed.
pub(super) struct OpenWatch {
    observer: Retained<Observer>,
    item: Retained<AVPlayerItem>,
}

/// The observer on its way out: released on the main queue, so the last
/// release never runs inside a callback that is delivering to it.
struct Release(#[allow(dead_code)] Retained<Observer>);

// SAFETY: it only moves to the main queue to be dropped there; nothing
// else touches the observer through it.
unsafe impl Send for Release {}

impl OpenWatch {
    /// Runs `wake` (from whichever thread AVFoundation changes the status
    /// on) once `item` is ready or failed, at once when it already is.
    pub(super) fn start(item: &AVPlayerItem, wake: Wake) -> Self {
        let this = Observer::alloc().set_ivars(Mutex::new(Some(wake)));
        // SAFETY: NSObject's designated initializer.
        let observer: Retained<Observer> = unsafe { msg_send![super(this), init] };
        // SAFETY: the observer outlives the registration (`Drop` removes it
        // before the observer is released) and the context is null.
        unsafe {
            item.addObserver_forKeyPath_options_context(
                &observer,
                ns_string!("status"),
                NSKeyValueObservingOptions::Initial | NSKeyValueObservingOptions::New,
                ptr::null_mut(),
            );
        }
        Self {
            observer,
            item: item.retain(),
        }
    }
}

impl Drop for OpenWatch {
    fn drop(&mut self) {
        // SAFETY: removes the registration `start` made on this item.
        unsafe {
            self.item
                .removeObserver_forKeyPath(&self.observer, ns_string!("status"))
        };
        // AVFoundation may post the status from a thread of its own, so a
        // callback can be running as the watch drops: the observer is let
        // go on the main queue, after the registration is gone.
        let last = Release(self.observer.clone());
        DispatchQueue::main().exec_async(move || drop(last));
    }
}
