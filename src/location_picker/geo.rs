//! Points on the globe and how the picker compares and prints them
//! (tdesktop `Core::GeoLocation`, `Core::AreTheSame`).

/// Latitude the Web Mercator map stops at (the square world's edge).
pub const MERCATOR_MAX_LATITUDE: f64 = 85.051_128_779_806_59;

/// A latitude / longitude pair in degrees. [`GeoPoint::new`] keeps both
/// finite, the latitude inside the Mercator map and the longitude in
/// `-180..180`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeoPoint {
    latitude: f64,
    longitude: f64,
}

impl GeoPoint {
    /// `None` for a non-finite coordinate.
    pub fn new(latitude: f64, longitude: f64) -> Option<Self> {
        if !latitude.is_finite() || !longitude.is_finite() {
            return None;
        }
        Some(Self {
            latitude: latitude.clamp(-MERCATOR_MAX_LATITUDE, MERCATOR_MAX_LATITUDE),
            longitude: wrap_longitude(longitude),
        })
    }

    /// Null Island, where tdesktop's map starts when it knows nothing.
    pub const ORIGIN: Self = Self {
        latitude: 0.,
        longitude: 0.,
    };

    pub fn latitude(&self) -> f64 {
        self.latitude
    }

    pub fn longitude(&self) -> f64 {
        self.longitude
    }

    /// The same place to about 11 m (tdesktop `AreTheSame`, epsilon
    /// 0.0001 degrees, longitude compared across the antimeridian).
    pub fn same_place(&self, other: &GeoPoint) -> bool {
        const EPSILON: f64 = 0.0001;
        let lat_diff = (self.latitude - other.latitude).abs();
        let mut lon_diff = (self.longitude - other.longitude).abs();
        if lon_diff > 180. {
            lon_diff = 360. - lon_diff;
        }
        lat_diff <= EPSILON && lon_diff <= EPSILON
    }

    /// The address line tdesktop shows when no geocoder named the place:
    /// `(lat, lon)` with six decimals (`QString::arg(value, 0, 'f')`).
    pub fn coordinates_label(&self) -> String {
        format!("({:.6}, {:.6})", self.latitude, self.longitude)
    }
}

/// `Option<GeoPoint>` equality by [`GeoPoint::same_place`]; two missing
/// points are not the same place (tdesktop compares only exact locations).
pub fn same_place(a: Option<GeoPoint>, b: Option<GeoPoint>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.same_place(&b),
        _ => false,
    }
}

fn wrap_longitude(longitude: f64) -> f64 {
    let wrapped = (longitude + 180.).rem_euclid(360.) - 180.;
    // rem_euclid can round up to exactly 360 for tiny negative inputs.
    if wrapped >= 180. { -180. } else { wrapped }
}

/// Where the platform's location services put the user, or why they did
/// not (tdesktop `ResolveCurrentExactLocation`; its failure carries no
/// reason, Quill keeps one so the picker can say what happened).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GeoOutcome {
    Found(GeoPoint),
    /// The user or the system turned location access off for Quill.
    Denied,
    /// No location service, no fix, or it timed out.
    Unavailable,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(lat: f64, lon: f64) -> GeoPoint {
        GeoPoint::new(lat, lon).expect("finite")
    }

    #[test]
    fn new_rejects_non_finite_and_normalizes() {
        assert!(GeoPoint::new(f64::NAN, 0.).is_none());
        assert!(GeoPoint::new(0., f64::INFINITY).is_none());
        let p = point(89.9, 190.);
        assert!((p.latitude() - MERCATOR_MAX_LATITUDE).abs() < 1e-9);
        assert!((p.longitude() + 170.).abs() < 1e-9);
        assert_eq!(point(0., 180.).longitude(), -180.);
        assert!((point(0., -540.).longitude() + 180.).abs() < 1e-9);
    }

    #[test]
    fn same_place_uses_tdesktop_epsilon_across_the_antimeridian() {
        assert!(point(10., 20.).same_place(&point(10.00009, 20.00009)));
        assert!(!point(10., 20.).same_place(&point(10.0002, 20.)));
        assert!(point(0., 179.99995).same_place(&point(0., -179.99995)));
        assert!(!same_place(None, Some(point(0., 0.))));
        assert!(!same_place(None, None));
    }

    #[test]
    fn coordinates_label_has_six_decimals() {
        assert_eq!(
            point(37.7749, -122.4194).coordinates_label(),
            "(37.774900, -122.419400)"
        );
    }
}
