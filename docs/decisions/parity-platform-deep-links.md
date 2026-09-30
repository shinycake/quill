## Slice parity/platform-deep-links (2026-09-30)

**Scope:** `parity:platform-deep-links` — t.me/tg: deep-link handling via getDeepLinkInfo.

- **Built:** launch with a `tg://`, `t.me` or `telegram.me` link argument →
  `getDeepLinkInfo` once auth is Ready → parse the `tg://` action from the
  `textEntityTypeTextUrl` entities → follow-up request → open the chat.
  `tg://resolve?domain=` (with `start=`/`post=`/`story=`), `tg://join?invite=`,
  `tg://openmessage?user_id=&message_id=` all handled; bot `start=` prefills
  `/start <param>` in the composer without sending. `need_update_application`
  and unknown/unparseable links show TDLib's own info text in a dialog.
  Linux `assets/quill.desktop` registers `x-scheme-handler/tg`.
- **Key decisions (ponytail):**
  - No new dependencies: `tg://` query parsing is ~30 lines hand-rolled
    (no `url` crate in the tree); percent-decoding is a 15-line helper.
  - Follow-ups reuse existing builders (`search_public_chat`,
    `create_private_chat`); only `getDeepLinkInfo` and
    `joinChatByInviteLink` builders are new.
  - Single-slot `Session::deep_link` state machine with generation guards,
    mirroring the `inline_bot_resolve` pattern (no new plumbing invented).
  - Chat open reuses the driver's `select_search_chat` (draft flush,
    recently-found, close_search) — no parallel open path.
  - Dialog uses the existing kit `Dialog` / `DialogKind` machinery
    (`DialogKind::DeepLinkInfo`), not a new dialog system.
  - UI pump is split: `poll_live` does driver-facing work (no window),
    render consumes `ChatReady` once (owns the `Window`) — same take-once
    pattern as `pending_story_open`.
- **Tests:** `tg://` param parsing, entity→action mapping, CLI arg
  detection, and one envelope-level `deepLinkInfo` JSON parse test. No
  network.
- **Out of this slice:** single-instance handoff (a second `quill <link>`
  opens a new window); `tg://proxy` links (need proxy-settings UI — info
  text is shown); Windows/macOS scheme registration (Linux .desktop only);
  auto-sending `/start`; `startgroup=` semantics (chat opens, no group
  picker); in-app updater behind `need_update_application` (queued future
  slice — the dialog text is the honest UI).
