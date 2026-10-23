//! GeoClue2 over the system bus (tdesktop `current_geo_location_linux.cpp`
//! uses libgeoclue's `gclue_simple` at exact accuracy; this speaks the same
//! D-Bus API with zbus, so no GLib is needed). A refused agent reads as
//! denied; a missing service or no fix within the timeout as unavailable.
//! Linux has no keyless geocoder Quill can use (tdesktop returns no
//! address here either).

use super::locale_region;
use futures_lite::StreamExt;
use quill::location_picker::{GeoOutcome, GeoPoint};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;
use zbus::zvariant::OwnedObjectPath;

const GEOCLUE: &str = "org.freedesktop.GeoClue2";
const MANAGER_PATH: &str = "/org/freedesktop/GeoClue2/Manager";
const MANAGER: &str = "org.freedesktop.GeoClue2.Manager";
const CLIENT: &str = "org.freedesktop.GeoClue2.Client";
const LOCATION: &str = "org.freedesktop.GeoClue2.Location";
/// The window's app id, which GeoClue's agent shows when it asks.
const DESKTOP_ID: &str = "org.shinycake.quill";
/// `GCLUE_ACCURACY_LEVEL_EXACT`.
const ACCURACY_EXACT: u32 = 8;
const FIX_TIMEOUT: Duration = Duration::from_secs(20);

fn denied(error: &zbus::Error) -> bool {
    match error {
        zbus::Error::MethodError(name, _, _) => name.as_str().ends_with("AccessDenied"),
        zbus::Error::FDO(fdo) => matches!(**fdo, zbus::fdo::Error::AccessDenied(_)),
        _ => false,
    }
}

async fn location_at(
    connection: &zbus::Connection,
    path: OwnedObjectPath,
) -> zbus::Result<Option<GeoPoint>> {
    let location = zbus::Proxy::new(connection, GEOCLUE, path, LOCATION).await?;
    let latitude: f64 = location.get_property("Latitude").await?;
    let longitude: f64 = location.get_property("Longitude").await?;
    Ok(GeoPoint::new(latitude, longitude))
}

async fn lookup() -> GeoOutcome {
    let fail = |error: zbus::Error| {
        if denied(&error) {
            GeoOutcome::Denied
        } else {
            GeoOutcome::Unavailable
        }
    };
    let Ok(connection) = zbus::Connection::system().await else {
        return GeoOutcome::Unavailable;
    };
    let manager = match zbus::Proxy::new(&connection, GEOCLUE, MANAGER_PATH, MANAGER).await {
        Ok(manager) => manager,
        Err(error) => return fail(error),
    };
    let client_path: OwnedObjectPath = match manager.call("GetClient", &()).await {
        Ok(path) => path,
        Err(error) => return fail(error),
    };
    let client = match zbus::Proxy::new(&connection, GEOCLUE, client_path.clone(), CLIENT).await {
        Ok(client) => client,
        Err(error) => return fail(error),
    };
    let setup = async {
        client.set_property("DesktopId", DESKTOP_ID).await?;
        client
            .set_property("RequestedAccuracyLevel", ACCURACY_EXACT)
            .await?;
        client.receive_signal("LocationUpdated").await
    };
    let mut updates = match setup.await {
        Ok(updates) => updates,
        Err(error) => return fail(error),
    };
    if let Err(error) = client.call_method("Start", &()).await {
        return fail(error);
    }
    let first = futures_lite::future::or(
        async {
            let message = updates.next().await?;
            let (_, new): (OwnedObjectPath, OwnedObjectPath) = message.body().deserialize().ok()?;
            location_at(&connection, new).await.ok().flatten()
        },
        async {
            async_io::Timer::after(FIX_TIMEOUT).await;
            None
        },
    )
    .await;
    let _ = client.call_method("Stop", &()).await;
    let _ = manager.call_method("DeleteClient", &(client_path,)).await;
    first.map_or(GeoOutcome::Unavailable, GeoOutcome::Found)
}

pub(super) fn locate() -> Receiver<GeoOutcome> {
    let (answer, receiver) = channel();
    let fallback = answer.clone();
    let spawned = quill::thread_guard::spawn_guarded("quill-geolocation", move || {
        let _ = answer.send(zbus::block_on(lookup()));
    });
    if spawned.is_err() {
        let _ = fallback.send(GeoOutcome::Unavailable);
    }
    receiver
}

pub(super) fn reverse_geocode(_point: GeoPoint) -> Option<Receiver<Option<String>>> {
    None
}

/// tdesktop takes the region of the system locale (Qt reads `LC_ALL`,
/// `LC_MESSAGES`, then `LANG`).
pub(super) fn system_country() -> Option<String> {
    quill::emoji::system_locale().and_then(|locale| locale_region(&locale))
}
