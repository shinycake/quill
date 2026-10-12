# Advanced bundle: experimental page, beta channel, debug and test-server codes

## What tdesktop does

- `settings_experimental.cpp`: a page (Settings > Advanced > Experimental
  settings) with a warning, "Restore default values" while anything differs,
  `base::options` toggles by category, a search field, and Export / Import of
  a `tdesktop-flags:` code from the top bar menu.
- `settings_advanced.cpp`: "Install beta versions" sits with the update
  options (shown while automatic updates are on and nothing downloads);
  switching it re-checks at once.
- `settings_codes.cpp`: typed codes. `debugmode` asks "Do you want to
  enable/disable DEBUG logs?" (`withdebug` flag file; disabling restarts),
  `viewlogs` shows `log.txt`, `testmode` (one signed-out account only) adds
  and activates an account in the other environment, the intro shows "Test
  Mode", `numberbuttons` toggles the fast-buttons option.

## What changed

- `src/advanced/`: `codes` (same buffer algorithm), `debug_mode`,
  `test_environment`, `experimental`, `update_channel`.
- Settings > Advanced page with "Experimental settings". The page has the
  warning, restore, Profile category ("Split profile media tab", moved out of
  Appearance), an "Other" category with Fast buttons mode only while on, and
  Export / Import (`quill-flags:`).
- Fast buttons mode is now gated by that option like tdesktop (per-bot
  switch, number keys and badges exist only while it is on). It was always
  available before.
- Install beta versions: switch in the update section, same visibility rule,
  re-checks on change. `updater::interpret_release_list` takes the newest
  non-draft release of any kind; stable keeps `/releases/latest`.
  `release.yml` got a `prerelease` input (draft flagged as pre-release,
  title "(beta)"); the site and `latest` ignore pre-releases.
- Debug mode: `withdebug` file, `--debug`, `logs/quill-debug.log`, TDLib's
  native log sent to `logs/tdlib.log` at verbosity 4 through `setLogStream`
  and `setLogVerbosityLevel` (otherwise the stream stays empty). Confirm
  dialog with tdesktop's text.
- Test server: a marker file in the account directory makes
  `setTdlibParameters` use `use_test_dc`; `testmode` adds such an account
  under the same single-signed-out-account rule; sign-in shows "Test Mode".

## Differences from tdesktop

- Keys are read on the sign-in screen itself and on the Settings list; tdesktop
  reads them on its logged-out settings page, which Quill does not have.
- Turning debug logs off does not restart; TDLib's file stream is closed in
  place.
- The experimental page has no search field and lists only the options Quill
  has behavior for (the rest of tdesktop's list has no counterpart). Not
  mirrored although Quill has the behavior unconditionally: "View Profile" in
  the chat list menu is always shown.
- Export/Import format is Quill's own prefix. tdesktop's `base::options`
  source is not in the local reference, so import semantics (missing ids
  reset) are assumed.

## Verified

Unit tests (codes, flag file, TDLib request shapes, release list, export
round trip, test-account marker, `use_test_dc`), UI tests for codes on the
Settings list, `numberbuttons`, the experimental page and the beta row, and
an ignored test against the real libtdjson 1.8.68 (file stream writes, closes).
Gate OK.

## Unfinished / unverified

- `testmode` on the real sign-in screen and the "Test Mode" label: no UI test
  and no visual capture; never connected to the test server.
- No screenshot of the Advanced / Experimental pages was taken.
- `release.yml` prerelease input was not run.
- `parity:auth-debug-toggles` is therefore not in the fragment.
