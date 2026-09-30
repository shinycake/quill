## Slice parity/msg-richtext-premium-gate (2026-09-30)

**Scope:** `parity:msg-richtext-premium-gate`. Schema: `premiumFeatureRichMessages` = "The ability to send rich messages" (td_api.tl:8160).

- **Built:**
  - Envelope (`telegram/envelope/users.rs`): `ParsedUser` now parses `user.is_premium` (schema line 2403).
  - State (`state/session_members.rs`): `Session::my_is_premium()` — looks up `my_user_id` in `users`; false until our own user object arrives.
  - UI (`ui/composer.rs`): the "⛶ Rich editor" button still shows for everyone (visible signal), but tapping it as a non-Premium user sets a status note "Rich messages require Telegram Premium" instead of opening the editor. `submit_rich_composer` refuses non-Premium sends as defense in depth (the server would reject `inputMessageRichMessage` anyway — we never fake a successful send).
- **Key decisions (ponytail):** no new premium-status request — `user.is_premium` rides on the user objects already flowing through `updateUser`/`getUser`; the demo bypasses the gate by setting `rich_editor_open` directly (screenshot demos unaffected). Kept the button visible (rather than hiding it) so non-Premium users learn *why* it's unavailable, matching how the button already narrates state via status notes.
- **Tests:** `update_user_parses_is_premium` (envelope); `my_is_premium_follows_own_user_record` (state).
- **UI proof:** `docs/screenshots/ready-rich-premium-gate.png` — real GPUI capture under Xvfb + lavapipe, showing the multi-line composer, visible Rich editor button, closed editor, and Premium requirement status. Reproduce with `cargo run --features ui -- --screenshot-demo ready-rich-premium-gate docs/screenshots`; the fixture sets the refusal state directly, without a live Telegram session.
- **Out of this slice:** none identified.
