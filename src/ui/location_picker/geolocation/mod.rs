//! Where the user is, from the platform's location services (tdesktop
//! `Core::ResolveCurrentExactLocation`, `current_geo_location_*`), the
//! address of a point where the platform has a keyless geocoder, and the
//! system country the map starts in without a fix.
//!
//! Each lookup answers once on a channel; the picker polls it from a UI
//! task, so nothing here blocks the UI thread.
#![deny(clippy::indexing_slicing)]

use quill::location_picker::{GeoOutcome, GeoPoint};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Receiver;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(windows)]
use windows as platform;

/// A running "where am I" lookup: its answer, and whether the system's
/// permission prompt is open (the user's turn, not the lookup's: its time
/// limit does not run meanwhile).
pub(in crate::ui) struct Locating {
    pub(in crate::ui) answer: Receiver<GeoOutcome>,
    pub(in crate::ui) permission_prompt: Arc<AtomicBool>,
}

impl Drop for Locating {
    /// Dropping the lookup (the picker closed, or it timed out) stops the
    /// platform's location manager instead of leaving it running.
    fn drop(&mut self) {
        platform::cancel(&self.permission_prompt);
    }
}

/// One "where am I" lookup. Call on the UI thread (CoreLocation answers on
/// the run loop of the thread that asked).
pub(in crate::ui) fn locate() -> Locating {
    let permission_prompt = Arc::new(AtomicBool::new(false));
    let answer = platform::locate(&permission_prompt);
    Locating {
        answer,
        permission_prompt,
    }
}

/// The address of `point`, where the platform geocodes without a key
/// (CoreLocation's `CLGeocoder`); `None` elsewhere, and the picker shows the
/// coordinates, as tdesktop does without its Mapbox token.
pub(in crate::ui) fn reverse_geocode(point: GeoPoint) -> Option<Receiver<Option<String>>> {
    platform::reverse_geocode(point)
}

/// tdesktop `Platform::SystemCountry`: the ISO 3166 alpha-2 code of the
/// system region, upper-cased.
pub(in crate::ui) fn system_country() -> Option<String> {
    platform::system_country()
        .map(|code| code.trim().to_ascii_uppercase())
        .filter(|code| code.len() == 2 && code.chars().all(|c| c.is_ascii_alphabetic()))
}

/// The region of a POSIX locale name such as `en_US.UTF-8` or `pt_BR@euro`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn locale_region(locale: &str) -> Option<String> {
    let name = locale.split(['.', '@']).next()?;
    let (_, region) = name.split_once(['_', '-'])?;
    Some(region.to_string())
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn join_address(parts: &[Option<String>]) -> Option<String> {
    let joined = parts
        .iter()
        .flatten()
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    (!joined.is_empty()).then_some(joined)
}

#[cfg(test)]
mod tests {
    use super::{join_address, locale_region};

    #[test]
    fn locale_names_give_their_region() {
        assert_eq!(locale_region("en_US.UTF-8").as_deref(), Some("US"));
        assert_eq!(locale_region("pt_BR@euro").as_deref(), Some("BR"));
        assert_eq!(locale_region("de-AT").as_deref(), Some("AT"));
        assert_eq!(locale_region("C"), None);
    }

    #[test]
    fn addresses_skip_missing_parts() {
        let part = |s: &str| Some(s.to_string());
        assert_eq!(
            join_address(&[part("Rue de Rivoli"), None, part("France")]).as_deref(),
            Some("Rue de Rivoli, France")
        );
        assert_eq!(join_address(&[None, part(" ")]), None);
    }
}
