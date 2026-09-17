# Decisions (Phase 0 / early Phase 1)

Research snapshot 2026-09-16, pin recheck **2026-09-17**.

## Product

- Fresh MIT repository **Quill**. Ideas-only from ZapFast / Paper Plane / Mezon / Coop. Not a fork.
- **Repo visibility: Public.**
- **Primary runners: Idan's personal Mac + Linux** (supported build/run targets, not deferred). GHA remains best-effort when billing allows; Linux CI runs unit/replay tests; `macos-latest` can compile the GPUI binary and assemble a dummy `.app` when jobs start.
- **No App Store / notarization / distribution pipeline.** Ad-hoc Apple Developer signing only if needed for Idan's personal Mac.
- **Credentials:** `TELEGRAM_API_ID` / `TELEGRAM_API_HASH` (or gitignored local `.env` / `quill.local.env`) load into the app. When credentials are present **and** `tdjson` is available (`QUILL_TDJSON_PATH` or bundled next to the executable), Quill opens `LiveTdJson`, sends `setTdlibParameters`, and drives auth updates to `WaitPhoneNumber`. Missing tdjson shows an honest install/build halt. Still no secrets in git. See `docs/credentials.md`.
- One account, cloud chats only. Channels/bots gated until sponsored-content handling exists. No secret chats, calls, telemetry, or AI.
- Storage: TDLib DB + small prefs. No second message database.

## UI family

- Toolchain pin: **Rust 1.98.1** (2026-09-03 stable). GPUI Kit 0.6.1 / gpui-pre 0.3.5 require at least 1.92 (`oo7`).
- Hello-world follows Kit docs: `gpui_kit::application()` + `gpui_kit::init` + `Root`.
- Default Kit features kept to `component` + `assets`. No tree-sitter / inspector / editor language packs.
- Synthetic chat uses Kit `MessageScroller` (auto-measured mixed heights, prepend, `remeasure_items`) and `TextareaState::submit_on_enter(true)`.
- Composer send policy is tested without GPU: IME composition and Shift/secondary Enter do not send.
- Kit `InputEvent::PressEnter` (**gpui-base 0.6.1**) has `{ secondary, shift }` only — no composing flag. `InputBaseState::enter` always emits `PressEnter` and does **not** consult `ime_marked_range` (Escape does). Quill reads `EntityInputHandler::marked_text_range` at PressEnter time via `enter_event_from_kit`. Do not hardcode `composing: false`.
- **VoiceOver** is a manual macOS follow-up. This agent has no GUI/VoiceOver runner on Linux. Do not claim the Phase 0 a11y gate until a Mac session records it.
- **Screenshots:** real GPUI window on Linux xvfb + lavapipe, `docs/screenshots/synthetic-chat.png` (plus composer and unsupported-auth shots), plus connect surfaces `connect-need-tdjson.png` / `connect-wait-phone.png` via `quill --screenshot-demo` + `scripts/capture-connect-screenshots.sh`. The stray “X” in an early capture was the X11 cursor, not a jump button. VoiceOver remains a macOS follow-up.

## TDLib

- Runtime + schema: official TDLib **1.8.67**, commit `d1085f9cebc5a62379991ae1652673954f229c1f`.
- Vendored `schema/td_api.tl` is the official file: **1,152,505 bytes**, SHA-256 `326b65b41442901ad6bf0ca2f7c356ae54365d6c343956a62e06a8b3cb305e87`. Tests fetch that commit’s `td/generate/scheme/td_api.tl` from GitHub and require **byte equality** (a 1,000,001-byte truncated copy with stripped `vector<T>` parameters is invalid). Refresh with `scripts/vendor-td-schema.sh`.
- **Rejected** as-is `tdlib-rs`: schema targets 1.8.61, receive splits responses vs updates, unknown variants log raw JSON.
- **Chosen path:** bind official `tdjson` C JSON API (`td_create_client_id` / `td_send` / `td_receive`) with one receive thread, copy-before-next-call, monotonic sequence, single reducer. Typed coverage for Phase 0/1 constructors; unknown variants keep only `@type`.
- `@extra` is a decimal **string** (no float IDs). `int64` (chat order) parsed from JSON strings. `sendMessage` uses schema `topic_id`.
- Native log callback counts only; it does not read or forward the C string. No TDLib calls from that callback.
- Ordinary builds do not download tdjson. Optional source build: `scripts/build-tdlib.sh`. Loader never searches Homebrew.

## Phase 1 (no secrets)

- Account-scoped directories under the app data dir.
- Database key: 32 random bytes in Keychain on macOS (`org.shinycake.quill` / `db-key:{account}`). **Linux live path:** `FileSecretStore` — `{app_data}/accounts/{account}/db-encryption.key`, mode `0600`, zeroize after read into `DatabaseKey`. `MemorySecretStore` is **tests only** (not `bootstrap_connect` on Linux). `KeychainSecretStore::get` maps `errSecItemNotFound` to missing (`Ok(None)`) and user-cancel / auth-failed / interaction-not-allowed / keychain-unavailable to `Locked`. Missing key + existing DB → halt, never mint a replacement.
- Auth view is a pure function of `updateAuthorizationState`. Premium / email / registration / unknown → unsupported halt UI. No payments, auto-register, or password reset.
- Chat list / history / send reducers with replay fixtures. Logout invalidates pending requests. Close ≠ logOut.
- Credentials load from owner `TELEGRAM_API_ID` / `TELEGRAM_API_HASH` (or local gitignored env files). Connect path: `evaluate_gate` → `prepare_connect` (paths + DB key) → `LiveTdJson` + `setTdlibParameters` → session auth reducers. Phone submit sends `setAuthenticationPhoneNumber` (code/2FA still manual). Never pasted into this repo.


## Linux DB key persistence (2026-09-17)

- **Problem:** `bootstrap_connect` used process-local `MemorySecretStore` on Linux. First live run minted a TDLib DB encryption key, then lost it on quit → `MissingKeyAgainstExistingDb` on the next launch.
- **Decision:** `FileSecretStore` under the account-scoped app data dir (`directories` ProjectDirs / `Quill`), file `db-encryption.key`, `0600`, atomic write (temp + rename), zeroize the read buffer after constructing `DatabaseKey`. Wired as the Linux live-connect store; macOS stays Keychain.
- **Tests:** round-trip across a new store instance (simulates restart); `MissingAgainstExistingDb` when the key file is absent but `tdlib/` already exists; mode `0600` asserted on Unix.

## Licenses

- Quill: MIT (existing LICENSE).
- GPUI Kit / GPUI: Apache-2.0.
- TDLib: Boost Software License 1.0 (not compiled into the default artifact yet).
- No GPL sources were copied.

## Measured prototype notes

- GPUI Kit 0.6.1 hello-world (`application` + `init` + `Root`) compiles on this Linux agent after `libfontconfig1-dev` and Vulkan lavapipe. Linux CI still runs **core only** (`--no-default-features`) so GitHub Ubuntu does not need a GPU.
- `MessageScroller` mixed-height prepend/remeasure works in the live window. Default tail-follow hides the first row when content is taller than the pane; the prototype scrolls to item 0 so the short row is visible in screenshots.
- Kit `TextareaState::submit_on_enter(true)` plus `should_send_on_enter` / `enter_event_from_kit(marked_text_range)` is the composer policy. Kit does not put composing on `PressEnter`; we use the IME mark.
- Official tdjson is **not** compiled here. Connect is proven with injected JSON (`ConnectDriver` + `RecordingSender`): parameter send shape, credential/tdjson gate, WaitPhoneNumber transition. Live `ReceiveBridge::spawn_live` is wired for machines with tdjson. Native bundle steps are documented for Apple Silicon.

## Blockers / follow-up

1. Live TDLib connect is implemented (`src/connect.rs`): credentials + tdjson → `setTdlibParameters` → `WaitPhoneNumber`. Still manual: verification code / 2FA UI submit, and building/bundling tdjson on each machine (`docs/native-bundle.md`). No secrets in git.
2. VoiceOver + real IME on a Mac (this environment cannot prove them). Primary Mac runner is Idan's personal machine.
3. Native tdjson build + rpath verification on Apple Silicon (`docs/native-bundle.md`). This Linux agent has no tdjson; UI shows the MissingTdjson halt when credentials are loaded.
4. **No App Store / notarization / distribution pipeline.** Ad-hoc Apple Developer signing only if needed for Idan's personal Mac. Public repo; brand polish is still a follow-up.
5. **GitHub Actions did not run** on 2026-09-17: billing/spending limit. Re-run after billing is fixed. Local gates: `cargo fmt`, `clippy -D warnings`, `cargo test --no-default-features --locked`.
