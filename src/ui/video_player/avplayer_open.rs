//! Hearing that an AVPlayer item left `unknown` (ready or failed) without
//! polling it: a key-value observer on the item's `status`.
#![deny(clippy::indexing_slicing)]

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AllocAnyThread, DefinedClass, define_class, msg_send};
use objc2_av_foundation::{AVPlayerItem, AVPlayerItemStatus};
use objc2_foundation::{
    NSDictionary, NSKeyValueObservingOptions, NSObject, NSObjectProtocol, NSString, ns_string,
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

/// The observer on one item; ended by [`Self::end`].
pub(super) struct OpenWatch(Retained<Observer>);

impl OpenWatch {
    /// Runs `wake` (from whichever thread AVFoundation changes the status
    /// on) once `item` is ready or failed, at once when it already is.
    pub(super) fn start(item: &AVPlayerItem, wake: Wake) -> Self {
        let this = Observer::alloc().set_ivars(Mutex::new(Some(wake)));
        // SAFETY: NSObject's designated initializer.
        let observer: Retained<Observer> = unsafe { msg_send![super(this), init] };
        // SAFETY: the observer outlives the registration (`end` removes it
        // before the watch is dropped) and the context is null.
        unsafe {
            item.addObserver_forKeyPath_options_context(
                &observer,
                ns_string!("status"),
                NSKeyValueObservingOptions::Initial | NSKeyValueObservingOptions::New,
                ptr::null_mut(),
            );
        }
        Self(observer)
    }

    /// Stops observing `item`, the one this watch started on.
    pub(super) fn end(self, item: &AVPlayerItem) {
        // SAFETY: removes the registration `start` made on this item.
        unsafe { item.removeObserver_forKeyPath(&self.0, ns_string!("status")) };
    }
}
