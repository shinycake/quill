# Send Location: the map picker

## What tdesktop does

`Telegram/SourceFiles/ui/controls/location_picker.cpp` (`Ui::LocationPicker`)
opens a 364 by 680 window from the attach menu's Location:

- a Mapbox map in a web view, 220 tall, collapsing to 92 as the list scrolls,
  with a fixed marker in the middle that lifts while the map moves;
- "Send This Location" with the pin's address under it, resolved 3 seconds
  after the map stops (`kResolveAddressDelay`), or the coordinates when no
  address is known;
- "Or choose a venue": places from the venue search bot (the
  `venue_search_bot_username` option, an inline bot asked with the user's
  location), each row a 42px coloured circle with the Foursquare category
  icon, the name and the address, ending with "Powered by Foursquare";
- "Places in this area" over the map once it rests somewhere new;
- a title-bar search over the same bot, debounced by 900 ms
  (`kSearchDebounceDelay`);
- the system's current location on open (`ResolveCurrentExactLocation`),
  falling back to the country (`ResolveCurrentCountryLocation`).

A venue sends `inputMessageVenue`; "Send This Location" sends
`inputMessageLocation`.

## What Quill had

A typed panel: two text fields for latitude and longitude above the composer.
It is gone, with its strings and `parse_coordinates`.

## What changed

- `src/location_picker/`: the picker as data. `LocationSharing` holds the venue
  bot, the last exact fix, venue icons and the open `LocationPicker` (view,
  moving, address, venue search with its cache and generations, map images).
  It says what to send (`next_requests`) and takes the answers. Tests in
  `picker_tests.rs` and `sharing_tests.rs`.
- `src/connect/location_picker.rs`: the driver sends those requests
  (`searchPublicChat`, `getInlineQueryResults`, `getMapThumbnailFile`,
  `getRemoteFile`) and downloads the files. Driver tests use the recording
  sender (`src/connect/tests/location_picker.rs`).
- `src/ui/location_picker/`: the window (`dialog.rs`), the map (`map.rs`), the
  timers and sends (`mod.rs`), platform location (`geolocation/`), the
  `ready-location-picker` demo and the GPUI tests (`tests.rs`).
- The attach menu's Location opens the picker.

## Deviations

- **The map is native, not a web view.** Quill has no web view in the main
  window, so the map is TDLib's static map image (`getMapThumbnailFile`) for
  the view, drawn with the pin on top. Dragging pans, the buttons, the wheel,
  pinch and the arrow and +/- keys zoom. While the map moves the image already
  held is shifted and scaled to the new view; half a second after the map
  stops, the image for the new view is asked for and replaces it. The edges
  uncovered by a pan stay plain until then.
- **Zoom is 13 to 20 in whole steps.** That is what `getMapThumbnailFile`
  accepts. A map that starts at the country centre starts at zoom 13, closer
  than tdesktop's country view.
- **Zoom and "Show my location" buttons** sit on the map. tdesktop has neither
  (Mapbox's own gestures cover zoom); a desktop map needs a way to zoom
  without a trackpad, and "Show my location" is the only way back after
  dragging away.
- **Addresses.** macOS names the pin's place with `CLGeocoder` (street, city,
  country). Windows and Linux have no keyless geocoder Quill can call, so the
  line shows the coordinates to six decimals, as tdesktop does when it has no
  address. `CLGeocoder` is deprecated in favour of MapKit's
  `MKReverseGeocodingRequest`, which needs macOS 26; Quill supports 14.
- **Denied and unavailable location** are said in the list ("Quill isn't
  allowed to use your location…", "Your location isn't available right
  now…") above tdesktop's "Choose location to see places nearby". tdesktop
  only shows the latter.
- **No venue bot** (the option is empty or the name is not an inline geo bot)
  shows "No places found" instead of loading forever.
- **A missing category icon** shows a map pin in the coloured circle. The
  circle's hue comes from the category, like tdesktop's.

## Platform location

- macOS: `CLLocationManager` on the main thread, "when in use" permission,
  three-kilometre accuracy, one fix. Quill.app's Info.plist carries
  `NSLocationUsageDescription` and `NSLocationWhenInUseUsageDescription`
  (`scripts/macos-package-smoke.sh`). A bare binary without them reads as
  unavailable and never prompts.
- Windows: `Windows.Devices.Geolocation.Geolocator` on a worker thread;
  "Let apps access your location" off reads as denied.
- Linux: GeoClue2 over D-Bus (`zbus`), desktop id `org.shinycake.quill`,
  20-second timeout; `AccessDenied` reads as denied.
- The country fallback is the system region (macOS `NSLocale.regionCode`,
  Windows `GetUserDefaultGeoName`, Linux the locale's region), then the
  login's guessed country.

No platform call runs on the UI thread's critical path: answers come over a
channel polled every 100 ms by a task, with a timeout.

## Verified

- Core, driver and GPUI tests (open, zoom by button and wheel with a new image
  after settling, drag with "Places in this area" and the address following
  the pin, search narrowing and "No places found", venue pick, Escape, denied).
- Screenshots of the `ready-location-picker` demo in light and dark: the
  picker, after arrow-key panning ("Places in this area", address loading),
  search results, no results, and denied.

## Not verified

- Real TDLib map images, a real venue bot and real Foursquare icons: the demo
  uses one fixture image for every view and has no icons.
- The location permission prompt and real fixes on macOS, Windows and Linux,
  and `CLGeocoder` addresses. None of the platform code ran against a real
  location service.
- Pinch zoom on a real trackpad.
