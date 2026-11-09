use objc2::{
    AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send,
    rc::Retained,
    runtime::{AnyClass, ProtocolObject},
};
use dispatch2::DispatchQueue;
use objc2_foundation::{NSObject, NSObjectProtocol, NSRunLoop, NSRunLoopCommonModes};
use std::cell::{Cell, RefCell};
use objc2_quartz_core::{
    CAMetalDisplayLink, CAMetalDisplayLinkDelegate, CAMetalDisplayLinkUpdate, CAMetalLayer,
};

thread_local! {
    /// A link's update callback is running on this (the main) thread.
    static IN_UPDATE: Cell<bool> = const { Cell::new(false) };
    /// Links dropped during an update, paused and waiting to be invalidated
    /// once it returns.
    static RETIRED: RefCell<Vec<(Retained<CAMetalDisplayLink>, Retained<MetalDisplayLinkDelegate>)>> =
        const { RefCell::new(Vec::new()) };
}

/// Quill: invalidates and releases the links retired during an update, so
/// none is still attached to a layer that a draw asks for `nextDrawable`.
/// Main thread only; does nothing inside an update.
pub fn invalidate_retired() {
    if IN_UPDATE.get() {
        return;
    }
    let retired = RETIRED.with_borrow_mut(std::mem::take);
    for (link, _delegate) in retired {
        link.invalidate();
    }
}

struct MetalDisplayLinkDelegateIvars {
    callback: Box<dyn Fn(&CAMetalDisplayLinkUpdate)>,
}

define_class!(
    // SAFETY: `NSObject` has no subclassing requirements.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = MetalDisplayLinkDelegateIvars]
    struct MetalDisplayLinkDelegate;

    unsafe impl NSObjectProtocol for MetalDisplayLinkDelegate {}

    unsafe impl CAMetalDisplayLinkDelegate for MetalDisplayLinkDelegate {
        #[unsafe(method(metalDisplayLink:needsUpdate:))]
        fn needs_update(&self, _link: &CAMetalDisplayLink, update: &CAMetalDisplayLinkUpdate) {
            // The callback may destroy its frame source, releasing our owner's retain.
            let delegate = self.retain();
            let outer = IN_UPDATE.replace(true);
            (delegate.ivars().callback)(update);
            IN_UPDATE.set(outer);
        }
    }
);

impl MetalDisplayLinkDelegate {
    fn new(
        main_thread: MainThreadMarker,
        callback: impl Fn(&CAMetalDisplayLinkUpdate) + 'static,
    ) -> Retained<Self> {
        let this = Self::alloc(main_thread).set_ivars(MetalDisplayLinkDelegateIvars {
            callback: Box::new(callback),
        });
        // SAFETY: `NSObject`'s `init` is its designated initializer.
        unsafe { msg_send![super(this), init] }
    }
}

/// A main-run-loop source of Metal drawables on macOS 14+ and iOS/iPadOS 17+.
///
/// Callbacks run on the run loop the link is registered with:
/// <https://developer.apple.com/documentation/quartzcore/cametaldisplaylink>.
/// This wrapper uses the main run loop because GPUI renders on the foreground thread.
pub struct MetalFrameSource {
    link: Retained<CAMetalDisplayLink>,
    // The link holds its delegate weakly.
    _delegate: Retained<MetalDisplayLinkDelegate>,
    _main_thread: MainThreadMarker,
}

impl MetalFrameSource {
    /// Whether this system has `CAMetalDisplayLink` (macOS 14+, iOS/iPadOS 17+).
    pub fn is_available() -> bool {
        AnyClass::get(c"CAMetalDisplayLink").is_some()
    }

    /// Starts delivering updates for `layer`, or returns `None` where
    /// `CAMetalDisplayLink` is unavailable.
    pub fn new(
        layer: &CAMetalLayer,
        main_thread: MainThreadMarker,
        callback: impl Fn(&CAMetalDisplayLinkUpdate) + 'static,
    ) -> Option<Self> {
        if !Self::is_available() {
            return None;
        }
        invalidate_retired();
        let delegate = MetalDisplayLinkDelegate::new(main_thread, callback);
        let link = CAMetalDisplayLink::initWithMetalLayer(CAMetalDisplayLink::alloc(), layer);
        link.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        // The default range follows the display and system power policy:
        // https://developer.apple.com/documentation/quartzcore/cametaldisplaylink/preferredframeraterange
        // SAFETY: the retained marker confines the source to the main thread,
        // and the main run loop outlives the source.
        unsafe { link.addToRunLoop_forMode(&NSRunLoop::mainRunLoop(), NSRunLoopCommonModes) };
        Some(Self {
            link,
            _delegate: delegate,
            _main_thread: main_thread,
        })
    }

    /// A paused link delivers no updates and acquires no drawables.
    pub fn set_paused(&self, paused: bool) {
        self.link.setPaused(paused);
    }
}

impl Drop for MetalFrameSource {
    /// Quill: a source dropped from inside its own update (the window going
    /// idle stops its link from the frame the link asked for) only pauses
    /// the link there; it is invalidated on the main queue after the update
    /// returns, as Chromium's link does from its callbacks. Invalidating it
    /// mid-update is the suspect for a null-pointer crash about a second
    /// after launch on macOS 14, when the window first goes idle (#1389).
    fn drop(&mut self) {
        if !IN_UPDATE.get() {
            self.link.invalidate();
            return;
        }
        self.link.setPaused(true);
        RETIRED.with_borrow_mut(|retired| {
            retired.push((self.link.clone(), self._delegate.clone()));
        });
        DispatchQueue::main().exec_async(invalidate_retired);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2_metal::MTLCreateSystemDefaultDevice;

    /// The window going idle drops its link from the update the link asked
    /// for: the link is paused there and invalidated only once the update
    /// is over (#1389).
    #[test]
    fn a_link_dropped_during_its_update_is_invalidated_after_it() {
        let Some(device) = MTLCreateSystemDefaultDevice() else {
            return;
        };
        if !MetalFrameSource::is_available() {
            return;
        }
        let layer = CAMetalLayer::new();
        layer.setDevice(Some(&device));
        // SAFETY: test only; the link is never run, so no callback reaches
        // another thread.
        let main_thread = unsafe { MainThreadMarker::new_unchecked() };
        let source = MetalFrameSource::new(&layer, main_thread, |_| {}).expect("link");
        let link = source.link.clone();

        IN_UPDATE.set(true);
        drop(source);
        assert!(link.isPaused(), "paused inside the update");
        invalidate_retired();
        assert_eq!(RETIRED.with_borrow(Vec::len), 1, "kept while inside");
        IN_UPDATE.set(false);

        invalidate_retired();
        assert_eq!(RETIRED.with_borrow(Vec::len), 0, "invalidated after");
    }
}
