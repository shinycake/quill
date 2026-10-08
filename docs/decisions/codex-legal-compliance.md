# Legal and license compliance pass (2026-10-08)

This records a check of Quill against Telegram's API terms and the licenses of
everything Quill ships, what changed, and what still needs the owner's
decision. It is an engineering review with sources, not legal advice. Where a
statement is an inference rather than something a source says, it is marked
as such.

Sources read on 2026-10-08:

- Telegram API Terms of Service: https://core.telegram.org/api/terms (no date
  on the page)
- Obtaining api_id: https://core.telegram.org/api/obtaining_api_id
- Telegram Terms of Service: https://telegram.org/tos (no brand or logo
  guidelines on the page; it links the API terms above)
- MTProto security guidelines: https://core.telegram.org/mtproto/security_guidelines
- Microsoft, "Redistribute Visual C++ Files":
  https://learn.microsoft.com/cpp/windows/redistributing-visual-cpp-files
- FFmpeg license and legal checklist: https://ffmpeg.org/legal.html
- Upstream license files cited per component below.

## 1. Disclaimer

- `README.md`: a warning block under the intro says Quill is unofficial and
  not affiliated with, endorsed by, or sponsored by Telegram, that "Telegram"
  is a trademark of its owner used only to name the service, that Quill is
  early and experimental, and that there is no warranty. It points to the MIT
  license's "AS IS" clause (`LICENSE`) instead of adding new legal terms, and
  recommends keeping an official app installed and backing up data.
- In the app: Settings → Appearance now ends with an "About Quill" section
  (`src/ui/updates.rs` `about_settings_section`, text in
  `src/about.rs::DISCLAIMER`): version, the same short disclaimer, and an
  "Open-source licenses" button. There was no About dialog before; the update
  checker already lived in that dialog, so the section sits next to it.
- Packages: `scripts/linux-package-README.txt` and
  `scripts/windows-package-README.txt` carry the short disclaimer.
- README claims that had no evidence were removed or reworded as goals:
  "built to be faster and more delightful than the official clients", "the
  client Telegram should have: instant, reliable, and a little bit delightful",
  "Complete auth" (two sign-in items are still unchecked), "Full messaging",
  and "1:1 and group audio/video with screen sharing" (live group video and
  group screen sharing are unchecked in the Calls checklist). No speed
  measurements exist, and the README now says so.

## 2. Telegram API terms

| Term (https://core.telegram.org/api/terms) | Status | Evidence |
|---|---|---|
| 1.1 Protect user privacy; follow the security guidelines | Compliant as far as checked | Quill does not implement MTProto: every Telegram request goes through TDLib (`src/telegram/ffi.rs`, `src/telegram/client.rs`). The security guidelines are protocol-level checks (DH parameters, msg_key, msg_id); that TDLib performs them is an inference, the page does not mention TDLib. Local data: TDLib's database is encrypted with a per-account key kept in the macOS Keychain, a DPAPI-sealed file on Windows, or a mode-0600 file on Linux (`src/platform.rs`). The only non-Telegram network request is the update check to `api.github.com/repos/shinycake/quill/releases/latest` (`src/updater.rs`), which the user can turn off. No analytics or crash reporting exist (searched `src/` for telemetry, analytics, sentry). Diagnostics never record message text, phone numbers, codes or the api_hash (`docs/credentials.md`, `src/credentials.rs` redacts the hash in `Debug`). |
| 1.3 Basic features work; no "download my app to see this" | Compliant | Quill sends ordinary Telegram messages through TDLib. |
| 1.4 No acting without consent, no keeping self-destructing content, no ghost mode, no blocking typing or read statuses | Compliant after this PR | No ghost-mode, hide-typing or read-receipt options exist (searched). Read status: `viewMessages` on open (`src/connect/composer.rs`); typing: `sendChatAction` (`src/telegram/requests/chats.rs`). Account export already skipped protected and self-destructing content (`src/account_export.rs`). **Changed:** the per-chat "Export chat history" (`src/chat_export.rs`) kept the caption of self-destructing messages and ran on chats with protected content. It now exports a placeholder row for self-destructing messages, refuses protected chats in `ConnectDriver::start_chat_export`, and hides the menu item for them (`src/ui/navigation.rs`), as Telegram Desktop does (`PeerData::canExportChatHistory` requires `allowsForwarding()`). Tests: `chat_export::tests::projection_drops_self_destructing_content`, `connect::tests::chat_state::chat_export_refuses_protected_chats`. |
| 1.5 No AI training on Telegram data | Compliant | Quill trains nothing. The composer's AI tools call Telegram's own server functions (`composeTextWithAi`, `fixTextWithAi`, ...), see the `parity:msg-richtext-ai-tools` item. |
| 2.1 Own api_id | Compliant in the repo; **owner decision for releases** | No api_id or api_hash is committed or embedded. `src/credentials.rs` reads only the environment (`TELEGRAM_API_ID`/`TELEGRAM_API_HASH`, aliases `QUILL_*`) or gitignored `.env`/`quill.local.env` files (`.gitignore`); `.gitleaks.toml` scanning runs in CI. No workflow references a Telegram secret. Consequence: a packaged release cannot sign in until the user supplies their own credentials. See follow-up F2. Note: the binary also looks for those files under the compile-time `CARGO_MANIFEST_DIR`, which only matters on the build machine. |
| 2.2 Tell users the app uses the Telegram API, in store listings and the in-app intro | Compliant after this PR | **Added** a line on the sign-in card (`src/ui/onboarding.rs`, text `src/about.rs::INTRO_NOTICE`): "Quill is an unofficial app that uses the Telegram API. It is not affiliated with Telegram." There are no app-store listings. |
| 2.3 Title must not contain "Telegram" unless preceded by "Unofficial" | Compliant | App name "Quill" everywhere: `CFBundleName` (`scripts/macos-package-smoke.sh`), `Name=Quill` (`assets/quill.desktop`), `set_app_identity("org.shinycake.quill", "Quill")` (`src/main.rs`), tray tooltip "Quill" (`src/tray.rs`), window titles. Other "Telegram" strings in `src/ui` name the service ("Telegram Premium", "Couldn't reach Telegram"). **Changed** to be conservative: the `.desktop` `Comment` from "Quill — Telegram desktop client" to "Unofficial Telegram desktop client", and the crate `description` in `Cargo.toml`. |
| 2.4 No official Telegram logo | Compliant | `assets/icons/` holds a "Q" with a quill feather (`docs/decisions/codex-app-icon.md`); the tray icon is drawn from `assets/icons/tray-template.svg`. No paper-plane artwork is in the repo. The decision record does not say how the raster artwork was made; see F6. |
| 3.2 List monetization methods in store descriptions | Not applicable | Quill has no monetization and no store listings. |
| 3.3 Apps that show channel content must support official sponsored messages and not interfere | **Not compliant** | Live chats call `getChatSponsoredMessages` for every channel (`src/connect/chat_list.rs`, `src/connect/search.rs`) and the reducer stores the result (`src/state/session_sponsored.rs`), but the live history never shows them: `sponsored_rows_pane` renders only for the `ReadySponsored` demo fixture (`src/ui/conversation.rs`, `self.sponsored_demo`). `viewMessages` is never sent for sponsored ids, and `clickChatSponsoredMessage` / `reportChatSponsoredMessage` are reachable only from the demo pane. The stale line in `docs/credentials.md` ("will not enable channels/bots until sponsored-content handling exists") was replaced with the real status. See F1. |
| 4 Breach: Telegram notifies the app's account; 10 days to fix | Informational | Telegram contacts the account tied to the api_id (obtaining_api_id page: "keep it current"). |

The obtaining_api_id page also says accounts using unofficial clients are "under
observation", that the sample api_id in Telegram's open-source apps must not be
used in published apps, and that publishing code is needed for GNU GPL
compliance. Quill contains no Telegram GPL code: it uses TDLib (BSL-1.0), and
its call tones are synthesized (`src/ui/call_tones.rs`) because Telegram's call
sounds are GPL.

## 3. Third-party licenses

### What changed

- `licenses/` (new) holds the texts the packages must carry: TDLib
  BSL-1.0, OpenSSL Apache-2.0, zlib, ntgcalls LGPL-3.0 plus GPL-3.0 (LGPL-3.0
  supplements GPL-3.0, so both texts ship), the components statically linked
  into ntgcalls (`ntgcalls-components.txt`), rlottie's `COPYING` and
  `licenses/*` at the pinned commit, Lucide's ISC license (from the
  gpui-kit-assets 0.7.0 crate; cargo-about only records the crate's own
  Apache-2.0), winpthreads (statically linked into the Windows FFmpeg DLLs),
  the spellcheck word-list attribution, and a Visual C++ runtime notice.
- `THIRD_PARTY_LICENSES.md` (new, generated): the license text of all 749
  crates compiled into the binary, grouped by license, made by
  `scripts/third-party-licenses.sh` (cargo-about 0.9.2, `licenses/about.toml`,
  `licenses/about.hbs`). Default features, targets aarch64-apple-darwin,
  x86_64/aarch64-unknown-linux-gnu, x86_64-pc-windows-msvc; build and dev
  dependencies excluded. It runs `--offline` after `cargo fetch` so the output
  depends only on `Cargo.lock`, so any PR that changes `Cargo.lock` must rerun
  the script (the `licenses` CI job reports it when it doesn't). Trade-off: for a crate whose package lacks its
  license file, cargo-about falls back to the canonical license text without
  the crate's copyright line.
- `THIRD_PARTY.md` rewritten as the index of everything above.
- Packaging: `scripts/stage-licenses.sh` copies `LICENSE`, `THIRD_PARTY.md`,
  `THIRD_PARTY_LICENSES.md` and `licenses/` into the Linux package and
  `Quill.app/Contents/Resources` (it replaces the old rlottie-only step, which
  depended on `vendor/rlottie/source` existing). `scripts/windows-package.ps1`
  mirrors it. Before this, the macOS app shipped no `LICENSE` or notices, and
  no package shipped the OpenSSL, TDLib, ntgcalls, Unicode or rlottie (Linux,
  Windows) texts.
- In the app, "Open-source licenses" opens the bundled `THIRD_PARTY.md`
  (beside the executable, or `Contents/Resources` on macOS, `src/about.rs`) and
  falls back to the GitHub copy for unpackaged builds.
- `deny.toml` + CI job `licenses` (not required): `cargo deny check licenses`
  against an allow-list (0BSD, Apache-2.0, Apache-2.0 WITH LLVM-exception,
  BSD-2-Clause, BSD-3-Clause, BSL-1.0, bzip2-1.0.6, CC0-1.0,
  CDLA-Permissive-2.0, ISC, MIT, MIT-0, MPL-2.0, Unicode-3.0, Unlicense, Zlib),
  and `scripts/third-party-licenses.sh --check`. Locally the deny check takes
  about 1.5 s; removing MPL-2.0 from the list makes it fail on 16 crates, so
  the check does reject unlisted licenses. Tools come prebuilt from
  `taiki-e/install-action` (cargo-deny 0.20.2, cargo-about 0.9.2).
- `third_party/gpui-base` is a modified Apache-2.0 crate. Apache-2.0 §4(b)
  requires modified files to carry a notice of the change; none did. Each of
  the 12 changed or added files now starts with one, and
  `third_party/gpui-base/QUILL-CHANGES.md` lists them.

### Inventory

Rust crates (cargo-about overview, crates per license; a crate with an AND
expression counts under each): MIT 673, Apache-2.0 41, ISC 22, Unicode-3.0 19,
MPL-2.0 16, BSD-3-Clause 11, 0BSD 4, CC0-1.0 3, Zlib 3, BSD-2-Clause 1,
CDLA-Permissive-2.0 1, Unlicense 1, bzip2-1.0.6 1. No crate is GPL, AGPL or
LGPL only, and none has an unknown license. `self_cell` (Apache-2.0 OR
GPL-2.0-only) and `r-efi` (MIT OR Apache-2.0 OR LGPL-2.1-or-later) are used
under the permissive option. MPL-2.0 crates (spellbook, symphonia-*, dwrote,
option-ext) are unmodified; MPL-2.0 §3.2 asks that recipients be told where the
source is, which the crates.io and repository links in
`THIRD_PARTY_LICENSES.md` do.

Native components, checked against the build scripts:

| Component | License | Obligations and how they are met |
|---|---|---|
| TDLib 1.8.67 + `native/patches/tdlib-quill-takeout-contacts.patch` | BSL-1.0 (`native/td/LICENSE_1_0.txt`) | BSL-1.0 does not require the notice in machine-code-only copies; it ships anyway. The patch is in the repo. |
| OpenSSL 3 (macOS: Homebrew `openssl@3`, 3.6.5 on the maintainer's Mac; Linux: distro; Windows: vcpkg) | Apache-2.0 (https://github.com/openssl/openssl/blob/openssl-3.5.0/LICENSE.txt) | §4(a) give recipients a copy of the license: now shipped. Unmodified, so no change notices. |
| zlib (Windows `z.dll`, vcpkg) | Zlib (https://github.com/madler/zlib/blob/v1.3.1/LICENSE) | No binary notice requirement; text ships anyway. |
| FFmpeg n8.1.3 (Linux, Windows) | LGPL-2.1-or-later | `scripts/build-ffmpeg.sh`: shared libraries, no `--enable-gpl`/`--enable-nonfree`, `--disable-autodetect`, decoders/demuxers/parsers only; `licenses/ffmpeg/` ships `COPYING.LGPLv2.1`, `LICENSE.md` and the source + configure line. Dynamic linking keeps it replaceable. Gap: source offer, see F3. |
| ntgcalls v3.0.0 (prebuilt, all platforms) | LGPL-3.0 (https://github.com/pytgcalls/ntgcalls/blob/v3.0.0/LICENSE) | Loaded at runtime via `dlopen`, replaceable. Statically contains WebRTC (BSD-3-Clause and its third-party code), FFmpeg 9.0.1 (LGPL; https://github.com/pytgcalls/ffmpeg `build.sh` passes no `--enable-gpl`/`--enable-nonfree`), Opus, OpenH264, Boost, and on Linux GLib (LGPL-2.1+), libffi, expat, PCRE2, X11 and Mesa libraries (`cmake/*.cmake`, `version.properties` at v3.0.0). The prebuilt zip ships no notices. Gaps: F3, F4. |
| rlottie ea06d2f | MIT plus FreeType (FTL), Pixman, Skia, stb, RapidJSON, MPL notices | All texts now in every package (`licenses/rlottie/`). |
| MinGW-w64 runtime in the Windows FFmpeg DLLs | libgcc: GCC Runtime Library Exception; winpthreads: MIT + BSD-3-Clause | winpthreads text added (from https://github.com/mirror/mingw-w64, `mingw-w64-libraries/winpthreads/COPYING`). |
| VC++ runtime (Windows, app-local) | Microsoft Visual Studio license, "Distributable Code" | Microsoft: redistribution "is limited to licensed Visual Studio users and is subject to Microsoft Software License Terms", and app-local deployment is allowed but not recommended. A notice now ships (`licenses/msvc-runtime.txt`). Gap: F5. |
| Unicode emoji data (`assets/emoji/emoji-17.tsv`, compiled in) | Unicode-3.0 | The notice must appear with the copies or in documentation; `licenses/unicode-emoji/LICENSE.txt` now ships. |
| Spellcheck word list (`assets/spellcheck/en.txt`, compiled in) | CC BY-SA 4.0 | **Problem license.** It is a filtered copy of FrequencyWords `content/2018/en/en_50k.txt` (all 48,439 words appear there, in the same order; 1,562 entries such as "etc." and "18-year-old" were dropped). FrequencyWords' README: code MIT, content CC BY-SA 4.0. There was no attribution. `licenses/spellcheck-wordlist.txt` now gives the attribution, the changes, and the license link, and states the adapted list is CC BY-SA 4.0 as ShareAlike requires. See F7. |
| Icons | Quill's own; Lucide ISC via gpui-kit-assets | See F6. |
| Call tones | Quill's own, synthesized | `src/ui/call_tones.rs`. |
| Fonts | none bundled | No font files in the repo or in gpui-kit-assets. |

## 4. Follow-ups (not done here)

- **F1. Show sponsored messages in live channels (API terms 3.3).** Render the
  fetched rows in the live history after the last message and every
  `messages_between` messages, label them Sponsored or Recommended, send
  `viewMessages` for a sponsored id once its whole text is on screen (schema
  `viewMessages` doc, `schema/td_api.tl` line 13224), and wire the existing
  click (`clickChatSponsoredMessage`) and report flows. The row and report UI
  already exist in `src/ui/sponsored.rs`. This blocks a public release.
- **F2. api_id for official builds (API terms 2.1).** Decide whether release
  packages embed the project's own api_id/api_hash (for example injected from a
  CI secret at build time) or ask users for theirs. Today a downloaded package
  cannot sign in without the user's own credentials.
- **F3. LGPL corresponding source.** FFmpeg's checklist asks distributors to
  ship the source of the exact FFmpeg they distribute
  (https://ffmpeg.org/legal.html). LGPL-2.1 §6 / LGPL-3.0 §4 allow a written
  offer or equivalent access from the same place. Recommended: the release
  workflow attaches source archives for FFmpeg n8.1.3, ntgcalls v3.0.0 and the
  LGPL parts inside ntgcalls (FFmpeg 9.0.1, GLib) to each GitHub release. Today
  the packages only point to upstream URLs. Whether a written offer is wanted
  instead is the owner's call.
- **F4. Notices for code inside ntgcalls.** WebRTC (BSD-3-Clause plus its
  bundled third-party licenses), OpenH264, Opus, Boost, PCRE2, libffi, expat,
  X11 and Mesa require their copyright notices in binary distributions.
  `licenses/ntgcalls-components.txt` lists them with licenses and sources, but
  the full notice texts are not collected yet. They could come from the
  pytgcalls/webrtc-build release archives or a WebRTC `NOTICE` file.
- **F5. VC++ runtime.** Confirm that the machine building the Windows
  package is covered by a Visual Studio license that allows redistributing the
  runtime (GitHub-hosted runners have Visual Studio preinstalled; their license
  for this use was not checked), or switch to the static CRT or to asking users
  to install the VC++ Redistributable, which Microsoft recommends.
- **F6. Icon provenance.** Record who made the app icon raster and under what
  terms in `docs/decisions/codex-app-icon.md`.
- **F7. Spellcheck word list.** Either keep the CC BY-SA 4.0 list with the
  attribution added here (the adapted list stays CC BY-SA 4.0), or replace it
  with a permissively licensed list such as SCOWL. Owner's choice.
- **F8. Privacy policy.** Quill sends no data to its maintainers. A short
  privacy statement saying so (and naming the GitHub update check) is needed
  before any app-store listing; none exists yet.
- **F9. A dedicated About entry** in the Settings list would make the
  disclaimer and licenses easier to find than the bottom of the Appearance
  dialog.
- **F10. Trademark wording.** The README names "Telegram FZ-LLC or Telegram
  Messenger Inc."; the owner may prefer to name only "Telegram".
