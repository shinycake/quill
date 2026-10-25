# Agent access

Agent access lets AI agents on the user's own computer (Claude Code, Claude
Desktop, Cursor, any MCP client) use their Telegram account through the
Quill that is already running. It ships in six phases behind one switch,
Settings > Advanced > Agent access, which is off by default. This record
covers phase 1, the channel and the bridge. Later phases add sections here.

## Why through the running Quill

Agents talk to Quill, never to TDLib or Telegram directly. Two processes
cannot share one TDLib database (that is why `single_instance.rs` exists),
and a second database would mean a second login, a new device in Active
Sessions and double the traffic on Quill's api_id. So the agent service lives
inside Quill and every request passes through it.

## Telegram's terms, and how the design answers them

| Rule | What it asks | How Agent access answers |
| --- | --- | --- |
| [API Terms](https://core.telegram.org/api/terms) 1.4 | No actions for a user without their knowledge and consent; no ghost mode, hidden read, online or typing status, or defeated self-destruct. | Off by default and turned on by the user. Every write (phase 3) is logged in Quill. No tool suppresses read, online or typing status. |
| [API Terms](https://core.telegram.org/api/terms) 1.5 and [Content licensing and AI scraping](https://telegram.org/tos/content-licensing) | No use of Telegram data to train, build or deploy AI, except with explicit, informed consent limited to a specific chat. | Quill never sends data to any AI itself: the user's own local agent asks Quill for it. Access is granted per chat (phase 2), matching the per-chat consent wording. No bulk export, crawl or index tools. The settings page and the agent guide say not to train on the data. |
| Content licensing, client exception | Clients may access data only as far as running the client strictly requires. | Agent access is automation of the user's own Quill, documented as operating the client, not as a data feed. |
| [Obtaining api_id](https://core.telegram.org/api/obtaining_api_id) | Flooding, spam or faked counters get the api_id banned; unofficial clients are watched. | Phase 3 sends go through one paced queue in Quill that obeys flood waits, with a cap on first messages to new peers. No mass-messaging tool. |

The residual risk is a broad reading of "deployment" that objects to any AI
reading chats with other people. The answer would be to narrow the default
grants, which is a settings change, not a redesign. This is a reading of the
text, not legal advice.

## Channel

The existing single-instance endpoint carries both protocols: a Unix domain
socket in the app data folder (a short temp path when that is too long for
`sun_path`) or a named pipe on Windows. The first line picks the protocol:

- `quill-ipc 1`: launch forwarding, unchanged.
- `quill-agent 1`: an agent connection (`src/agent/server.rs`).

The header is read a byte at a time, so nothing after it is consumed before
the handler for that protocol takes the stream. Agent connections are
long-lived, so they leave the launch-forwarding thread cap and take one of
four agent slots instead.

After the header the client speaks line-delimited JSON-RPC 2.0. The first
request must be `auth` with the token; then `tools/call`, `resources/read`
and `ping`, answered in MCP's result shapes, up to eight in flight per
connection. Each request runs on its own thread, so a slow TDLib call does
not hold up the rest.

## Who may connect

- The Unix socket is mode 0600 and the server checks that the peer's
  effective uid is its own (this also covers the temp-folder fallback path).
- The Windows pipe gets a DACL that grants access to the current user's SID
  only (`src/pipe_security.rs`). The default pipe DACL would let any local
  user connect.
- A random 256-bit token sits in `agent-token` in the app data folder, mode
  0600, written through a temp file that is 0600 from creation. Quill makes a
  new one each launch, writes it once the socket listens, and deletes it when
  the switch goes off. A connection whose first request does not carry it is
  answered with an error and closed. The token is compared in constant time.
- A connection takes one of the four agent slots only after it
  authenticates, so a client that never sends `auth` cannot lock agents out.
  Request lines are capped at 4 MiB on every read; a longer one closes the
  connection.
- `quill mcp` checks the socket's owner too (peer uid on Unix), and does not
  start Quill while the switch is off.
- On Windows both ends check the other's process user against their own
  (`pipe_security::is_current_user_process`): the pipe name is predictable,
  so another local user could create it first and collect the token.
- At most eight connections may wait to authenticate at once, and the
  `auth` line is capped at 4 KiB (a Windows pipe has no read timeout).
- Turning the switch off bumps a generation counter; open connections close
  within a second on Unix (they wake on a receive timeout to check), and at
  their next request on Windows, where named pipes have no read timeout. Answers
  that finish after the switch went off are replaced by the off error.

There is no network listener, now or later.

## Routing agent requests

Agent requests go to the account's live TDLib client through the shared
`TdHub` with an `@extra` of their own, `quill-agent:<n>`. The receive loop
offers every envelope to `agent::td::intercept` before parsing; an envelope
whose `@extra` carries that prefix goes to the waiting agent thread (or is
dropped when its caller already gave up) and never reaches an account
session. The check is a substring test first, so UI traffic pays almost
nothing. Clients register by account key when they open
(`connect/live.rs`) and count as alive while their route on the hub exists.

## `quill mcp`

A mode of the same executable (`dev_cli` dispatches it, so `main.rs` does not
change). It is an MCP stdio server. `initialize`, `tools/list`,
`resources/list` and the guide resource are answered from the binary itself,
so an agent can see what Quill offers before Quill runs. `tools/call` and
`resources/read` go to Quill verbatim and Quill's answers come back verbatim.
On the first forwarded request the bridge connects, or starts Quill (no
shared stdio, its own process group, so it outlives the bridge) and waits up
to 45 seconds for the socket. If Quill quits or the switch goes off, every
request still waiting gets an error that says to retry.

## Tools and discoverability

Phase 1 tools read Saved Messages only: `quill_status`, `list_messages`,
`read_history`, `search_messages`, `get_message`. The `chat` argument already
takes any chat, so phase 2's grants change no tool's shape. Every tool
description says when to use it, gives an example input and names the output
shape. Every error says what to do next ("Chat not granted…", "Agent access
is off in Quill. Ask the user to turn it on…"). Results come as readable text
lines plus the same data as `structuredContent`. The server's `instructions`
orient the agent (what Quill is, the four layers, grants, start with
`quill_status`), and `quill://guide` and `quill://grants` are resources.

## Settings

Settings > Advanced > Agent access: an explanation, the Allow agents switch,
and while it is on, the scope line and Connect an agent with copyable setup
commands for Claude Code and any other MCP app. The page reads the switch
from `agent::access` on every render; nothing caches it.

## Phase 2: grants, read tools and events

### Grants

`src/agent/grants.rs`, saved in `agent_grants.json` per account key.

| What | Default | Why |
| --- | --- | --- |
| Saved Messages | read and write | The user's own notes; the safest place for an agent to work. |
| Bots | read and write | Talking to bots is what automation is for, and a bot is not a person whose words get read. |
| Channels the user owns | read | Their own posts. Writing would reach every subscriber, so it is asked for, not assumed. |
| All other chats | off | Reading other people's messages needs the user's explicit choice (API Terms 1.5, content licensing). Turning it on shows its own consent text. |
| Secret chats | off, one at a time | Never covered by All chats: the user picked end-to-end encryption for them. |
| Contacts | off | Names and usernames only, never phone numbers. |

A per-chat entry overrides everything, including a block (`scope: None`)
that keeps one chat out of All chats. Where a class default and All chats
both apply, the wider scope wins. Every call reads the grants afresh, so a
change in settings applies to the next call. A chat's class (Saved, bot,
own channel, secret, other) comes from TDLib (`getChat`, then `getUser` or
`getSupergroup`) and is cached per client.

### Naming chats

Tools take a chat id, `"saved"`, an `@username` or t.me link
(`searchPublicChat`), or a title (`searchChats`, exact match first, then a
single result; several matches answer with the candidates and their ids).
Titles only ever match granted chats, and no error names an ungranted chat:
otherwise an agent could list the people the user talks to by trying names,
around the contacts grant. "Chat not granted" gives only the id it was
passed. `request_access` alone resolves a name among all chats; the user
sees the match in the dialog, and the agent learns its title and id only
once the user allows it.

### Tools

`list_chats`, `list_folders`, `list_unread`, `get_chat_info`,
`list_members`, `list_contacts`, `download_file`, `wait_for_message`,
`request_access`, and `search_messages` across all granted chats. Only
granted chats ever appear: a search across chats drops matches in other
chats without saying how many there were. `list_unread` reads history with
`getChatHistory`, which does not mark anything read. `download_file` copies
from TDLib's cache to a path the agent names, or returns the cache path.
The sender picks the file name, so it is cut to its last path component
(no folders, `..`, drive letters or control characters), and the copy
never overwrites: a taken name in a folder gets " (2)", an existing file
path is refused. The cross-chat search cursor is TDLib's own offset format,
"date,chat_id,server_message_id", where the server id is the TDLib message
id shifted right by 20.

### `request_access`

The tool files a prompt (`src/agent/prompts.rs`) and waits up to two
minutes. Quill shows it as a dialog naming the agent (the MCP client's
`clientInfo`, which `quill mcp` passes at `auth`), what it asks for and its
reason in its own words, raises the window, and applies the grant before it
answers Allowed. Unanswered prompts are withdrawn. At most four wait at
once. Turning access off answers every prompt Denied.

### Events

The hub passes every raw envelope to `agent::events::observe`. TDLib writes
`@type` first, so a prefix test picks out `updateNewMessage` (kept in a
256-message buffer, only while an agent is connected) and
`updateChatFolders` (kept per client; TDLib sends it once at start, and
there is no call that lists folders). Grants are applied by the readers on
their own threads, never on the receive thread. `wait_for_message` waits on
the buffer; `quill://unread` can be subscribed to, and a watcher thread per
connection sends `notifications/resources/updated` when a message from
someone else arrives in a granted chat.

## Phase 3: writes, pacing, the activity log and the connected strip

### Write tools

`send_message`, `send_file` (2 to 10 files go as an album), `send_poll`,
`send_sticker`, `forward_messages`, `edit_message`, `delete_messages`,
`react`, `pin_message`, `set_draft`, `mark_read`, `mute_chat`,
`archive_chat` and `set_chat_folder` (`src/agent/tools/write/`). Each one
needs `read_write` on its chat. `forward_messages` needs only `read` on the
source chat. Edits and deletes accept only the user's own messages
(`is_outgoing` plus TDLib's `getMessageProperties`), so an agent never
removes what someone else wrote. In a channel Telegram marks every post as
outgoing, so there "own" means any post the user may edit as an admin;
channels are never granted for writing by default. Text is plain by default. `markdown`
(MarkdownV2) and `html` go through TDLib's `parseTextEntities`, and a parse
error says how to fix it. `mark_read` is the only path that marks anything
read, and an agent calls it only when asked.

The bots default (`read_write`) covers bots the user already talks to. A
bot with an empty chat, typically one an agent just found by @username,
needs its own grant before an agent writes to it, because a stranger runs
it. `send_file` refuses paths inside Quill's data folder (sign-in keys, the
agent token, the cache), links resolved first.

### Sends

Every message an agent sends goes through `src/agent/pacing.rs`, and every
other write takes a turn in the same queue (one message's worth), so a
flood wait stops edits, reactions and pins too:

- A send to one chat starts at least 1.5 s after the previous one, and any
  send at least 0.4 s after the previous one (times the number of messages
  in an album or forward).
- A turn more than 30 s away is refused rather than queued.
- A flood wait from Telegram pauses every agent send for the seconds it
  names. Nothing is retried for the agent; it gets the wait in the error.
- At most five first messages a day may go to people who never wrote to the
  user: a private chat with a non-contact and nothing incoming in the last
  50 messages.

Agents never set `paid_message_star_count`. Before a send, Quill checks the
chat's price (`userFullInfo.outgoing_paid_message_star_count` or
`supergroup.paid_message_star_count`) and refuses a chat that charges, so
agents cannot spend Stars. TDLib answers a send with a pending message; the
tool waits up to 30 s for `updateMessageSendSucceeded` or
`updateMessageSendFailed` (recorded by `events::observe` while an agent is
connected) and returns the final message id, or the error.

Each call remembers the switch's generation when it began. Right after its
turn in the queue and before it reaches TDLib, a write checks that Agent
access is still on in that generation, so Revoke or the switch stops a
write that was waiting in the queue; the agent hears that nothing went out.

### What the user sees

- **Activity log.** Every write that reaches Telegram or the user is
  logged, with refusals of someone else's messages and the user's declines,
  in `agent_actions.json` (newest 500, all accounts, owner-only) with its
  time, agent, kind (`send_message`, `unpin_message`), chat, whether it
  worked, and what it carried (a short quote or ids). The UI words the kind
  in the user's language; the file is written outside the log's lock.
  Refusals by the grants are not logged: they change nothing and name no
  chat the agent may see. Settings > Advanced > Agent access
  lists the active account's log, 50 rows at a time, with Clear. Reads are
  not logged.
- **Connected strip.** While an agent is connected, a quiet row at the
  bottom of the chat list names it ("Claude Code connected", or "2 agents
  connected"), with a tooltip and Revoke. Revoke turns Agent access off,
  which ends every connection and deletes the token. Presence is held by
  the server for the life of each authenticated connection.
- **Confirm each send** (off by default, beside the switch). When on,
  sends, forwards, edits, deletes, reactions and pins wait for the user's
  Allow in a dialog titled by the kind of write ("Claude Code wants to
  delete your messages in "Ann"") that shows what it carries. Drafts, read state,
  mute, archive and folders don't ask.

### Name lookups

A title that matches no granted chat gets the same answer whether or not an
ungranted chat matched, so trying names reveals nothing about chats the
agent can't see.

## Phase 4: raw TDLib calls

`td_call` (`src/agent/tools/td_call.rs`) passes one TDLib request through
for what the curated tools don't cover.

**The schema decides, not the method name alone.** The first review showed
that guessing from names and fields lets a request lie: a decoy `chat_id`
on a method that has none turned a deny-listed call into a granted one, and
a chat id sent as a string skipped the grant check. So every request is
read against `schema/td_api.tl` (the schema Quill is built with, embedded
and parsed on first use, `td_schema.rs`):

- An unknown method is refused. Fields the method or a nested constructor
  doesn't have are dropped before anything else looks at the request, so
  padding and decoy ids are gone (`td_request.rs`).
- Every chat id (`chat_id`, `*_chat_id`, `*_chat_ids`, and the chats of
  `supergroup_id`, `basic_group_id`, `secret_chat_id`) is made an integer;
  anything that isn't one is refused.
- A change runs under the grants only when the method has a top-level
  `chat_id: int53`. Every other change asks.

**Classes** (`td_policy.rs`):

- **Blocked.** Quill's plumbing (`setOption`, so no agent can turn off read
  receipts, online status or typing; `setTdlibParameters`, `close`, the
  sign-in flow, logs, push, `setDatabaseEncryptionKey`, file generation,
  terms of service, `openChat`/`closeChat`), any method with a `file_id`,
  any request with a nested `inputFileId` or `inputFileGenerated`
  (TDLib runs a `#file_id#` conversion itself, reaching a file by id), and
  `searchFileDownloads`/`getRemoteFile` (file ids are small integers handed
  out in order; `download_file` checks the chat a file came from), the TON
  wallet and its plumbing (TON Center, wallet bot, on-ramp; owner rule),
  `addLocalMessage`, `getCurrentState` and `getChatFolderChatCount` (they
  cover chats agents may not see), `getLanguagePackString` (it opens any
  path), and any request carrying `callbackQueryPayloadDataWithPassword`.
- **Ask, account.** `deleteAccount`, `logOut`, `terminateSession`,
  `terminateAllOtherSessions`, `setPassword`, `changePhoneNumber`, recovery
  and login email, privacy rules, proxies, `sharePhoneNumber`,
  `addContact`, `importContacts`, `transferChatOwnership`, `deleteChat`;
  reads that act for the user (`getCallbackQueryAnswer` presses a bot
  button, `getLoginUrl` and `getWebAppUrl` sign in to sites and web apps,
  passport elements, `validateOrderInfo`, inline queries that send a
  location, `getUserLink` which can revoke links and waive the paid-message
  fee); account data (`getActiveSessions`, `getProxies`, saved order info,
  password state); RTMP stream keys; stories and live stories (TDLib posts
  them as the user from the Saved Messages chat id, which agents have by
  default); every method with a `group_call_id` and no `chat_id` (call ids
  are small local integers that reach any chat's call); and every change
  without a `chat_id`.
- **Ask, money.** Any method or nested type named for Stars, TON, Grams,
  payments, gifts, paid content, Premium, giveaways, invoices,
  subscriptions, offers, stakes, revenue or withdrawals (whole camel-case
  words, plurals listed, so `startGroupCall` is not "Star"), named methods
  that move Stars or boosts without such a word (`boostChat`,
  `joinChatByInviteLink`, refunds of unpaid-message charges, withdrawal
  URLs), and any request with a positive `paid_message_star_count` or
  `star_count` at any depth (`searchPublicPosts` takes its price at the
  top level).
- **Granted.** Reads need `read` on every chat they name. Chat-scoped
  changes need `read_write` on every chat they act in (a forward's or an
  external reply's source needs `read`), are refused for a bot the user
  never talked to, wait for "Confirm each send" when it is on, and take a
  turn in the paced queue. Sends also go through the paid-chat check, the
  new-conversation cap and the delivery wait.

An allowed Ask still needs the grants on every chat it names, and an
allowed send still goes through the queue. The Allow dialog shows the
agent's reason and the whole sanitized request, scrollable; a request over
6000 characters is refused rather than shown in part, since the agent
controls field order and could push a field out of view.

**Answers** pass through `td_redact.rs`. Anything belonging to a chat the
agent may not read (`chat`, `supergroup`, `basicGroup`, `secretChat`, any
object with a `chat_id`) is dropped from lists; elsewhere it becomes a stub
with no id. An object is judged by its own id only (a story by its poster): its other chat fields
(`linked_chat_id`, `personal_chat_id`, `supergroup_id`...) read 0 when not
granted, so a granted channel with an ungranted discussion group still
shows. Every `…chat_ids` list keeps only granted chats. A list filtered by
chat, or left empty, always has its `total_count` set to the items
returned, even when nothing was dropped (otherwise `getChats` on a folder with a granted first
chat would count the folder). `phone_number` and `vcard` fields are
dropped. `sponsored*` and `…Advertisement` objects get `"sponsored": true`.

TDLib builds some paging offsets from the last result (`searchMessages`
packs its date, chat id and message id), so answers carry opaque
`quill-offset:` tokens that Quill maps back (`td_offset.rs`, last 512
kept). When redaction drops items from a paged read, Quill follows up to
ten more pages to fill it, so a short or empty page does not by itself say
that hidden chats matched. The
contact-list reads need the contacts grant. Every call that isn't a read is
logged under `td_call` with its method, and declines are logged as such.

Why the deny list asks instead of refusing: a user can tell an agent "log
out my old laptop session" or "send Ann a gift" and approve it, while no
agent can do either on its own.

## Phase 5: UI driver

Step 1 (this record): the element tree, input injection, the tools, the
security model and the setting's row in Settings, with no other visuals.
Step 2 adds the pause on real input and the on-screen sign that an agent
is driving.

### Why an element tree, not pixels

Agents see the window as GPUI's own accessibility tree: every element with
a role and an id (Quill has about 375 roles and 850 labels today), with its
AccessKit id, role, name, value, states, supported actions, children and
bounds in window logical pixels. It is the tree a screen reader gets, so
work that helps one helps the other, and no screenshot goes to a model.

GPUI builds that tree only while assistive technology is connected
(`window.a11y.is_active()`). The vendored `gpui-pre` gains
`Window::set_a11y_forced(bool)`, ORed into the per-frame check, and
`Window::a11y_tree_snapshot()`, the last built tree as plain data
(`third_party/gpui-pre/QUILL-CHANGES.md`). A forced tree is never sent to
the platform adapter, and `Window::is_a11y_system_active()` keeps Quill's
screen-reader behaviour (focus announcements in the history, Up/Down
moving the read message) off: an agent driving must not change what the
user's keys do. Only the tree gating (`slices.a11y_active`, which renders
every slice uncached so the tree is whole) follows the forced flag.

The tree is forced only while it is used: the switches are on, an agent is
connected and one drove within the last 30 seconds
(`ui_driver::bridge::wants_forced`). The agent wake task checks every five
seconds while forced and releases it.

### Cost of forcing the tree

Measured with `forced_tree_frame_cost` (`src/ui/agent_driver_tests.rs`,
ignored; `cargo test --release --features demo-capture --bin quill --
--ignored --nocapture forced_tree_frame_cost`), the `ready-chats` demo at
1100x900 under the test platform (CPU layout and paint, no GPU), 60 full
redraws each way, on the M-series Mac this was built on, 2026-10-25:

| Build | Full redraw, tree off | Tree forced | Off again | Nodes | Snapshot read |
| --- | --- | --- | --- | --- | --- |
| release | 2.0 ms | 3.3 ms | 4.2 ms | 47 | 78 µs |
| debug | 82 ms | 111 ms | 88 ms | 47 | 134 µs |

So forcing the tree costs on the order of a millisecond per drawn frame in
release, within the run-to-run noise of this harness (the second "off"
run came out slower than "on"); the debug build shows the same shape at
about 30 ms. Nearly all of it is the slices rendering uncached (the price
a screen reader pays too), not the tree itself, which the 78 µs snapshot
read bounds. It is paid only while an agent drives and only on frames that
are drawn; idle Quill, and Quill with agents merely connected, build no
tree. A chat with a long history has more nodes than this demo's 47;
message rows join the tree only when a screen reader focuses them, so the
count stays in the low hundreds.

### How a command runs

Tool threads never touch GPUI. `ui_driver::bridge` is a queue of
`UiCommand`s with a reply channel each (`post`, `Ticket::wait`; `send`
does both), woken with `wake::AGENT.poke()`. The UI thread drains it where
it already reads agent state, in the agent wake task
(`agent_state.rs`, `QuillApp::agent_driver_tick` in
`src/ui/agent_driver.rs`). `State` (open chat, window active) is answered
at once. Every other command becomes a task: force the tree, wait for a
frame newer than the one before (polling `Window::a11y_tree_frame` every
16 ms, up to 90 times), inject the input through the window context (the
handlers update `QuillApp` themselves, so this cannot run inside the
entity's own update), then wait for the frame after the input's and answer
with that tree and the nodes that appeared. Input goes through
`Window::dispatch_event` and `dispatch_keystroke`, the platform's own
path: a MouseMove, then MouseDown and MouseUp per click (`click_count`
counts up for double clicks), a ScrollWheel with the delta negated so
"dy > 0 scrolls down", a keystroke with `key_char` per typed character and
a bare KeyDown per chord (`dispatch_keystroke` would also hand Enter's
"\n" to the text box, which no platform does for a key the window let
through; the message box would send and keep a line break). A command is
split into steps, one pointer action, one character or one chord each
(`agent_driver_input::steps`), and each step is checked on its own, below.
A thread-local `INJECTING` flag (`ui_driver::guard`) spans the whole
update that injects a step, so the effects a handler queues (an emitted
event, a subscription) still run under it; `presence::input_probe` skips
injected input, so an agent's clicks keep nobody online and (step 2) real
input can pause the driver.

### Security model: the driver follows the chat grants

The first cut gated the driver on the open chat alone and guarded clicks
by matching English words in element names. That did not hold: an agent
could click Allow in its own request dialog, open Settings > Agent access,
read every chat row in the list, ctrl-tab into an ungranted chat and get
its content back as "nodes that appeared", and a renamed button slipped
the word list. The model now is structural, decided by app state and
element identity, never by labels.

Surface allowlist. `QuillApp::driver_surface`
(`src/ui/agent_driver_surface.rs`) computes, from app state alone, what
the window is showing right now: `Surface::ChatList`, `Surface::Chat
{chat_id, scope}` or `Surface::Blocked(what)`
(`src/agent/ui_driver/surface.rs`). In order: the sign-in screen, the
passcode lock screen, the welcome screen and Settings are blocked; so is
any agent prompt (the Allow/access/confirm/`td_call` dialogs and the
All-chats consent); a dialog on `HARD_DIALOGS` (log out, delete account,
passcode, 2FA, passkeys, every payment, Stars, gift, Premium and
subscription dialog, the accounts switcher, web-app and login-URL
confirmations, the callback-password prompt) is blocked in every mode and
`every_hard_dialog_is_registered` keeps the list honest against
`QuillShell`'s dialog registry; an open ESC layer on `HARD_LAYERS`
(sessions, privacy, payment and earn layers, screen privacy, the first-run
intro, pending deletes) is blocked likewise. Then the open chat: granted,
or `Blocked("an ungranted chat")`. A blocked surface makes `read_ui_tree`
answer a short note ("Quill is showing Settings; agents can't see or use
it. Ask the user.") and refuses every input; an agent cannot approve its
own request because the request is a surface it never sees. A system
screen reader being active (`Window::is_a11y_system_active`) refuses
reads and input outright: the tree then belongs to the user.

Two scopes, one widening switch. `DriverScope::Granted` is the base: the
chat list's own parts (search, folder tabs, rows of granted chats, search
results of granted chats), the open granted chat, its forum column and
context menus; other dialogs, side panels (chat info, shared media), the
archive row and ESC layers not on `GRANTED_LAYERS` (menus, pickers, the
chat search bar, reply and forward panels, selection mode) are blocked.
"All other chats" (`grants.all_chats`) switches to
`DriverScope::Everything`: every chat counts as granted except secret
chats, which still need their own grant, and dialogs, panels, viewers and
layers not on the hard lists are allowed. The hard lists apply in both
scopes. One pure decision, tested without a window:
`every_listed_layer_exists_and_no_layer_is_both`.

Redaction by region and chat. Containers carry stable GPUI-side tags
(`InteractiveElement::a11y_tag`, a `gpui-pre` addition; the tag is never
sent to the platform adapter): `region=chat-list|chat|forum|menu|panels`,
`part=search|folders|rows|search-results`, `chat=<id>` on every chat row,
search result, contact and top-people tile, and `archive` on the archive
row. `ui_driver::redact::redact` walks the snapshot from the root, zones
each node (allowed, chat list, list, pruned) from its ancestors' tags and
the scope, drops pruned subtrees and ungranted `chat=` nodes with their
children, and keeps the ancestors of allowed nodes with their names,
values and actions removed so the shape survives without the content.
The tree an agent gets is computed in the same `cx.update` that checks
the surface, so no answer describes a window other than the one checked.

Lists are default-deny (second review). The first cut let an untagged
row through: search media rows, downloads, joined channels and the reply
chooser named ungranted chats in the granted scope. Now `part=rows`,
`part=search-results` and any container tagged `list` are a zone where
only two things survive: a `chat=<id>` subtree of a granted chat, and a
`controls` subtree (a filter strip, a "Show more" row, a header with no
chat in it). A row that names no chat is cut, whatever it shows, so a
new list leaks nothing until someone tags its rows. `point_allowed`
mirrors it: a point inside a list needs a granted row or a control under
it. Every chat-bearing row carries its chat: search results and media
rows, the channels tab, the apps tab (a bot's chat id is its user id),
the reply-elsewhere and share destinations, the player bar and the
downloads rows (the chat is recorded on the download when it starts,
`DownloadsState::note_chat`, so a row never scans messages to find it). `Everything` ignores `list` and `controls`: there every chat
but a secret one is granted. The window test
`list_parts_hide_rows_of_ungranted_chats_and_untagged_rows` renders the
channels, photos, downloads and posts tabs and the reply chooser with
ungranted chats and asserts nothing from them reaches the tree, then
turns All chats on to prove the rows render.

Checks per step (`agent_driver::check_step`). Before each step: the
ticket is still waited for (a tool that timed out cancels it, so a late
step never runs), the drive switch and Agent access are on, the grants
generation has not changed since the command was filed, no screen reader
appeared, the surface still allows input and is the one the step before
left (a key that moved to another granted chat continues; one that would
open an ungranted chat is refused by `QuillApp::agent_may_open` in
`select_chat_with`, `select_search_chat` and `select_search_message`, and
the sequence stops with no tree). Characters, line breaks, Enter and
Space go only while GPUI reports a focused text input handler
(`Window::focused_text_input`) and the tags above the focused element
pass `redact::point_allowed` (`Window::focused_a11y_tags`, recorded at
paint from a tag stack, so a key is checked against the regions the way
a click is), a tab character is refused outright, and a line break is
typed as the chord that inserts one under the user's send-key setting. A
pointer step first injects a MouseMove, then asks GPUI which tagged
hitboxes are under the pointer as it hit-tests them
(`Window::hovered_a11y_tags`: the hitboxes `Hitbox::is_hovered` would
report, within `hover_hitbox_count`, no further than an occluding
hitbox, tagged or not) and `redact::point_allowed` decides from those
tags and the scope; only then does it press. A node target must have
bounds inside the viewport. The `press_keys` chord list is checked chord
by chord, so a sequence stops at the first refusal. Between steps the
task waits for the a11y frame index to move past the frame the previous
step was injected into (not a timer), so the focus and the surface a
step meets are the ones its predecessor left.

Action guards. While `guard::injecting()` the handlers themselves refuse
what the driver must not do, whatever element was clicked:
`grants::update` and every `access` setter, `prompts::answer` (no
self-approval), `confirm_all_chats`, voice and video recording, the paid
and formatted send paths, and `LiveSender::send_json` screens every TDLib
request (`guard::screen_request`, `ConnectSendError::AgentDriving`):
first the `td_policy` read allowlist, then money by parameters, not by
prefix: a request whose own type or any nested type `names_money`, or
that carries a positive `star_count`, `paid_message_star_count`,
`price`, `amount`, `star_amount` or `total_amount` at any depth
(`guard::carries_money`, with `td_request::spends`), is refused whatever
it is called. That is what stops the paid public-post search
(`searchPublicPosts` with `star_count`), which reads by name and spends
Stars; its confirmation (`PaidPostSearch`) refuses while injecting too.
The screen covers reactions, poll votes, bot buttons, deletes, leaves,
blocks, reports and payments from any surface the redaction missed. Each
guard records a refusal (`guard::refuse_if_injecting`) that the tool
reply lists under `refusals`. A read refused only for its money (an
`Everything`-scope profile asks for gifts, a channel page for revenue)
is not an error to the caller: `AgentScreen` (`src/connect/sender.rs`)
queues a TDLib-shaped error for the request's `@extra`
(`guard::refused_read_answer`, code 403 `QUILL_AGENT_DRIVING`) and the
next UI poll ingests it (`ConnectDriver::ingest_refused_answers`, at
`LOCAL_ANSWER_SEQ` outside TDLib's receive order), so each caller's own
error path clears what it set loading and no spinner sticks
(`connect::tests::agent_screen`). Writes and the denied reads
(`DENIED_READS`, now with `getLoginUrlInfo`) still return
`AgentDriving`.

Nothing leaves Quill (second review). `Everything` scope reaches
surfaces with local effects, so those refuse while injecting whatever
the scope: `ZoomPlatform` (`src/ui/interface_zoom.rs`), the one
decorator every `open_url`, `reveal_path` and `open_with_system` passes
through, refuses them with a message that tells the agent to give the
user the link or the file name (`guard::refuse_outside_if_injecting`).
Quill's own launches meet one door, `platform::os_open` (third review):
`open_external_url`, `open_mailto` and `open_local_file` all go through
it and it refuses before anything is spawned, so a link clicked in a
message, an Instant View fallback or a bot's login button cannot reach
the browser; `reveal_in_file_manager` and the two System Settings
openers (push-to-talk and screen-capture permissions) refuse likewise.
Test builds count launches instead of spawning (`os_launch_attempts`),
and `a_link_an_agent_clicks_in_a_granted_chat_launches_nothing` clicks
a URL in a granted chat through the window: refused, nothing launched,
while the user's click on the same spot launches; the file-open confirmation (`FileOpenConfirm`), the
chat export and the account export dialogs are on `HARD_DIALOGS`, and
their entry points (`open_file_guarded`, `open_file_now`,
`confirm_file_open`, `start_chat_export`, `run_chat_export`,
`open_account_export`, `run_account_export`) refuse as well; so does the
device location lookup (`locate_for_request`, and the bot's location
button refuses before its share sheet opens). The clipboard is the
user's: Cmd/Ctrl+V, Cmd/Ctrl+Shift+V and Shift+Insert are forbidden
chords (`keys::forbidden`), and a paste dispatched any other way (the
Edit menu, an input's context menu, a mini app's "Paste from clipboard",
the composer's image paste) reads an empty clipboard, because the
`ZoomPlatform`'s clipboard reads (the find pasteboard included) refuse
while injecting and the two composer paths that read the pasteboard directly refuse too.

Delayed opens run as part of the step. A clicked link runs at render
(`run_pending_link`) and a username or t.me link resolves through TDLib
after the step ended, outside the injecting flag. Each delayed path
carries its origin with it and runs its half under
`guard::with_injecting`:
- the pending link (`PendingLink.from_agent`);
- the deep-link flow: the origin lives with the flow
  (`ChatsState.deep_link_from_agent`, set when a flow starts, kept while
  it continues, taken at every terminal state), and a link queued while
  another resolves is the agent's only if the agent queued that link
  (`LinkUi.agent_pending_link`), so a user's link queued behind an
  agent's flow neither clears nor inherits it
  (`a_flow_keeps_its_origin_when_another_link_arrives`,
  `a_queued_link_is_the_agents_only_if_the_agent_queued_that_link`);
- the Instant View fallback (the request id is remembered as the
  agent's, `instant_view_fallback_from_agent`, and the fallback URL opens
  under that origin);
- the bot login button (`LoginUrlRequest.from_agent`; the dialog refuses
  for an agent's request).

The resolved chat open (`open_deep_link_chat`) re-checks
`agent_may_open` first: an ungranted chat stays closed with a refusal.
Typed links from an agent's step (settings, proxies, folders, sticker
sets, backgrounds) are refused outright.
`a_link_resolved_after_the_step_opens_a_granted_chat_only` covers the
three cases.

The emoji, sticker and GIF panel occludes the composer, so it carries
the chat's region tag (`REGION_CHAT`) itself; without it no point in it
passed. `the_emoji_sticker_and_gif_panels_take_clicks_in_a_granted_chat`
switches each panel's tab through the driver with no refusal; sending
from them stays refused by the send guards.

Sends are routed, not refused. Enter or the Send button in the message
box of a granted chat while injecting goes through `submit_composer` to
`QuillApp::agent_route_send` (`src/ui/agent_driver_send.rs`): it builds
a `sendMessage` for the reply and topic with clean `SendOptions` (the
composer's link preview choices and nothing else: no schedule, silent
flag, effect, paid Stars or suggested-post terms) and hands it over as
`UiSend` instead of sending. The tool thread
(`tools/ui.rs::finish_ui_send`) trusts the window no further than
`not_plain`: anything but a `sendMessage` of `inputMessageText` with no
`scheduling_state`, `suggested_post_info`, effect or Stars is refused
with the draft kept. Then the agent write path: `target::granted`
(read_write, no stranger bots), `common::confirm` ("Confirm each send"
waits for the user's dialog, which the driver cannot click) and
`common::send` (the paced queue, the activity log as `ui_send`). On
success the UI gets `ComposerSent` and clears the box; on a refusal the
draft stays and the reply says why. Edits, forwards, files and formatted
blocks through the driver are refused.

Side effects and the activity log. The driver acts for real: opening or
scrolling a chat marks it read, typing in the message box saves a draft
and shows the other side a typing indicator. The tool descriptions and
the guide say so. Every UI input command writes one activity-log entry
(`log_agent_command`: tool `ui_click`, `ui_type`, `ui_keys`, `ui_scroll`,
`ui_hover`, `ui_menu`, the open chat and a summary like `click "Search"`),
so Settings > Advanced > Agent access shows what agents did in the
window next to what they sent.

The setting. "Let agents drive Quill's window" is a row in Settings >
Advanced > Agent access (`agent_confirm_section`, off by default,
`access::ui_driver`), with one line under it: agents can read, click and
type only in the chats allowed below, and sends still go through the
checks on that page. `quill_status` names it.

### Tools

`read_ui_tree {filter?, role?, limit?}`, `click_element {target, button?,
count?}`, `type_text {text}`, `press_keys {keys}`, `scroll_view {target,
dx?, dy?}`, `hover_element {target}`, `open_menu {target}`
(`src/agent/tools/ui.rs`). A target is `{"node": id}` from the tree or
`{"x", "y"}` in window pixels; node ids are AccessKit's ids in hex, stable
while the element stays on screen. Input tools answer with the focused
node, the nodes that appeared, `refusals` and, for a routed send, `sent`.
`press_keys` presses chords and never types characters.
`quill_status` reports `ui_driver {available, active, paused}`.

### Cross-platform

`dispatch_event`, the accessibility tree, the hitbox tags and the focused
input handler work the same on macOS, Linux and Windows; nothing here is
platform code. Out of reach everywhere: native menus and the macOS menu
bar (those need `cx.dispatch_action`, a later tool), file and folder
pickers, the IME, and system dialogs. A window that draws no frames
(hidden, minimised, on some platforms occluded) answers "did not redraw"
after 1.5 seconds.

### Tests

Window tests in `src/ui/agent_driver_tests.rs` (`demo-capture`,
`ready-chats`): the tree has the granted rows only and a click by id
lands (and is logged); a blocked surface (Settings, then an agent prompt)
answers with the note and a click on Allow by point is refused with the
prompt still waiting; a ctrl-tab that would open an ungranted chat stops
the sequence with no tree; the composer's Enter hands the send to the
agent path with the draft kept, a read-only chat is refused, and
`ComposerSent` clears the box; a tool that gave up gets nothing injected;
the search tabs and the reply chooser leak no ungranted chat
(`agent_driver_redact_tests.rs`); a link resolved after the step opens a
granted chat only; the focused message box carries `region=chat` and a
blurred window carries nothing. Unit tests cover the redaction zones,
the list rule and point rules (`ui_driver::redact`), money by parameters
(`guard::money_is_read_from_the_parameters_not_the_prefix`), the paste
chords (`keys::paste_chords_are_forbidden`), the plain-send check
(`tools::ui::only_a_plain_text_send_passes_the_agent_path`), the surface
notes, the chord policy and the step split.

### Gaps left for step 2 and later

- Pause on real input and the connected strip saying "driving":
  `bridge::note_real_input` and `set_paused` exist; nothing sets the pause
  yet.
- Message rows join the tree only when a screen reader focuses them, so
  history content is read through the chat tools, not the window.
- `focus` is reported only when the focused element itself has a role; the
  kit `Input` registers its focus handle on a wrapper, so a focused text
  box shows no `focused` state (the typed value shows it worked).
- `open_menu` reports the nodes that appeared one frame after the click; a
  menu that animates in over several frames may need a `read_ui_tree`.
- A system screen reader and the driver do not share the window; a user
  who runs both gets the driver refused until the reader disconnects.
