## Slice parity/platform-deep-links (2026-09-30)

**Scope:** `parity:platform-deep-links` — t.me/tg: deep-link handling via getDeepLinkInfo.

- **Built:** launch with a `tg://`, `t.me` or `telegram.me` link argument →
  `getDeepLinkInfo` once auth is Ready → parse the `tg://` action from the
  `textEntityTypeTextUrl` entities → follow-up request → open the chat.
  `tg://resolve?domain=` (with `start=`/`post=`/`story=`), `tg://join?invite=`,
  `tg://openmessage?user_id=&message_id=`, `tg://privatepost?channel=&post=`
  (`t.me/c/<id>/<msg>`), and `tg://user?id=` all handled; bot `start=` prefills
  `/start <param>` in the composer without sending. `need_update_application`
  and unknown/unparseable links show TDLib's own info text in a dialog.
  Linux `assets/quill.desktop` declares `x-scheme-handler/tg`; the asset is inert
  until installed and registered with the desktop.
- **Invite safety:** `JoinInvite` calls `checkChatInviteLink` with the full
  `https://t.me/+<hash>` URL. Its checked title, member count, group/channel
  type and join-request note appear in a kit confirmation dialog. Only **Join**
  sends `joinChatByInviteLink`; Cancel, Escape, backdrop and close clear the
  preview. Confirmation and answers retain generation guards. Launch never joins.
- **Message jumps:** `post=` and `message_id=` are Telegram server IDs.
  `MessageId::from_server_id` shifts them left by 20 for TDLib client IDs in
  all three jump paths (resolve, privatepost, openmessage), following TDLib's
  [`td/telegram/MessageId.h`](https://github.com/tdlib/td/blob/master/td/telegram/MessageId.h)
  (`SERVER_ID_SHIFT` and the `ServerMessageId` constructor). Story IDs are unchanged.
- **UI proof:** `docs/screenshots/ready-deep-link-info.png` and
  `docs/screenshots/ready-deep-link-invite.png` are real GPUI captures using
  injected Ready fixtures, with no live Telegram.
- **Key decisions (ponytail):**
  - No new dependencies: `tg://` query parsing is ~30 lines hand-rolled
    (no `url` crate in the tree); percent-decoding is a 15-line helper.
  - Follow-ups reuse existing builders (`search_public_chat`,
    `create_private_chat`); only `getDeepLinkInfo`,
    `checkChatInviteLink` and `joinChatByInviteLink` builders are new.
  - Single-slot `Session::deep_link` state machine with generation guards,
    mirroring the `inline_bot_resolve` pattern (no new plumbing invented).
  - Chat open reuses the driver's `select_search_chat` (draft flush,
    recently-found, close_search) — no parallel open path.
  - Dialog uses the existing kit `Dialog` / `DialogKind` machinery
    (`DialogKind::DeepLinkInfo` / `DeepLinkInvite`).
  - UI pump is split: `poll_live` does driver-facing work (no window),
    render consumes `ChatReady` once (owns the `Window`) — same take-once
    pattern as `pending_story_open`.
- **Tests:** `tg://` param parsing, entity→action mapping, CLI arg
  detection, a server-ID scale test, an envelope-level `deepLinkInfo` JSON parse test,
  and an injected driver test for invite preview, explicit confirmation,
  cancellation, duplicate confirmation and stale answers. No
  network.
- **Out of this slice:** single-instance handoff (a second `quill <link>`
  opens a new window); `tg://proxy` links (need proxy-settings UI — info
  text is shown); Windows/macOS scheme registration (Linux .desktop only);
  auto-sending `/start`; `startgroup=` semantics (chat opens, no group
  picker); in-app updater behind `need_update_application` (queued future
  slice — the dialog text is the honest UI).
