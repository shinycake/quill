//! caBLE advertisements over CoreBluetooth (tdesktop
//! `cable_scanner_mac.mm`): a `CBCentralManager` on its own serial queue
//! scans with duplicates allowed once the radio is on, and hands the
//! service data under FDE2 or FFF9 to the ceremony.
//!
//! macOS kills an app that touches Bluetooth without
//! `NSBluetoothAlwaysUsageDescription` in its Info.plist, so the scanner
//! only starts when the running bundle has that key (Quill.app does,
//! `scripts/macos-package-smoke.sh`); a bare binary reads as "Bluetooth
//! unavailable".
#![deny(clippy::indexing_slicing)]

use super::ble::{FIDO_CABLE_UUID, GOOGLE_CABLE_UUID, ScanEvent, Scanner};
use super::eid::ADVERT_LEN;
use dispatch2::{DispatchQueue, DispatchRetained};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{AllocAnyThread, DefinedClass, define_class, msg_send};
use objc2_core_bluetooth::{
    CBAdvertisementDataServiceDataKey, CBCentralManager, CBCentralManagerDelegate,
    CBCentralManagerScanOptionAllowDuplicatesKey, CBManagerState, CBPeripheral, CBUUID,
};
use objc2_foundation::{
    NSBundle, NSData, NSDictionary, NSNumber, NSObject, NSObjectProtocol, NSString, ns_string,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

/// What the delegate shares with the scanner.
struct Shared {
    events: Mutex<Option<Sender<ScanEvent>>>,
    /// Availability goes out once (tdesktop `base::take(onAvailability)`).
    told: AtomicBool,
    stopped: AtomicBool,
}

impl Shared {
    fn send(&self, event: ScanEvent) {
        if self.stopped.load(Ordering::SeqCst) {
            return;
        }
        if let Ok(events) = self.events.lock()
            && let Some(events) = events.as_ref()
        {
            let _ = events.send(event);
        }
    }

    fn tell_available(&self, available: bool) {
        if !self.told.swap(true, Ordering::SeqCst) {
            self.send(ScanEvent::Available(available));
        }
    }
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "QuillCableScannerDelegate"]
    #[ivars = Arc<Shared>]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl CBCentralManagerDelegate for Delegate {
        #[unsafe(method(centralManagerDidUpdateState:))]
        fn did_update_state(&self, central: &CBCentralManager) {
            crate::thread_guard::ffi_boundary((), || self.update_state(central));
        }

        #[unsafe(method(centralManager:didDiscoverPeripheral:advertisementData:RSSI:))]
        fn did_discover(
            &self,
            _central: &CBCentralManager,
            _peripheral: &CBPeripheral,
            advertisement: &NSDictionary<NSString, AnyObject>,
            _rssi: &NSNumber,
        ) {
            crate::thread_guard::ffi_boundary((), || {
                for blob in cable_service_data(advertisement) {
                    self.ivars().send(ScanEvent::Advert(blob));
                }
            });
        }
    }
);

impl Delegate {
    fn new(shared: Arc<Shared>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(shared);
        // SAFETY: NSObject's designated initializer.
        unsafe { msg_send![super(this), init] }
    }

    /// Powered on: scan everything, duplicates included, and say
    /// Bluetooth works. Unknown and resetting are passing states; anything
    /// else (off, unauthorized, unsupported) means no Bluetooth.
    fn update_state(&self, central: &CBCentralManager) {
        // SAFETY: a plain property read on the manager calling us.
        let state = unsafe { central.state() };
        if state == CBManagerState::PoweredOn {
            let yes = NSNumber::new_bool(true);
            // SAFETY: the key is CoreBluetooth's own constant.
            let key = unsafe { CBCentralManagerScanOptionAllowDuplicatesKey };
            let options =
                NSDictionary::<NSString, AnyObject>::from_slices(&[key], &[&*yes as &AnyObject]);
            // SAFETY: no service filter and an options dictionary of the
            // documented key and type, on the manager's own queue.
            unsafe { central.scanForPeripheralsWithServices_options(None, Some(&options)) };
            self.ivars().tell_available(true);
        } else if state != CBManagerState::Unknown && state != CBManagerState::Resetting {
            self.ivars().tell_available(false);
        }
    }
}

/// The service data under the caBLE UUIDs that is long enough to be an
/// advertisement.
fn cable_service_data(advertisement: &NSDictionary<NSString, AnyObject>) -> Vec<Vec<u8>> {
    // SAFETY: CoreBluetooth's own constant.
    let key = unsafe { CBAdvertisementDataServiceDataKey };
    let Some(service_data) = advertisement.objectForKey(key) else {
        return Vec::new();
    };
    let Ok(service_data) = service_data.downcast::<NSDictionary>() else {
        return Vec::new();
    };
    let wanted = [GOOGLE_CABLE_UUID, FIDO_CABLE_UUID].map(|short| {
        // SAFETY: a valid 16-bit UUID string.
        unsafe { CBUUID::UUIDWithString(&NSString::from_str(&format!("{short:04X}"))) }
    });
    let mut out = Vec::new();
    for uuid in service_data.allKeys().iter() {
        let Some(cb_uuid) = uuid.downcast_ref::<CBUUID>() else {
            continue;
        };
        if !wanted.iter().any(|want| cb_uuid.isEqual(Some(want))) {
            continue;
        }
        let Some(data) = service_data.objectForKey(&uuid) else {
            continue;
        };
        let Some(data) = data.downcast_ref::<NSData>() else {
            continue;
        };
        let bytes = data.to_vec();
        if bytes.len() >= ADVERT_LEN {
            out.push(bytes);
        }
    }
    out
}

/// Whether the running bundle declares why it uses Bluetooth.
fn has_usage_description() -> bool {
    NSBundle::mainBundle()
        .objectForInfoDictionaryKey(ns_string!("NSBluetoothAlwaysUsageDescription"))
        .is_some()
}

/// One scan; made, used and dropped on the ceremony's worker thread.
#[derive(Default)]
pub struct MacScanner {
    running: Option<Running>,
}

struct Running {
    shared: Arc<Shared>,
    central: Retained<CBCentralManager>,
    _delegate: Retained<Delegate>,
    _queue: DispatchRetained<DispatchQueue>,
}

impl Scanner for MacScanner {
    fn start(&mut self, events: Sender<ScanEvent>) -> bool {
        self.stop();
        if !has_usage_description() {
            crate::webauthn::log("no NSBluetoothAlwaysUsageDescription in the bundle");
            return false;
        }
        let shared = Arc::new(Shared {
            events: Mutex::new(Some(events)),
            told: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
        });
        let delegate = Delegate::new(shared.clone());
        let queue = DispatchQueue::new("org.shinycake.quill.cable", None);
        // SAFETY: the delegate outlives the manager (both live in
        // `Running`, the delegate is detached in `stop`), and the queue is
        // a serial queue kept alive with them.
        let central = unsafe {
            CBCentralManager::initWithDelegate_queue(
                CBCentralManager::alloc(),
                Some(ProtocolObject::from_ref(&*delegate)),
                Some(&queue),
            )
        };
        self.running = Some(Running {
            shared,
            central,
            _delegate: delegate,
            _queue: queue,
        });
        true
    }

    fn stop(&mut self) {
        let Some(running) = self.running.take() else {
            return;
        };
        running.shared.stopped.store(true, Ordering::SeqCst);
        if let Ok(mut events) = running.shared.events.lock() {
            *events = None;
        }
        // SAFETY: plain calls on a manager this scanner owns; the delegate
        // is detached before it is released.
        unsafe {
            running.central.stopScan();
            running.central.setDelegate(None);
        }
    }
}

impl Drop for MacScanner {
    fn drop(&mut self) {
        self.stop();
    }
}
