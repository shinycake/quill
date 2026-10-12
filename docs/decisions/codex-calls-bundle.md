# Calls bundle: speaker links, and why the other three items stay open

Four call items were in this bundle. Speaker and listener links ship. The
screen source chooser, screen-share pause and conference calls are blocked by
what ntgcalls 3.0.2 and TDLib 1.8.68 expose; the exact gaps are below so the
next attempt starts from them.

## Speaker and listener links (`parity:calls-speaker-links`)

### What tdesktop does

`ShareInviteLinkAction` and `ShareInviteLinkBox` in
`calls/group/calls_group_settings.cpp`:

- Offered for a video chat in a channel or supergroup with a public username
  (the Settings box's "Share Invite Link" button; the "Invite Members" row of a
  public broadcast channel, `Panel` `addMembersRequests`; the left button of a
  scheduled public broadcast).
- A click asks for the listener link (`phone.exportGroupCallInvite`) and, when
  the viewer can manage the call, the speaker link (`can_self_unmute`). An RTMP
  stream has no speaker link, and a failed speaker request leaves it out. A
  second click while the answers are out does nothing.
- When both are in, the share box opens: a chat list ("Share to"), a "Users with
  this link can speak" checkbox (ticked by default, only when a speaker link
  exists) and a "Copy Speaker Link" / "Copy Listener Link" button that copies
  and keeps the box open, with the toast "Invite link copied to clipboard."
- A locked app asks for the passcode first (`ShowUnlockPasscodeBox`).
- The members list's invite row (`Members::setupAddMember`): the invite picker
  when the chat isn't a broadcast channel, you can send there and you joined as
  yourself; otherwise the share action when the chat is public; otherwise no
  row.

### What changed

- `src/calls/share_links.rs` (pure, tested): the two links, the click plan,
  answer and failure handling, the copy label and the invite row decision.
- TDLib: both links are `getVideoChatInviteLink` (schema 1.8.68 line 14789,
  "Returns invite link to a video chat in a public chat"), with
  `can_self_unmute` true for the speaker link. The purpose carries
  `can_self_unmute`, so each `httpUrl` lands in the right slot.
  `groupCall.is_rtmp_stream` is now parsed and tracked.
- Driver: `request_video_chat_share_links` replaces
  `fetch_video_chat_invite_link`. Revoking clears both cached links.
- UI (`src/ui/group_call_share.rs`): the box over the call window, opened from
  the "⋯" menu's "Share Invite Link" (public chats only) or the invite row. A
  chat in "Share to" opens in the main window with the link in its message
  box, unsent. The box and the invite picker wait while the passcode lock is
  on and bring the main window (its lock screen) forward.
- Fixed on the way: inviting someone to a video chat sent
  `inviteGroupCallParticipant`, which TDLib accepts only "for group calls not
  bound to a chat". Video chats now send `inviteVideoChatParticipants`.

### Differences that remain

- Quill's call window has no Settings box, so "Share Invite Link" stays in the
  "⋯" menu where Quill already had it. The menu is also there for a scheduled
  call, which covers tdesktop's scheduled-broadcast button.
- "Share to" puts the link in the chosen chat's message box instead of sending
  it with a comment to several chats. This is how every other link share in
  Quill works (`choose_share_chat`, sticker set links), so the two paths behave
  the same. Flagged for the owner.
- The call window has no toast layer, so "Invite link copied to clipboard."
  shows as a line in the box.
- A failed listener link is reported and the next click asks again. tdesktop
  has no failure handler there, so its button stays dead.
- In a private group, tdesktop's button copies the group's own invite link.
  Quill used to call `getVideoChatInviteLink` there, which TDLib refuses, so the
  item is now hidden in private chats. Copying the chat invite link needs
  `supergroupFullInfo.invite_link` / `basicGroupFullInfo.invite_link`, which
  Quill doesn't parse yet; that is a separate gap.
- "Revoke Invite Link" (`revokeGroupCallInviteLink`) predates this bundle and
  has no tdesktop counterpart for video chats. It stays and now clears both
  links.

## Screen source chooser (`parity:calls-screen-source-chooser`): blocked

tdesktop (`calls/group/ui/desktop_capture_choose_source.cpp`) lists screens and
windows from tgcalls' `DesktopCaptureSourceManager`, with a 1 fps preview each,
and hands the chosen `deviceIdKey` to the capturer.

ntgcalls 3.0.2 can't be given such a choice. `DesktopCapturerModule`
(`ntgcalls/src/media/devices/desktop_capturer_module.cpp`) always builds
WebRTC's generic capturer:

- macOS: `get_sources` returns one fixed entry, `"Screen / Window"` with
  `{"id":0,"display_id":0}`, and opening it shows the system picker
  (`allow_sck_system_picker`, `EnsureVisible`). Probed on this Mac through
  `ntg_get_media_devices`: that single entry is all it reports.
- Linux: upstream WebRTC builds the PipeWire capturer only under Wayland
  (`BaseCapturerPipeWire::IsSupported`). Its source list is one placeholder id
  and the xdg-desktop-portal dialog does the choosing. Under X11 there is no
  generic capturer, so no source.
- Windows: upstream WebRTC's `CreateGenericCapturer` has no Windows branch, so
  ntgcalls reports no source there either (unless its WebRTC build patches
  this; the probe ran on macOS only).

So where ntgcalls can share a screen at all, the selection already happens in
the operating system's picker, which shows screens and windows with
thumbnails (macOS 14 and later, the portal on Wayland). An in-app grid would have nothing to pass on: any id other than the
one ntgcalls lists is ignored. Building our own grid would mean capturing
screens and windows ourselves on three platforms and feeding frames through
`NTG_MEDIA_SOURCE_EXTERNAL`, which is a separate project. Left unchecked.

## Pause screen sharing (`parity:calls-screen-share-pause`): blocked

tdesktop has no pause button. `GroupCall` sets `_screenState` to `Paused` when
tgcalls' capturer calls `setOnPause` (for example when the shared window is
minimized), then sends `presentation_paused`
(`toggleGroupCallScreenSharingIsPaused` in TDLib).

ntgcalls reports no such event. `DesktopCapturerModule` only delivers frames,
`ERROR_PERMANENT` and cancel (both end the stream), and
`ntg_media_state.presentation_paused` mirrors Quill's own `ntg_pause` call, not
the capture source. Without the trigger, adding a pause button would be a
Quill invention. Showing other people's paused screen tiles already works
(`screen_sharing_video_info.is_paused`, `parity:calls-group-video-pause`).

## Conference calls (`parity:calls-conference`): blocked

In TDLib 1.8.68, group calls that aren't bound to a chat are end-to-end
encrypted. The signaling is all there: `createGroupCall`, `joinGroupCall` with
`inputGroupCallLink` or `inputGroupCallMessage`, `inviteGroupCallParticipant`,
`discardCall` with `invite_link` and `callDiscardReasonUpgradeToGroupCall` for
adding people to a 1:1 call, `callStateReady.is_group_call_supported`, and
`updateGroupCallVerificationState`. Quill already uses several of these
(invitations, accepting them, call links).

The media path is what's missing. TDLib keeps the call's key chain to itself and asks the
client to pass every outgoing and incoming packet through
`encryptGroupCallData` / `decryptGroupCallData`. tdesktop's tgcalls has a hook
for that (`e2eEncryptDecrypt`); ntgcalls doesn't. Its conference mode
(`NTG_CALL_TYPE_CONFERENCE`, `ntg_init_conference`, `ntg_apply_blocks`,
`ntg_on_outbound_block_callback`, `ntg_on_subchain_request_callback`) runs its
own key chain and needs the raw MTProto blocks and public keys
(`phone.getGroupCallChainBlocks`, `phone.sendConferenceCallBroadcast`, the
`public_key` and `block` of `phone.joinGroupCall`), which TDLib never hands
out. `joinGroupCall` takes only `groupCallJoinParameters`. Even with a hook,
`encryptGroupCallData` is not one of the methods TDLib runs synchronously, so
each packet would be an async round trip.

Without a media path, "add people to a 1:1 call" would move a working call
into a silent one, so it isn't offered. Unblocking needs either an encrypt /
decrypt callback in ntgcalls or chain-block access in TDLib.

## Partial items

`calls-video-tiles` and `calls-group-screen-share` are unchanged: both still
need a real group call with the native runtime, and this work joined no call.

## Verified

- Unit tests: `share_links` (plan, answers, failures, copy label, invite row).
- Driver tests with the recording sender
  (`src/connect/tests/group_calls/share_links.rs`): both requests and their
  JSON, no second request while generating, members and RTMP streams ask only
  for the listener link, speaker failure, listener failure and retry, the
  video-chat gate, revoke clearing both links, `inviteVideoChatParticipants`.
- Request JSON tests for `getVideoChatInviteLink` and
  `inviteVideoChatParticipants`.
- GPUI tests (`src/ui/group_call_share_tests.rs`): the box opens only once the
  links are in, copies the speaker or listener link to the clipboard and stays
  open, a chat gets the link as a draft, the public channel's invite row opens
  the share box, and the box waits for the passcode unlock.
- A temporary demo capture of the call window with the box (checked and
  unchecked, copied line) was looked at and not committed.
- Not verified: the links against a real video chat. No call was joined and
  nothing was sent.
