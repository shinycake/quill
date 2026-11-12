# Re-review of r3-10 second fix round (Opus, 2026-11-12). Verdict: merge

Line numbers are on origin/claude/sweep-r3-10.

Second fix round: c424d9e12, 29d479c8e. Status of the bullets in reviews/r3-10-round2.md:

- 18-003 (Windows build): resolved. `windows-future = "0.3"` is under `[target.'cfg(windows)'.dependencies]` (Cargo.toml:357), and Cargo.lock resolves it to 0.3.2. That is the same copy `windows` 0.62.2 links, and both use windows-core 0.62.2. src/ui/location_picker/geolocation/windows.rs:16 imports `windows_future::IAsyncOperation`, and :43 calls `operation.Cancel()`. I checked every path the file uses against the registry sources:
  - windows-future 0.3.2: `IAsyncOperation<T>` is `#[derive(Clone)]` (bindings.rs:1636). `Cancel` is an inherent method (bindings.rs:1738, which casts to `IAsyncInfo` itself). `join` is in join.rs:12. `IntoFuture` with `Output = windows_core::Result<T>` is in future.rs:94, behind the default `std` feature, which the direct dependency enables. The crate is edition 2024, so `into_future` needs no import.
  - windows 0.62.2, `Devices_Geolocation`, which turns on `Devices` and `Foundation`: `Geolocator::new` (:472), `RequestAccessAsync` (:565), `SetDesiredAccuracy` (:486), `LocationStatus` (:512) and `GetGeopositionAsync` (:519) all exist, and the two async calls return `windows_future::IAsyncOperation`. So do `Geoposition::Coordinate` (:788), `Geocoordinate::Point` (:352), `Geopoint::Position` (:720), `BasicGeoposition { Latitude, Longitude }` (:23-25), `GeolocationAccessStatus::Denied`, `PositionStatus::{Disabled, NotAvailable}` and `PositionAccuracy::High`. None of these is behind any other cfg feature.
  - The removed `windows::core::Interface` import has no remaining use.
- 19-006 (card form delete snapshot): resolved. `DeleteEdit::Snapshot { before, press }` (src/ui/payments/card_form.rs:88) and `CardInput::presses` (:40, `next_press` :64) replace the task and the oneshot. The clear runs inside the second nested defer, in the same effect cycle (:440-446). `expire_delete` (:71-75) clears only the press that took the snapshot. Nothing outlives the press. Test: src/ui/payments/card_form_tests.rs:496.

The new commits add no unwrap, expect, indexing, RefCell borrows, allow-list lines or baseline lines. The guard scripts pass on the tip. check-bundle-dlopen.py needs a package directory, so it does not apply to a source tree.

Notes: no Windows CI has run on this branch yet, so the PR's Windows job is the first real compile. I checked the paths against the crate sources and expect it to pass. The new test calls `expire_delete` directly, so it covers the press match but not the defer ordering. The ordering rests on GPUI's nested-defer semantics, which the round-2 review accepted.
