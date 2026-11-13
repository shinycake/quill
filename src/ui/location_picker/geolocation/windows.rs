//! Windows.Devices.Geolocation (tdesktop `current_geo_location_win.cpp`):
//! a `Geolocator` at high accuracy on a worker thread. "Let apps access
//! your location" turned off reads as denied; a machine without location
//! services as unavailable. Windows has no keyless geocoder Quill can use
//! (tdesktop returns no address here either).

use super::{Receiver, channel};
use quill::location_picker::{GeoOutcome, GeoPoint};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use windows::Devices::Geolocation::{
    GeolocationAccessStatus, Geolocator, Geoposition, PositionAccuracy, PositionStatus,
};
use windows_future::IAsyncOperation;

/// Wakes the lookup thread parked in [`position_unless_dropped`].
struct Unpark(std::thread::Thread);

impl Wake for Unpark {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

/// Waits for the position, or for the picker to drop its `Locating` (the
/// cancel sender), whichever comes first. On the drop the operation in
/// flight is cancelled, so the system stops locating when the picker closes.
fn position_unless_dropped(
    operation: IAsyncOperation<Geoposition>,
    cancelled: &mut futures_channel::oneshot::Receiver<()>,
) -> Option<windows::core::Result<Geoposition>> {
    let mut answer = Box::pin(operation.clone().into_future());
    let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    loop {
        if let Poll::Ready(result) = answer.as_mut().poll(&mut context) {
            return Some(result);
        }
        if Pin::new(&mut *cancelled).poll(&mut context).is_ready() {
            // The operation may already be done: Cancel then does nothing.
            let _ = operation.Cancel();
            return None;
        }
        std::thread::park();
    }
}

/// The picker dropped its `Locating` (the cancel sender): stop before the next
/// blocking call.
fn dropped(cancelled: &mut futures_channel::oneshot::Receiver<()>) -> bool {
    cancelled.try_recv().is_err()
}

fn lookup(mut cancelled: futures_channel::oneshot::Receiver<()>) -> GeoOutcome {
    let Ok(access) = Geolocator::RequestAccessAsync().and_then(|op| op.join()) else {
        return GeoOutcome::Unavailable;
    };
    if dropped(&mut cancelled) {
        return GeoOutcome::Unavailable;
    }
    if access == GeolocationAccessStatus::Denied {
        return GeoOutcome::Denied;
    }
    let Ok(locator) = Geolocator::new() else {
        return GeoOutcome::Unavailable;
    };
    let _ = locator.SetDesiredAccuracy(PositionAccuracy::High);
    match locator.LocationStatus() {
        Ok(PositionStatus::Disabled) => return GeoOutcome::Denied,
        Ok(PositionStatus::NotAvailable) | Err(_) => return GeoOutcome::Unavailable,
        Ok(_) => {}
    }
    if dropped(&mut cancelled) {
        return GeoOutcome::Unavailable;
    }
    let Ok(operation) = locator.GetGeopositionAsync() else {
        return GeoOutcome::Unavailable;
    };
    let Some(position) = position_unless_dropped(operation, &mut cancelled) else {
        return GeoOutcome::Unavailable;
    };
    let position = position
        .and_then(|position| position.Coordinate())
        .and_then(|coordinate| coordinate.Point())
        .and_then(|point| point.Position());
    match position {
        Ok(position) => GeoPoint::new(position.Latitude, position.Longitude)
            .map_or(GeoOutcome::Unavailable, GeoOutcome::Found),
        Err(_) => GeoOutcome::Unavailable,
    }
}

pub(super) fn locate(
    _permission_prompt: &std::sync::Arc<super::PromptGate>,
    cancelled: futures_channel::oneshot::Receiver<()>,
) -> Receiver<GeoOutcome> {
    let (answer, receiver) = channel();
    let fallback = answer.clone();
    let spawned = quill::thread_guard::spawn_guarded("quill-geolocation", move || {
        let _ = answer.send(lookup(cancelled));
    });
    if spawned.is_err() {
        let _ = fallback.send(GeoOutcome::Unavailable);
    }
    receiver
}

pub(super) fn reverse_geocode(_point: GeoPoint) -> Option<Receiver<Option<String>>> {
    None
}

pub(super) fn system_country() -> Option<String> {
    use windows_sys::Win32::Globalization::GetUserDefaultGeoName;
    let mut buffer = [0u16; 16];
    // SAFETY: the buffer and its length in UTF-16 units match.
    let written = unsafe { GetUserDefaultGeoName(buffer.as_mut_ptr(), buffer.len() as i32) };
    let written = usize::try_from(written).ok()?.checked_sub(1)?;
    String::from_utf16(buffer.get(..written)?).ok()
}

/// Nothing to do here: dropping the `Locating` drops the lookup's cancel
/// sender, which `lookup` checks between its calls and while the position
/// is pending (`position_unless_dropped` cancels the operation).
pub(super) fn cancel(_lookup: &std::sync::Arc<super::PromptGate>) {}
