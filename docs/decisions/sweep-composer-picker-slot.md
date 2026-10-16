# Composer picker answers and logout cleanup follow the slot

Follow-up to `sweep-per-chat-composer.md` (PR #899 review notes).

## What tdesktop does

Its file dialog (`FileDialog::GetOpenPaths`) is modal and `SendFilesBox` is
closed when another chat is shown (`MainWidget::showPeerHistory` calls
`hideSettingsAndLayer`), so an answer can never reach a different chat. On
Linux the xdg portal picker is not modal in Quill, which is the case this
covers.

## Before

- The attach menu's picker answered into whatever chat its window showed by
  then, so on a non-modal picker a chat or account switch put the files in the
  wrong tray.
- A session that ended without `LoggingOut` being seen (expired or terminated
  session, TDLib closing the client) kept its composer entries until the
  account was removed or the app quit.

## After

- `pick_attachments` captures the `SendSlot` when the picker opens.
  `attach_picked_in` takes the ordinary path when that slot is on screen; in
  another slot it writes through `composer_sends.update(&slot, ..)` and leaves
  the open chat alone. If the slot's account is not on screen or signed in in
  the background, or its chat is gone from the on-screen account's list
  (`send_slot_exists`), the answer is dropped.
- The video sound probes and the file thumbnails (`check_tray_videos`,
  `prepare_tray_file_thumbnails`) now scan every tray instead of the open one
  (`ComposerSends::attachments`). Their results were already keyed by path and
  written to every tray holding the file, so no per-slot bookkeeping was
  needed.
- `forget_sends_after_auth` forgets the on-screen account's entries when its
  auth state goes Ready/LoggingOut/Closing to WaitPhoneNumber/Closed (`session_ended`),
  or when `LoggingOut` was latched as before. It derives from the transition
  of `session.auth`; no new flag.
- Not hooked into `restart_live_connection`: its callers are the logout
  restart (already covered) and the Retry button after a lost connection, which
  resumes the same account; dropping the user's tray on Retry would lose
  unsent work for nothing.
- Send gating (`tray_videos_checking`, `tray_thumbnails_pending`) looks only at
  the paths in the open slot's tray, so a probe for another chat's file never
  blocks a send here.
- An off-screen picker answer that hits a limit shows no note: it would name
  nothing and land in the chat on screen.

## Verification

GPUI tests in `src/ui/composer_sends_tests.rs`:
- `a_file_picker_answered_after_a_chat_switch_fills_the_chat_it_was_opened_in`
  (open in A, switch to B, answer: A has the file, B is empty, back in A it
  shows)
- `a_file_picker_answer_for_a_gone_account_or_chat_is_dropped`
- `a_pending_check_for_another_chats_file_does_not_block_a_send_here`
- `a_session_closing_over_two_polls_ends_like_one_that_closed_at_once`
- `the_choices_go_when_the_session_leaves_ready_without_a_logout` (Ready to
  Ready and a never-Ready prompt keep entries; Ready to WaitPhoneNumber and
  Ready to Closed drop them)

No capture: the picker is a native dialog that the demo cannot drive; the tray
rendering itself is unchanged and the GPUI test asserts its contents.
