# Bot location request and .vcf file import

Two early, crude surfaces replaced in one change.

## 1. Bot keyboard "Request location"

**tdesktop** (`api/api_bot.cpp`, `ButtonType::RequestLocation`): hides a
single-use keyboard and shows an inform box with
`lng_bot_share_location_unavailable` ("location sharing is currently
unavailable in Telegram Desktop"). It never sends a location. The task
brief described a confirm-then-send flow; that is **Telegram X**
(`MessagesController.onRequestLocation`: a "Share your location" alert for
private chats, OK runs `shareCurrentLocation(oneTime)`). Quill follows Telegram
X here because tdesktop has no behavior to match, and the owner asked for the
working flow on all three platforms.

**Quill**:
- `RequestShareKind::Location { one_time }` joins the phone/users/chat request
  box (`src/ui/request_share.rs`). The box is derived from one value,
  `RequestShare::locate` (`None` = looking up, else a `GeoOutcome`):
  - looking up: "Finding your location…", Share disabled;
  - found: "Do you want to share your current location with {bot}?", Share
    enabled;
  - denied / unavailable: the reason plus Try Again, Share stays disabled.
- The lookup is the location picker's (`geolocation::locate`: CoreLocation,
  Windows.Devices.Geolocation, GeoClue), through `QuillApp::locate_then`, so
  it works on macOS, Windows and Linux and never blocks the UI thread.
- Share sends `share_location_to_chat` (`inputMessageLocation`), replying to
  the keyboard message as tdesktop's phone share does. A one-time keyboard
  hides after a successful send only (tdesktop hides it on press; Telegram X
  after sending, so a denied lookup keeps the keyboard usable).
- Only bot chats open the box (same rule as the phone button); elsewhere a
  status note says only a bot can ask.
- The button no longer carries the "not supported yet" tooltip.

## 2. Contacts "Import" box

**tdesktop** has no vCard import at all (no `.vcf` handling outside
contact cards in messages and the data export). The feature stays, as the
file flow the owner asked for.

**Quill**: the pasted-text area is gone. "Choose File…" opens the system open
dialog (`prompt_for_paths`); the file is read off the UI thread and
`parse_vcard` runs on it. The box renders from `VcardImport`
(`src/vcard_import.rs`): `Empty`, `Reading`, `Ready(preview)` or `Failed`. A
ready file shows the count, the first six contacts (name and number), "and N
more", how many cards were skipped (no phone number) or cut by the
`VCARD_IMPORT_LIMIT`; Import is enabled only when the state holds contacts, and
sends exactly those. Files over 8 MB, unreadable files and non-`.vcf` names
are reported in the box instead of failing quietly.

**Deviation**: gpui's `PathPromptOptions` has no file-type filter on any
platform, so the dialog lists every file; the extension is checked after the
choice (`.vcf` / `.vcard`) and a wrong file says so with "Choose Another
File…". Adding a filter means changing the platform crates under
`third_party/gpui-pre-*`, out of scope here.

## How it was verified

Screenshot demos with in-process capture (light and dark):
`ready-bot-location-request` (button, found / denied / unavailable via
`QUILL_DEMO_LOCATION`) and `ready-contacts-import` (empty, ready via
`QUILL_DEMO_VCF`, wrong file). GPUI UI tests cover both flows; unit tests
cover the file parsing and preview. Not exercised: the real OS location
prompt and a real open dialog (neither can run in a capture), and a live
`importContacts` / `sendMessage` round trip.
