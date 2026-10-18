# Installing on macOS without notarization

Quill has no paid Apple Developer ID, so `Quill.app` is ad-hoc signed and not
notarized. Opening a browser-downloaded copy shows the "Apple could not verify"
dialog whose main button moves the app to the Bin, and since macOS 15 the only
way past it is System Settings > Privacy & Security > Open Anyway.

## What macOS actually checks

- Gatekeeper assesses an app only when it carries the `com.apple.quarantine`
  extended attribute. Browsers, Mail, Messages and AirDrop add it; `curl` does
  not, and neither does an app's own HTTP client unless its Info.plist sets
  `LSFileQuarantineEnabled` (Quill's doesn't).
- On Apple Silicon every binary still needs a valid signature; an ad-hoc one
  (`codesign -s -`) is enough. A broken or missing signature fails to launch
  whether quarantined or not.
- The Control-click > Open bypass was removed in macOS 15.
- Homebrew deprecated `--no-quarantine` in 5.0 and drops casks that fail
  Gatekeeper from 2026-09-01, so a cask is no route for an unnotarized app.

## Decision

`scripts/macos-install.sh`, run as
`curl -fsSL https://raw.githubusercontent.com/shinycake/quill/main/scripts/macos-install.sh | bash`:
downloads the latest release zip with curl, checks it against the release's
`SHA256SUMS.txt`, verifies the code signature, clears any quarantine attribute, quits the
Quill it replaces if that copy is running (with `quill --quit`, never by bundle
id, so a dev build elsewhere keeps running), and replaces `Quill.app` in `/Applications` (or
`~/Applications` when `/Applications` isn't writable). The README, the release
notes and the site lead with it; the browser path keeps the `xattr` and Open
Anyway instructions.

The checksum comes from the same release as the zip, so it catches a corrupt
download, not a compromised release. That's the same trust as downloading the
zip directly. `package-macos.yml` runs the installer on each build's zip and
checks that the installed app has no quarantine attribute and a valid signature.

Quill's own HTTP downloads (ureq) aren't quarantined either, so an in-place
macOS updater won't trip Gatekeeper as long as it keeps the bundle's signature
valid and the Info.plist never sets `LSFileQuarantineEnabled`. Until that
updater ships, Mac users update by running the installer again.
