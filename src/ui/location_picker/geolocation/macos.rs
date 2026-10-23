//! CoreLocation (tdesktop `current_geo_location_mac.mm`): a
//! `CLLocationManager` made on the UI thread asks for "when in use"
//! permission if it was never given, then for one fix at three-kilometre
//! accuracy; `CLGeocoder` turns a point into an address.
//!
//! macOS denies location to an app without `NSLocationUsageDescription`
//! in its Info.plist, so a bare binary reads as "unavailable" without
//! asking; Quill.app has the key (`scripts/macos-package-smoke.sh`).

use super::join_address;
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{AllocAnyThread, DefinedClass, MainThreadOnly, define_class, msg_send};
use objc2_core_location::{
    CLAuthorizationStatus, CLError, CLLocation, CLLocationManager, CLLocationManagerDelegate,
    kCLLocationAccuracyThreeKilometers,
};
#[allow(deprecated)]
use objc2_core_location::{CLGeocoder, CLPlacemark};
use objc2_foundation::{
    MainThreadMarker, NSArray, NSBundle, NSError, NSLocale, NSObject, NSObjectProtocol, ns_string,
};
use quill::location_picker::{GeoOutcome, GeoPoint};
use std::cell::RefCell;
use std::sync::mpsc::{Receiver, Sender, channel};

struct Ivars {
    answer: RefCell<Option<Sender<GeoOutcome>>>,
}

impl Ivars {
    fn answer(&self, outcome: GeoOutcome) {
        if let Some(answer) = self.answer.borrow_mut().take() {
            let _ = answer.send(outcome);
        }
    }

    fn waiting(&self) -> bool {
        self.answer.borrow().is_some()
    }
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "QuillLocationDelegate"]
    #[ivars = Ivars]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl CLLocationManagerDelegate for Delegate {
        #[unsafe(method(locationManagerDidChangeAuthorization:))]
        fn did_change_authorization(&self, manager: &CLLocationManager) {
            quill::thread_guard::ffi_boundary((), || self.authorization_changed(manager));
        }

        #[unsafe(method(locationManager:didUpdateLocations:))]
        fn did_update_locations(
            &self,
            manager: &CLLocationManager,
            locations: &NSArray<CLLocation>,
        ) {
            quill::thread_guard::ffi_boundary((), || {
                let Some(location) = locations.lastObject() else {
                    return;
                };
                // SAFETY: a plain read on a location CoreLocation handed us.
                let coordinate = unsafe { location.coordinate() };
                // SAFETY: a stop on the manager calling us, on its thread.
                unsafe { manager.stopUpdatingLocation() };
                self.ivars().answer(
                    GeoPoint::new(coordinate.latitude, coordinate.longitude)
                        .map_or(GeoOutcome::Unavailable, GeoOutcome::Found),
                );
            });
        }

        #[unsafe(method(locationManager:didFailWithError:))]
        fn did_fail(&self, manager: &CLLocationManager, error: &NSError) {
            quill::thread_guard::ffi_boundary((), || {
                // tdesktop keeps waiting through kCLErrorLocationUnknown:
                // CoreLocation goes on trying after it.
                let code = error.code();
                if code == CLError::LocationUnknown.0 {
                    return;
                }
                // SAFETY: a stop on the manager calling us, on its thread.
                unsafe { manager.stopUpdatingLocation() };
                self.ivars().answer(if code == CLError::Denied.0 {
                    GeoOutcome::Denied
                } else {
                    GeoOutcome::Unavailable
                });
            });
        }
    }
);

impl Delegate {
    fn new(mtm: MainThreadMarker, answer: Sender<GeoOutcome>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars {
            answer: RefCell::new(Some(answer)),
        });
        // SAFETY: NSObject's designated initializer.
        unsafe { msg_send![super(this), init] }
    }

    fn authorization_changed(&self, manager: &CLLocationManager) {
        if !self.ivars().waiting() {
            return;
        }
        // SAFETY: plain calls on the manager on the thread that made it.
        unsafe {
            match manager.authorizationStatus() {
                CLAuthorizationStatus::NotDetermined => manager.requestWhenInUseAuthorization(),
                CLAuthorizationStatus::AuthorizedAlways
                | CLAuthorizationStatus::AuthorizedWhenInUse => manager.startUpdatingLocation(),
                CLAuthorizationStatus::Denied | CLAuthorizationStatus::Restricted => {
                    self.ivars().answer(GeoOutcome::Denied)
                }
                _ => self.ivars().answer(GeoOutcome::Unavailable),
            }
        }
    }
}

/// The running lookup: the manager and its delegate live until the next
/// lookup replaces them (the manager keeps only a weak delegate).
struct Running {
    manager: Retained<CLLocationManager>,
    _delegate: Retained<Delegate>,
}

thread_local! {
    static RUNNING: RefCell<Option<Running>> = const { RefCell::new(None) };
}

fn has_usage_description() -> bool {
    let bundle = NSBundle::mainBundle();
    bundle
        .objectForInfoDictionaryKey(ns_string!("NSLocationUsageDescription"))
        .is_some()
        || bundle
            .objectForInfoDictionaryKey(ns_string!("NSLocationWhenInUseUsageDescription"))
            .is_some()
}

pub(super) fn locate() -> Receiver<GeoOutcome> {
    let (answer, receiver) = channel();
    let Some(mtm) = MainThreadMarker::new() else {
        let _ = answer.send(GeoOutcome::Unavailable);
        return receiver;
    };
    // SAFETY: a class property read.
    if !has_usage_description() || !unsafe { CLLocationManager::locationServicesEnabled_class() } {
        let _ = answer.send(GeoOutcome::Unavailable);
        return receiver;
    }
    let delegate = Delegate::new(mtm, answer);
    // SAFETY: made on the main thread, whose run loop delivers the
    // delegate calls; the delegate is kept alive with it in `RUNNING`.
    let manager = unsafe { CLLocationManager::new() };
    // SAFETY: setters on the manager just made, on its thread.
    unsafe {
        manager.setDesiredAccuracy(kCLLocationAccuracyThreeKilometers);
        manager.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    }
    delegate.authorization_changed(&manager);
    RUNNING.with(|running| {
        if let Some(old) = running.borrow_mut().replace(Running {
            manager,
            _delegate: delegate,
        }) {
            // SAFETY: detach and stop the replaced lookup on its thread.
            unsafe {
                old.manager.setDelegate(None);
                old.manager.stopUpdatingLocation();
            }
        }
    });
    receiver
}

/// `CLGeocoder` is deprecated for MapKit's `MKReverseGeocodingRequest`,
/// which needs macOS 26; Quill runs on 14.
#[allow(deprecated)]
pub(super) fn reverse_geocode(point: GeoPoint) -> Option<Receiver<Option<String>>> {
    let (answer, receiver) = channel();
    // SAFETY: CLGeocoder's designated initializer.
    let geocoder = unsafe { CLGeocoder::init(CLGeocoder::alloc()) };
    // SAFETY: a CLLocation made from finite degrees.
    let location = unsafe {
        CLLocation::initWithLatitude_longitude(
            CLLocation::alloc(),
            point.latitude(),
            point.longitude(),
        )
    };
    let pending = RefCell::new(Some((answer, geocoder.clone())));
    let done = RcBlock::new(
        move |placemarks: *mut NSArray<CLPlacemark>, _: *mut NSError| {
            quill::thread_guard::ffi_boundary((), || {
                // SAFETY: CoreLocation passes a valid array or null.
                let first = unsafe { placemarks.as_ref() }.and_then(|list| list.firstObject());
                let address = first.and_then(|place| {
                    let text = |value: Option<Retained<objc2_foundation::NSString>>| {
                        value.map(|value| value.to_string())
                    };
                    // SAFETY: plain property reads on a placemark.
                    unsafe {
                        join_address(&[
                            text(place.thoroughfare()),
                            text(place.locality()),
                            text(place.country()),
                        ])
                    }
                });
                if let Some((answer, _geocoder)) = pending.borrow_mut().take() {
                    let _ = answer.send(address);
                }
            });
        },
    );
    // SAFETY: CoreLocation copies the block; the block keeps the geocoder
    // alive until it answers.
    unsafe {
        geocoder.reverseGeocodeLocation_completionHandler(&location, RcBlock::as_ptr(&done));
    }
    Some(receiver)
}

pub(super) fn system_country() -> Option<String> {
    NSLocale::currentLocale()
        .regionCode()
        .map(|code| code.to_string())
}
