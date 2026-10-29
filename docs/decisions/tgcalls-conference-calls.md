# Conference calls on tgcalls

Conference calls are Telegram's end-to-end encrypted group calls. They are
not bound to a chat. A 1:1 call is upgraded into one by "Add People", and
you can also reach one from a `messageGroupCall` invitation or a `t.me/call`
link. This slice ships them on the tgcalls engine (`tgcalls-engine.md`),
matching Telegram Desktop (`calls_instance.cpp`, `calls_group_call.cpp`,
`calls_panel.cpp`).

## The frame encryption: a blocking bridge to TDLib

tgcalls encrypts and decrypts every audio and video frame of a conference
call itself. It calls `GroupInstanceDescriptor.e2eEncryptDecrypt`, which is
**synchronous**, on its media threads (the WebRTC frame transformer).
tdesktop answers that call in place with tde2e (`TdE2E::EncryptDecrypt`,
channel 0 for both the main and the screen instance).

TDLib keeps the call's key state, the blockchain, to itself. It offers only
the asynchronous `encryptGroupCallData` and `decryptGroupCallData`.
`td_execute` refuses both: they are not in `SynchronousRequests.cpp`.

We considered three options:

| Option | Verdict |
| --- | --- |
| Link tde2e and keep the key state in Quill | Impossible: TDLib owns the blockchain and exposes neither its blocks nor its key. |
| Decrypt after tgcalls, outside the media path | Impossible: tgcalls needs the plaintext frame back from the callback. |
| Block the media thread on TDLib's answer | **Chosen.** |

How the chosen bridge works (`src/telegram/group_call_crypto.rs`):

- The media thread sends `encryptGroupCallData` or `decryptGroupCallData`.
  - The request goes straight to the account's live client with `td_send`, which is thread-safe.
  - It carries an `@extra` of its own, `quill-e2e:<n>`.
  - It skips the connect driver's sender on purpose: the agent screen and the paid chat guard screen user actions, and a frame is neither.
- The thread then waits on a one-slot channel for at most `FRAME_TIMEOUT` (250 ms), and drops the frame if no answer comes. tgcalls treats an empty result as a dropped frame.
- The process-wide receive loop (`hub::receive_one`) passes every envelope to `group_call_crypto::intercept` before anything else, the same way the agent intercept works.
  - `intercept` hands a frame answer to its waiting thread with a non-blocking `try_send`, so the receive loop never waits on a call thread.
  - An answer that arrives after its thread gave up is still taken, so it never reaches the UI.

Why this is sound:

- **No deadlock.** Media threads wait on TDLib, and TDLib's answers flow through the receive thread, which never waits on media threads. The driver thread is not involved.
- **Bounded waits.** Each wait is capped. `qc_group_destroy` waits out callbacks in flight (`CallbackGuard`), so a teardown must never wait on TDLib. Every teardown path disarms the frames first:
  - `unbind` wakes every frame still waiting on TDLib (it drops their one-slot senders), and the driver unbinds before it leaves the call (`teardown_group_call_transport`), before a refused join's instance goes, and in its own `Drop`, which covers an account switch and quitting.
  - Every destroy in the engine (leaving, a rejoin's fresh instance, a refused create, the presentation, the engine's drop) first closes the instance's conference context. A frame that reaches a closed context is dropped at once, without a request.
  - The guard itself has a `closing` flag. `close()` sets it before it takes the unique lock, and `run()` checks it before the shared lock. glibc's rwlock prefers readers, and several media threads each hold the shared lock across a round trip, so without the flag `close()` could starve.
- **Frames before the join are dropped.** TDLib queues frame requests until the join completes and answers `GROUPCALL_JOIN_MISSING` when not joined. Each error is a dropped frame. Quill does not even send a request until the join answer binds the call id (tdesktop `setCallId`, see below).
- **The channel matches tdesktop.** The data channel is always `groupCallDataChannelMain`, in both directions and for both instances. tdesktop's `EncryptDecrypt::callback` passes `CallChannelId(0)` to `call_encrypt` and `call_decrypt` alike, for its screencast instance too. TDLib maps `groupCallDataChannelMain` (and a null channel) to tde2e channel 0, and `groupCallDataChannelScreenSharing` to channel 1 (`GroupCallManager::encrypt_group_call_data`). A shared screen sent on channel 1 would be decrypted by tdesktop on channel 0. Incoming screencast frames arrive on the main instance, and tgcalls does not say which stream a frame belongs to, so channel 1 is never used. The screen share instance shares the call's crypto.
- **A conference never runs in the clear.** `qc_group_create` returns NULL for `"e2e": true` without `callbacks.e2e`. The contract test checks it.
- **Leaving stops the requests.** `unbind` (tdesktop `clearCallId`) runs on teardown, and later frames drop without a request.

Each frame costs a JSON and base64 round trip through TDLib's client thread.
That is the price of TDLib owning the key. Quill accepts it until TDLib
offers a synchronous or in-process path.

Each call counts its round trips, timeouts and errors, and the answered
round trips' average and longest latency. When the call is unbound, one line
goes to the always-on calls log (`logs/calls.log`):
`[call] group <id> e2e: <n> round trips, <t> timeouts, <e> errors, avg <a> us, max <m> us`.

### The frames stay out of TDLib's log

Debug mode sets TDLib's verbosity to 4. TDLib writes every request and every
answer under its `td_requests` log tag (`VLOG(td_requests)` in
`td/telegram/Td.cpp`: "Receive request" and "Sending result for request"),
whose level is INFO (3). So at verbosity 4, `tdlib.log` would hold every
`encryptGroupCallData` request and every `decryptGroupCallData` answer: the
call's plaintext.

While a conference call is bound, Quill raises the tag's level out of reach:

- `setLogTagVerbosityLevel("td_requests", 1024)`. A line is written only when the global verbosity reaches its tag's level, so "lower the verbosity" of a tag means raising this number.
- The level before is read with `getLogTagVerbosityLevel` and put back when the last bound call is unbound or dropped. TDLib's tags are process-wide, so the calls of every account share one count (`RequestLogGate`).
- It is done whether or not debug mode is on, so turning debug mode on during a call writes no frames either.

While a conference call is on, TDLib's request log has no requests at all.
Its other tags still log as usual.

## The C API (version 4)

`QC_API_VERSION` is 4. The loader, `quill-calls-sys` and the CI check follow
it. Version 4 adds:

- the config key `"e2e": true`, which makes the instance call the new last callback `qc_group_callbacks.e2e(user, frame, set_result, result)` for every frame;
- the `qc_group_e2e_frame` struct, with the data, the direction, the sender's `user_id` and the unencrypted prefix;
- `set_result`, which copies the bytes before it returns. A callback that sets nothing drops the frame.

No symbol was added, so the loader is unchanged apart from the version.

## Who sent each frame: media channel requests

Decryption needs the sender. Without `request_media_channels`, tgcalls labels
incoming audio with `user_id` 0. A conference instance therefore answers
tgcalls' `requestMediaChannelDescriptions` from the participants.
`conference_ssrc_owners` maps both `audio_source_id` and
`screen_sharing_audio_source_id` to their user, as tdesktop's
`mediaChannelDescriptionsStart` does:

- A request whose ssrcs are all known is answered at once.
- Otherwise it waits until a participant update supplies the missing ssrcs, or until `loaded_all_participants`.
- With every participant loaded, an unknown ssrc is answered unresolved, as tdesktop does.
- A request can come while `qc_group_create` is still running, before the engine has the instance's handle. Its answer is kept and sent once the handle is attached, so it is never lost.

The pump sends the owners to the engine on every tick
(`set_group_ssrc_owners`), and the engine ignores an unchanged set.

## The provisional engine key

The join's tgcalls offer must exist before `joinGroupCall` or
`createGroupCall` goes out. The call's id, however, arrives only with the
`groupCallInfo` answer. The flow is:

1. The instance is created under `PENDING_CONFERENCE_KEY` (-1).
2. The answer is seen before the reducer (`note_conference_answer`).
3. That step renames the instance (`rekey_group_call`) and binds the crypto to the call.
4. A refused join drops the instance and the crypto.

A conference instance has chat id 0 and the Member role.

## The flows

| Flow | What happens |
| --- | --- |
| Accept an invitation | `joinGroupCall(inputGroupCallMessage)`. |
| Join a link | `t.me/call/<slug>` asks "Join Group Call" (`lng_confcall_join_*`), then `joinGroupCall(inputGroupCallLink)`. |
| Add People (caller) | Only for an established 1:1 call whose `callStateReady.is_group_call_supported` is true. Earlier, the toast `lng_call_error_add_not_started` shows. `createGroupCall` makes the conference with you in it. Once its `groupCall.invite_link` is known, the 1:1 call ends with `discardCall(invite_link)` and the picked people get `inviteGroupCallParticipant` (tdesktop `startedConferenceReady`: `finishConferenceInvitations`, then `destroyCurrentCall` with the slug). With "Create Call Link", the panel opens on the link instead. |
| Add People (callee) | `callDiscardReasonUpgradeToGroupCall(invite_link)` joins the link (tdesktop `finishByMigration`). Neither side shows the 1:1 end screen. If the join can't go out or TDLib refuses it, the 1:1 call ends as failed: its end screen shows "Failed to connect" (tdesktop sets `State::Failed`, `lng_call_status_failed`). |
| Incoming invitation | A fresh, incoming `messageGroupCall` that is not active and not missed rings in the call window: "Telegram Group Call", who already joined, Decline or Join. It stops when the message says you joined or missed it, when the message is deleted, or when you answer. One older than 90 s, or one that arrives during a call, never rings and is declined (`showConferenceInvite` calls `declineIncomingConferenceInvites` for both). |
| Rejoin | After `need_rejoin`, Quill sends `joinGroupCall` again with the call's link, or with how the call was first reached. The instance is rebuilt under the real id, with the same crypto. Once the rejoins are spent (or the transport failed), the conference is dead: a new invitation or link replaces it without pressing Leave first. |
| End | `endGroupCall` is gated on `is_owned` for a conference. |

In the panel, a conference shows:

- the title "Group Call";
- the "E2E Encrypted" badge, then the verification emoji with tdesktop's explanation;
- an invite box titled "Add People" that starts with the call link and Copy Link;
- `messageGroupCall` rows labelled like tdesktop's group call entries.

## Gaps

- **No live interop test.** Interop with real Telegram clients was not
  tested, because the rules forbid real accounts. The work is covered by:
  - the C contract test against the built library (all callback combinations);
  - a real-library test that creates, renames, joins and leaves a conference instance;
  - deterministic driver tests on the mock engine;
  - unit tests of the crypto bridge with a synchronous fake client.
- **Speed.** Every frame pays a JSON and base64 round trip through TDLib, as described above. CPU use with many video participants is not measured.
- **Camera carry-over.** tdesktop moves the 1:1 call's camera, or screen, into the conference. Quill starts the conference with the camera off, so you turn it on in the panel.
- **The invite box is simpler than tdesktop's.**
  - It lists the first 30 contacts with no search.
  - It has no per-person video invite: invites go out with `is_video` false.
  - It doesn't show the `lng_confcall_invite_*` result toasts (privacy, already in, banned). A failed invite shows the existing group call error.
- **The invitation screen.** It names who already joined in text, without their photos. It plays no ring sound and posts no system notification, unlike a 1:1 call.
- **The link confirmation.** It doesn't preview the call's participants. tdesktop fetches them first.
- **Link revocation and creation.**
  - There is no "Revoke link" for a conference call.
  - There is no standalone "Create Call Link" from the Calls tab (tdesktop `lng_confcall_create_link` outside a call).
- **The chat list preview.** A `messageGroupCall` still reads "Video chat invitation" or "Voice chat invitation".
- **Duration.** `messageGroupCall.duration` is parsed but not shown on the row.
