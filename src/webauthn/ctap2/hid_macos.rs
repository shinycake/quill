//! FIDO devices through IOKit HID, after libfido2's `hid_osx.c`: the HID
//! manager lists the devices, those with the FIDO primary usage page are
//! opened, output reports go out with `IOHIDDeviceSetReport`, and input
//! reports arrive through a callback while this thread runs its run loop
//! in a private mode. Each device is created, used and dropped on the
//! ceremony's worker thread.
#![deny(clippy::indexing_slicing)]

use super::{FIDO_USAGE_CTAPHID, FIDO_USAGE_PAGE};
use crate::webauthn::ctap2::ctaphid::{HidDevice, HidError};
use core_foundation_sys::base::{
    CFGetTypeID, CFIndex, CFRelease, CFRetain, CFTypeRef, kCFAllocatorDefault,
};
use core_foundation_sys::number::{CFNumberGetTypeID, CFNumberGetValue, kCFNumberSInt64Type};
use core_foundation_sys::runloop::{CFRunLoopGetCurrent, CFRunLoopRef, CFRunLoopRunInMode};
use core_foundation_sys::set::{CFSetGetCount, CFSetGetValues, CFSetRef};
use core_foundation_sys::string::{CFStringCreateWithCString, CFStringRef, kCFStringEncodingUTF8};
use std::collections::VecDeque;
use std::ffi::{CStr, c_void};
use std::time::{Duration, Instant};

type IOHIDManagerRef = *mut c_void;
type IOHIDDeviceRef = *mut c_void;
type IOReturn = i32;
type InputCallback = extern "C" fn(
    context: *mut c_void,
    result: IOReturn,
    sender: *mut c_void,
    report_type: u32,
    report_id: u32,
    report: *mut u8,
    report_length: CFIndex,
);

const K_IO_RETURN_SUCCESS: IOReturn = 0;
const K_IOHID_REPORT_TYPE_OUTPUT: u32 = 1;
/// `kCFRunLoopRunFinished`: the mode has no sources.
const RUN_FINISHED: i32 = 1;

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOHIDManagerCreate(allocator: *const c_void, options: u32) -> IOHIDManagerRef;
    fn IOHIDManagerSetDeviceMatching(manager: IOHIDManagerRef, matching: *const c_void);
    fn IOHIDManagerCopyDevices(manager: IOHIDManagerRef) -> CFSetRef;
    fn IOHIDDeviceGetProperty(device: IOHIDDeviceRef, key: CFStringRef) -> CFTypeRef;
    fn IOHIDDeviceOpen(device: IOHIDDeviceRef, options: u32) -> IOReturn;
    fn IOHIDDeviceClose(device: IOHIDDeviceRef, options: u32) -> IOReturn;
    fn IOHIDDeviceScheduleWithRunLoop(
        device: IOHIDDeviceRef,
        run_loop: CFRunLoopRef,
        mode: CFStringRef,
    );
    fn IOHIDDeviceUnscheduleFromRunLoop(
        device: IOHIDDeviceRef,
        run_loop: CFRunLoopRef,
        mode: CFStringRef,
    );
    fn IOHIDDeviceRegisterInputReportCallback(
        device: IOHIDDeviceRef,
        report: *mut u8,
        report_length: CFIndex,
        callback: Option<InputCallback>,
        context: *mut c_void,
    );
    fn IOHIDDeviceSetReport(
        device: IOHIDDeviceRef,
        report_type: u32,
        report_id: CFIndex,
        report: *const u8,
        report_length: CFIndex,
    ) -> IOReturn;
}

/// A CFString from a C literal; null on failure. The caller releases it.
fn cf_string(text: &CStr) -> CFStringRef {
    // SAFETY: `text` is a valid NUL-terminated C string for the call.
    unsafe { CFStringCreateWithCString(kCFAllocatorDefault, text.as_ptr(), kCFStringEncodingUTF8) }
}

/// An integer property of the device, when it is a CFNumber.
fn number_property(device: IOHIDDeviceRef, key: &CStr) -> Option<i64> {
    let name = cf_string(key);
    if name.is_null() {
        return None;
    }
    // SAFETY: `device` is a live IOHIDDevice and `name` a live CFString;
    // the property follows the Get rule (not owned), so it is not released.
    let value = unsafe { IOHIDDeviceGetProperty(device, name) };
    // SAFETY: `name` was created above and is released once.
    unsafe { CFRelease(name.cast()) };
    if value.is_null() {
        return None;
    }
    // SAFETY: `value` is a live CF object; its type is checked before it is
    // read as a CFNumber into an i64 that outlives the call.
    unsafe {
        if CFGetTypeID(value) != CFNumberGetTypeID() {
            return None;
        }
        let mut out: i64 = 0;
        let ok = CFNumberGetValue(value.cast(), kCFNumberSInt64Type, (&raw mut out).cast());
        ok.then_some(out)
    }
}

/// Reports the callback received and not read yet.
struct Inbox {
    buffer: Vec<u8>,
    reports: VecDeque<Vec<u8>>,
}

extern "C" fn on_input(
    context: *mut c_void,
    result: IOReturn,
    _sender: *mut c_void,
    _report_type: u32,
    _report_id: u32,
    report: *mut u8,
    report_length: CFIndex,
) {
    crate::thread_guard::ffi_boundary((), || {
        if context.is_null() || report.is_null() || result != K_IO_RETURN_SUCCESS {
            return;
        }
        let Ok(len) = usize::try_from(report_length) else {
            return;
        };
        // SAFETY: `context` is the `Inbox` registered with this callback;
        // it lives until the callback is unregistered in `Drop`, and the
        // callback only runs inside `CFRunLoopRunInMode` on this thread.
        let inbox = unsafe { &mut *context.cast::<Inbox>() };
        let len = len.min(inbox.buffer.len());
        // SAFETY: IOKit hands `report` with `report_length` valid bytes,
        // and `len` is no longer than that.
        let bytes = unsafe { std::slice::from_raw_parts(report, len) };
        inbox.reports.push_back(bytes.to_vec());
    });
}

struct IoKitDevice {
    device: IOHIDDeviceRef,
    run_loop: CFRunLoopRef,
    mode: CFStringRef,
    inbox: Box<Inbox>,
    report_len: usize,
}

// SAFETY: an `IoKitDevice` is opened, used and dropped on the ceremony's
// worker thread (`ceremony::run_*` call `hid::open_all` there); `Send` is
// only needed to name it as `Box<dyn HidDevice>`, it never crosses threads.
unsafe impl Send for IoKitDevice {}

impl IoKitDevice {
    fn open(device: IOHIDDeviceRef, report_len: usize) -> Option<Self> {
        let mode = cf_string(c"org.quill.passkey.hid");
        if mode.is_null() {
            return None;
        }
        // SAFETY: `device` is a live IOHIDDevice from the manager's set.
        let opened = unsafe { IOHIDDeviceOpen(device, 0) };
        if opened != K_IO_RETURN_SUCCESS {
            crate::webauthn::log(&format!("could not open the key: 0x{opened:08x}"));
            // SAFETY: `mode` was created above and is released once.
            unsafe { CFRelease(mode.cast()) };
            return None;
        }
        // SAFETY: retained so the device outlives the manager's set; the
        // matching release is in `Drop`.
        unsafe { CFRetain(device.cast_const()) };
        let mut inbox = Box::new(Inbox {
            buffer: vec![0; report_len],
            reports: VecDeque::new(),
        });
        // SAFETY: getting the current thread's run loop has no preconditions.
        let run_loop = unsafe { CFRunLoopGetCurrent() };
        let context: *mut Inbox = &mut *inbox;
        // SAFETY: the buffer and the context are owned by `inbox`, a heap
        // allocation that stays put and lives until `Drop` unregisters the
        // callback; the device is open; the run loop is this thread's.
        unsafe {
            IOHIDDeviceRegisterInputReportCallback(
                device,
                inbox.buffer.as_mut_ptr(),
                CFIndex::try_from(report_len).unwrap_or(64),
                Some(on_input),
                context.cast(),
            );
            IOHIDDeviceScheduleWithRunLoop(device, run_loop, mode);
        }
        Some(Self {
            device,
            run_loop,
            mode,
            inbox,
            report_len,
        })
    }
}

impl Drop for IoKitDevice {
    fn drop(&mut self) {
        // SAFETY: the device was opened and scheduled in `open` on this
        // thread's run loop; unregistering the callback before the inbox
        // is freed keeps IOKit from writing into freed memory, then the
        // retain from `open` and the mode string are released once each.
        unsafe {
            IOHIDDeviceRegisterInputReportCallback(
                self.device,
                self.inbox.buffer.as_mut_ptr(),
                CFIndex::try_from(self.report_len).unwrap_or(64),
                None,
                std::ptr::null_mut(),
            );
            IOHIDDeviceUnscheduleFromRunLoop(self.device, self.run_loop, self.mode);
            IOHIDDeviceClose(self.device, 0);
            CFRelease(self.device.cast_const());
            CFRelease(self.mode.cast());
        }
    }
}

impl HidDevice for IoKitDevice {
    fn report_len(&self) -> usize {
        self.report_len
    }

    fn write_report(&mut self, report: &[u8]) -> Result<(), HidError> {
        let len = CFIndex::try_from(report.len()).map_err(|_| HidError::TooLong)?;
        // SAFETY: the device is open; `report` is valid for `len` bytes
        // for the duration of this synchronous call.
        let result = unsafe {
            IOHIDDeviceSetReport(
                self.device,
                K_IOHID_REPORT_TYPE_OUTPUT,
                0,
                report.as_ptr(),
                len,
            )
        };
        if result == K_IO_RETURN_SUCCESS {
            Ok(())
        } else {
            Err(HidError::Io(format!("IOHIDDeviceSetReport 0x{result:08x}")))
        }
    }

    fn read_report(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>, HidError> {
        let until = Instant::now() + timeout;
        loop {
            if let Some(mut report) = self.inbox.reports.pop_front() {
                report.resize(self.report_len, 0);
                return Ok(Some(report));
            }
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(None);
            }
            // SAFETY: `mode` is a live CFString and the device is scheduled
            // on this thread's run loop in that mode; the input callback
            // runs inside this call, on this thread.
            let ran = unsafe { CFRunLoopRunInMode(self.mode, left.as_secs_f64(), 1) };
            if ran == RUN_FINISHED {
                std::thread::sleep(left.min(Duration::from_millis(20)));
            }
        }
    }
}

pub(super) fn open_all() -> Vec<Box<dyn HidDevice>> {
    let mut devices: Vec<Box<dyn HidDevice>> = Vec::new();
    // SAFETY: creating a HID manager with the default allocator.
    let manager = unsafe { IOHIDManagerCreate(kCFAllocatorDefault.cast(), 0) };
    if manager.is_null() {
        return devices;
    }
    // SAFETY: `manager` is live; a null matching dictionary matches every
    // HID device (filtered by usage page below, as libfido2 does).
    let set = unsafe {
        IOHIDManagerSetDeviceMatching(manager, std::ptr::null());
        IOHIDManagerCopyDevices(manager)
    };
    if !set.is_null() {
        // SAFETY: `set` is a live CFSet owned by us (Copy rule).
        let count = usize::try_from(unsafe { CFSetGetCount(set) }).unwrap_or_default();
        let mut refs: Vec<*const c_void> = vec![std::ptr::null(); count];
        // SAFETY: `refs` has room for exactly `count` values.
        unsafe { CFSetGetValues(set, refs.as_mut_ptr()) };
        for device in refs {
            let device = device.cast_mut();
            if device.is_null() {
                continue;
            }
            let page = number_property(device, c"PrimaryUsagePage");
            let usage = number_property(device, c"PrimaryUsage");
            if page != Some(i64::from(FIDO_USAGE_PAGE))
                || usage != Some(i64::from(FIDO_USAGE_CTAPHID))
            {
                continue;
            }
            let input = number_property(device, c"MaxInputReportSize").unwrap_or(64);
            let output = number_property(device, c"MaxOutputReportSize").unwrap_or(64);
            let Some(report_len) = usize::try_from(input)
                .ok()
                .filter(|len| input == output && (8..=64).contains(len))
            else {
                continue;
            };
            if let Some(opened) = IoKitDevice::open(device, report_len) {
                devices.push(Box::new(opened));
            }
        }
        // SAFETY: releasing the set we own; the opened devices hold their
        // own retain.
        unsafe { CFRelease(set.cast()) };
    }
    // SAFETY: releasing the manager we created.
    unsafe { CFRelease(manager.cast_const()) };
    devices
}
