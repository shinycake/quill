# Agent access

Agent access lets AI agents on the user's own computer (Claude Code, Claude
Desktop, Cursor, any MCP client) use their Telegram account through the
Quill that is already running. It ships in six phases behind one switch,
Settings > Agent access, which is off by default. This record
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
(Step 3 changed the last part: an agent can ask for a screenshot, cut by
the same rules as the tree. The tree stays the way to find and target
things.)

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
`quill_status` reports `ui_driver {available, active, paused, stopped}`
(`stopped` came with step 2's Stop, `window` with the live-test fixes).

### Cross-platform

`dispatch_event`, the accessibility tree, the hitbox tags and the focused
input handler work the same on macOS, Linux and Windows; nothing here is
platform code. Out of reach everywhere: native menus and the macOS menu
bar (those need `cx.dispatch_action`, a later tool), file and folder
pickers, the IME, and system dialogs. A window that draws no frames
(hidden, minimised, on some platforms occluded) answered "did not redraw"
after 1.5 seconds; since the live-test fixes it answers at once with the
window's state (below).

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

- Pause on real input: done in step 2 (below). The connected strip still
  says "connected", not "driving"; the takeover pill says it instead.
- Message rows join the tree only when a screen reader focuses them, so
  history content is read through the chat tools, not the window.
- `focus` was reported only when the focused element itself had a role;
  fixed after the live test (below).
- `open_menu` reports the nodes that appeared one frame after the click; a
  menu that animates in over several frames may need a `read_ui_tree`.
- A system screen reader and the driver do not share the window; a user
  who runs both gets the driver refused until the reader disconnects.

## Phase 5 step 2: takeover state

While an agent drives the window, Quill shows it: a glow along the window
edge in the accent colour, a ghost cursor where the agent is about to
click or type, and a pill at the top centre with Stop. Esc, or the user's
own mouse or keyboard, pauses the agent and hands the window back.

### One record

`quill::agent::ui_driver::takeover` holds the facts (the session: agent,
start, last command, end, last aimed action; the pause and its reason;
Stop; when the user last let it continue; the last real pointer position)
and derives the phase from them on every read, with the current clock and
whether an agent is connected and allowed:

- `Idle`: no session, or the last one has faded out.
- `Driving`: from the first input command (click, type, keys, scroll,
  hover, menu; reads such as `read_ui_tree` start and extend nothing), for
  as long as they keep coming (each one extends a 4 s hold) and the agent
  stays connected with access and the driving switch on.
- `Paused`: Esc or real input while driving. The hold does not run out
  while paused. An agent that disconnects fades the layer out, but the
  pause stays: when an agent connects again the paused pill fades back in
  and every UI tool still refuses until the user presses "Let it
  continue". Only the switches going off forget it.
- `FadingOut`: the hold ran out, the agent disconnected, the switch went
  off, or the user pressed Stop. 600 ms, starting from however far the
  240 ms fade-in had got.

The window keeps no copy. The bridge's pause flag from step 1 became
`takeover::pause`/`let_continue`, and `policy::switch_refusal` ends with
`takeover::refusal()`, so the MCP tools, the per-step checks and the
overlay read the same record. Nothing in it is persisted; a restart
starts idle.

### Pause and hand back

The window's capture-phase input probe (`presence::input_probe`) passes
every real event to `takeover::note_real_input`; injected input is marked
by `guard::injecting` and never counts. While driving:

- a press, a scroll or a key pauses at once;
- Esc pauses through a keystroke interceptor registered before the app's
  others and is consumed, so it does not also close the open chat or a
  dialog (once paused or idle, Esc does its normal job again);
- a pointer move pauses only when the window is active, the pointer moved
  at least 4 px, and 1.5 s have passed since "Let it continue". Crossing
  the window on the way to another app, trackpad jitter and the hand
  leaving the button it just clicked do not pause;
- pointer input on the pill answers the pill. The pill records its painted
  bounds each frame, and presses, moves and scrolls inside them never
  pause, so pressing Stop refuses queued commands with the Stop message
  rather than the pause one;
- input while one of the agent's own prompts is open answers the prompt:
  its request or confirm (`AgentRequest`, including "Confirm each send"
  and `td_call` asks) and the all-chats consent. They are modal, so all
  input goes to them; the takeover layer records each frame whether one
  is open, and the probe then treats every event like one on the pill.
  Answering one starts the same 1.5 s pointer grace as "Let it continue",
  and Esc on one denies it instead of pausing. (Recording the dialog's
  bounds like the pill's was the other choice; a modal dialog makes the
  whole window its surface, so the open flag is simpler and exact.)
  The flag counts only a prompt the user can see: a request waiting
  behind a higher dialog (Scheduled, Archive settings) answers no input,
  so a press there still pauses. While any request waits, shown or not,
  the driver's surface is blocked, so an agent cannot hide its own
  prompt behind a dialog it opened and keep driving that dialog.

Without the last rule, confirming a send the agent typed paused it: the
user's move to Confirm or the press on it counted as taking the window
back, and the `ComposerSent` that clears the box after the send was then
refused, so the text stayed and the next Enter sent it twice.
`ComposerSent` is also answered before the switch and pause checks now:
it is cleanup of a send the user confirmed, and still clears the box only
when the same chat is open with the same text, after a pause or a Stop
too.

While paused every UI tool answers "The user took control of Quill, so the
UI driver is paused. Wait, then call quill_status to see when you can
drive again (ui_driver.paused turns false)." "Let it continue" unpauses.
Both pill buttons refuse injected clicks (`guard::refuse_if_injecting`),
so an agent cannot continue or stop itself through the window. A read
that was waiting for its frame when the user paused checks the pause
again once the frame came and refuses; the answer after an input command
does the same, so no tree drawn after the user took over reaches the
agent.

### Stop

Stop refuses the running command and every queued one
(`bridge::refuse_all`), ends the session with the fade, and keeps the UI
tools refused ("The user pressed Stop in Quill: don't drive its window
again...") until the user turns "Let agents drive Quill's window" (or
Agent access) off and on again, or restarts Quill. A disconnect and
reconnect does not lift it: an agent with a shell could otherwise kill
its `quill mcp`, start another and drive on. `quill_status` reports
`ui_driver.stopped` and `resumes_when` (what lifts a Stop or a pause).
Agent access stays on and the chat tools keep working. There is no "allow
again" in the pill on purpose: the user brings driving back with the
switch, not with a button an agent could ask them to press. The Stop
button's tooltip says so.

### Refusals after the step

Some of a step's effects run later under `guard::with_injecting`: a link
run at render, a bot login button's answer, a deep link TDLib resolved,
an Instant View fallback. While the input command is still open (until it
answers, `guard::OpenCommand`), what they refuse goes into that command's
reply. After it answered nobody waits for them, and the thread-local list
would hand them to the next command's reply, as if that step had been
refused. They go to the activity log instead, as declined entries under
the latest step's agent and tool (`ui_click`...).

### The overlay

`src/ui/agent_takeover.rs` draws one deferred layer at priority 300, above
popups (100) and tooltips (200), built in `QuillApp`'s root render. It has
no hitbox except the pill's (`occlude`), so a click anywhere else lands on
what is underneath; a test clicks a chat row under the glow and the ghost,
and the row opens.

- Glow: an inset shadow along the window edge plus a 2 px border in
  `theme.primary`, rounded like the window (16 px on macOS, 8 px
  elsewhere), with a light sweeping round it while the agent drives (see
  "The glow"). Theme tokens only, so it is the accent in light and dark.
- Ghost cursor: a ring with Lucide's `mouse-pointer-2`, its tip on the
  target. Before each pointer step the driver aims (`input::aim_step`) and
  waits 220 ms while the ghost glides there from its last spot on an
  ease-out curve; the click sends a ring outwards for 380 ms. For typing
  it shows a keyboard badge at the focused input's caret, read through a
  small GPUI patch (`Window::focused_text_caret`, in `QUILL-CHANGES.md`).
  The glide adds about a quarter of a second to each pointer step; typing
  and keys add nothing.
- Pill: `popover` background, border and shadow. Driving: a live dot,
  "{agent} is driving" and a ghost `Button` "Stop" whose tooltip says
  driving stays off until the switch is turned off and on, and access
  stays on. Paused: a hand icon, "Paused", "{agent} will wait until you
  let it continue", a primary "Let it continue" and "Stop". Fading out:
  the label only. Strings are in `lang/en/agent_access.strings`. Its
  hitbox is the pill alone: `occlude` sits on the pill, not on the
  full-width row that centres it, so the title bar either side still
  drags the window; only the pill's own 32 px tall box covers it.

### The glow

There is one look: the edge (a soft inset bloom and a 2 px accent border)
with a band of light, 340 px long and 5 px thick, that runs round the edge
once every 5 s while the agent drives. While it sweeps the border dims to
55% so the light stands out; still, it is at 85%. The light's core is the
accent lifted towards white on a dark theme, and a deeper, more saturated
accent on a light one (a lighter core reads as a gap in the border on a
white window). A tight halo and a wide bloom in the accent carry it into
the window in both. The first captures used the plain accent on light and
the light barely showed against the border; the dimmer border and deeper
core fixed that.

The first round of this phase offered three looks behind
`QUILL_AGENT_GLOW` (still edge, a breathing glow, the sweep) and made the
still edge the default. The owner picked the edge with the sweep after
watching the clips: a window that moves on its own is easy to miss when
the frame holds still, and the light moving round it says "something is
acting here" in a way a border does not. The breathing look and the
variable are gone; one demo kind records the look.

The sweep shows only that an agent is acting, so it runs only then:

- Paused: the light holds where it was, beside the "Paused" pill.
- Stop or a disconnect: the border and the light fade out together.
- Hidden window (minimised, on another space, fully covered where the OS
  reports it): no frames; the light picks up where the clock says when
  the window shows again.

"Takeover effect", under "Let agents drive Quill's window" in Settings >
Agent access, chooses whether it moves:

| Takeover effect | Sweeps | Holds still |
| --- | --- | --- |
| Automatic (default) | otherwise | reduced motion, Power Saving or the system's low-power mode |
| Glow and sweep | otherwise | reduced motion (the hint says so) |
| Still glow | never | always |

The signals:

- Reduced motion: the system's, or `QUILL_REDUCED_MOTION`.
- Power Saving: Quill's Power Saving for interface animations, which the
  system's battery saver or heat turn on unless "Save Power on Low
  Battery" is off.
- Low-power mode: the system's own, read by `battery_saving`. That is
  macOS Low Power Mode, the Windows battery saver, or Linux's
  power-saver profile through the portal or power-profiles-daemon.
  Unknown counts as not saving.

`agent_takeover_glow::sweep_allowed` is the one place that combines them.
The layer reads it on every render, so a change shows on the next frame.
The battery watcher, which otherwise parks while nothing listens, keeps
polling every 30 s while a takeover shows, so turning Low Power Mode on
stops the sweep within that time. The preference lives in `access.rs`
next to the driver switch (an old settings file reads as Automatic) and
is hard-blocked from the driver like the switch itself.

Why the design guide's Motion rules still hold:

- The light marks a state (an agent is acting now) and stops the moment
  that state ends or pauses. It is not decoration on an idle window.
- Reduced motion always wins.
- Nothing depends on it: the pill and the still border carry the same
  state.

### Frame cost

Animation runs on the existing frame clock only. The layer asks for frames
at 30 fps while `takeover::moving` says something moves (a fade, a glide,
a press pulse), and at 15 fps while the light sweeps. It uses the clock's
background variant so the takeover keeps animating while Quill sits
behind the terminal the agent runs in. Otherwise it sets one timer for the
next phase change (the hold running out, the fade ending) and draws
nothing in between. With no agent driving the layer is not built; the
cost is one mutex read per render.

GPUI redraws the whole window on each frame, and while an agent drives the
element tree is forced on, so slices draw uncached (see "Cost of forcing
the tree"). Each sweep frame therefore costs a full window draw, and the
only levers are how many frames there are and when. Caching `QuillApp`'s
root so a frame repaints only the layer would cut the per-frame cost, but
it touches every refresh path and the forced tree, so it was left out.
The levers used:

- 15 frames a second instead of 30, with a 5 s lap: about 48 px a frame on
  a 1100x720 window, a seventh of the light's length, so its soft ends
  still read as gliding (the clip shows the real rate).
- Frames only while the agent drives: none while paused, fading or
  saving, none when the window is hidden.

Measured 2026-10-25 on an M1 Pro (macOS), release build, 1100x720 window,
the same demos as the first round: `ready-agent-steady` (driving, faded
in, only the sweep moving), `ready-agent-still` (the same with Power
Saving on), `ready-agent-paused`, and `ready-chats` with no agent. Each
run waited 8 s and then measured 20 s. CPU is a share of one core.

The first round's method (`ps` %cpu sampled once a second) gave two
readings about 2.3 times apart from run to run on this machine, at every
rate. Exact CPU time showed the same thing, so it is not sampling noise:
the same frames take more time when macOS runs them on the efficiency
cores at low clocks. A share of CPU time is therefore only comparable
within one session. This round also reads the process's instructions,
cycles and energy from `proc_pid_rusage`; instructions do not depend on
which core ran them. The sweep rows are the mean of three runs, all in
the same session and the same (slower) mode; the other rows are one
`proc_pid_rusage` run, with three CPU-time runs agreeing.

| Window | CPU | Instructions | Energy | RSS |
| --- | --- | --- | --- | --- |
| No agent (`ready-chats`) | 0.0% | 0 | 0 mW | 119.6 MB |
| Driving, sweep at 30 fps (the first round's rate) | 27.0% | 693 M/s | 82.7 mW | 120.9 MB |
| Driving, sweep at 20 fps | 20.0% | 469 M/s | 52.7 mW | 120.7 MB |
| **Driving, sweep at 15 fps (shipped)** | **15.5%** | **353 M/s** | **40.3 mW** | 120.8 MB |
| Driving, sweep at 10 fps | 11.3% | 238 M/s | 25.2 mW | 120.7 MB |
| Driving, still glow (Power Saving) | 0.5% | 11 M/s | 1.7 mW | 120.3 MB |
| Paused | 0.0% | 0 | 0 mW | 119.9 MB |

Every sweep frame costs the same, about 23.5 M instructions, so the cost
follows the rate: 15 fps is half the first round's 30 fps by every
measure. In the faster mode seen in other runs of the same session, the
CPU column read 5.3% at 15 fps, 7.2% at 20 fps and 10.2% at 30 fps. The
first round's 9.3% for the 30 fps sweep was taken in that mode, so like
for like the sweep now costs about 5% of a core where it cost 9.3%.

The still glow's 0.5% is the demo's pinned clock: the hold never runs
out, so the wake-up timer fires every few seconds and draws a frame. In
the app the hold ends and nothing draws. Fades, glides and pulses still
cost 30 fps for their 240 to 600 ms. Memory is the same within noise.

Scripts (scratchpad, not in the repo): `p5b-rusage.sh` and
`p5b-rusage-rates.sh` start each demo with `QUILL_DEMO_LINGER_MS=40000`
and read `proc_pid_rusage(RUSAGE_INFO_V6)` before and after the 20 s.
The rates came from one build with a temporary `QUILL_TMP_SWEEP_FPS`
override, removed before commit.

### Tests

- Core (`takeover_tests.rs`): driving lasts while commands come and fades
  after the hold; real input pauses and "Let it continue" hands back;
  pointer moves pause only when deliberate; answering a prompt gives the
  pointer the grace; Stop ends the session at once and holds through a
  disconnect and a reconnect until the switch goes off; a pause outlives a
  reconnect and its pill fades back in; a disconnect or the switch fades
  out; an interrupted fade-in carries on from the value reached; the
  glide's ease and the pulse; what moves and when the next change is due;
  the global record's refusal texts.
- Window (`agent_takeover_tests.rs`, `demo-capture`): a real click pauses,
  "Let it continue" hands back, the hold and a disconnect fade the layer
  out; Esc pauses and the open chat stays open; Stop refuses a queued
  command and a later one with the Stop text while access stays on; a
  reconnect lifts neither a pause nor Stop, the switch off and on lifts
  Stop; a click under the glow and the ghost reaches the chat row; the
  frame clock runs while fading in and stops when nothing moves and when
  idle; the sweep keeps the clock going under each "Takeover effect" and
  saving signal exactly as the table in "The glow" says, switching live,
  and holds still while paused.
- Confirmed sends (`agent_driver_confirm_tests.rs`): the agent types and
  presses Enter, "Confirm each send" opens, a real mouse move and press on
  Confirm answers it without pausing (and the hand leaving the button
  after does not pause either), then `ComposerSent` clears the box only
  for the same text; Stop while the confirm is open leaves the confirm up,
  a confirmed send still clears the box, and Stop holds; a read filed
  before a pause is refused once its frame comes.
- Guard (`guard.rs`): a deferred half's refusals go to the open command's
  reply, and after it answered to the activity log; the window test of a
  deep link resolved after the step checks the same in the app.
- Glow (`agent_takeover_glow.rs`, `access.rs`): the still border has no
  light and the sweep laps; the light is brighter than the accent; the
  sweep wraps round the corners; the three effects against reduced motion
  and power saving; an old settings file reads as Automatic.

Screenshot demos: `ready-agent-driving` (light along the top edge),
`ready-agent-steady`, `ready-agent-still` (Power Saving on: the still
border), `ready-agent-glide`, `ready-agent-typing` and
`ready-agent-paused`; `ready-settings-agent-activity` shows the "Takeover
effect" row. With `QUILL_DEMO_TAKEOVER_FRAMES=<dir>`,
`ready-agent-driving` records the scripted session as one PNG per sweep tick
of takeover time (15 a second).

### Cross-platform

None of this is platform code: the deferred layer, shadows and gradients
(Metal and wgpu), the input probe, keystroke interceptors and the input
handler's selected bounds work the same on macOS, Linux and Windows. Only
the corner radius differs, as a constant per OS rather than a value read
from the window. The sweep's signals come from code that already has a
backend per OS: `battery_saving` (macOS `NSProcessInfo`, Windows
`GetSystemPowerStatus`, Linux portal or power-profiles-daemon) and GPUI's
window visibility (see Gaps for what each OS reports).

### Gaps

- Checked in screenshot demos and window tests only. No real agent has
  driven it with a real mouse and keyboard yet, so the pointer-move rule
  (4 px, active window, 1.5 s grace) needs a check on real hardware.
- The corner radius is a per-OS guess; a Linux window with square or
  client-drawn corners shows the border slightly inset at the corners.
- On a dark theme the ghost has less contrast than on light. It reads,
  but a halo in `background` may help.
- The typing badge needs the focused input to report selected bounds; an
  input that does not shows no badge (typing still works).
- The connected strip still says "connected", not "driving".
- The system's reduce-motion setting is read once at startup (as it is
  for the rest of Quill), so changing it while Quill runs reaches the
  sweep only after a restart. Power Saving, low-power mode and the
  preference switch live.
- Low-power mode reaches the sweep within the battery watcher's 30 s
  poll, not at once.
- Windows keeps compositing a fully covered window and X11 compositors
  never report one as obscured, so there a covered window keeps drawing
  the sweep; minimised windows stop on every OS.
- The sweep's visibility was checked in captures on macOS only; Linux
  (wgpu) and Windows need a look on real hardware.

## Phase 5 step 3: screenshots

Owner ask: after each step the agent gets back a screenshot and the
updated tree, on all three platforms, and Quill's own threads can use it
for visual verification. `take_screenshot {full_resolution?}` answers with
the tree of the frame and a PNG; every input tool takes
`"screenshot": true` (and `full_resolution`) and adds a PNG of the frame
after the input to its usual answer. The image goes back as an MCP image
block (`{"type":"image","data":<base64>,"mimeType":"image/png"}`) after the
text block, and the data carries `screenshot {width, height, scale,
scope}`, where `scope` is `granted` or `all_chats` and `scale` is image
pixels per window pixel (a point in the image divided by it is a point
`click_element` takes). The long side is shrunk to 1280 unless
`full_resolution`.

### Capture: the window renders its own frame

Each platform's renderer already had, or could get, an offscreen path:
GPUI's `render_to_image` exists for tests on Metal and Direct3D 11.
Quill builds it in every configuration and adds
`PlatformWindow::capture_image(&Scene)`, which renders the last drawn scene
to an offscreen texture with the window's own renderer and reads it back:

| Platform | Path | Fallback |
| --- | --- | --- |
| macOS | `MetalRenderer::capture_scene`: `render_scene_to_image` at the layer's drawable size. No drawable is taken from the layer and nothing is presented. | A window without a layer answers "the window has no layer to capture". |
| Windows | `DirectXRenderer::render_to_image` (the back buffer, staging copy). | An HDR (scRGB) swap chain refuses readback; the agent gets "Quill could not capture its window (...)". A render target with more than one sample would too (see below). |
| Linux X11 and Wayland | `WgpuRenderer::capture_scene`: an offscreen texture in the surface format and the surface's alpha mode, copied to a mapped buffer, 5 s timeouts. | A surface format other than BGRA8 or RGBA8 answers with an error. |
| Others, and builds without the UI | `capture_image`'s default body: "this platform's renderer cannot capture the window". | `quill_status` reports `ui_driver.screenshots.available` false. |

Rejected: ScreenCaptureKit, `CGWindowListCreateImage`, `PrintWindow` /
`Windows.Graphics.Capture`, the XDG screenshot portal and X11 `GetImage`.
They read the screen, so they need a screen-recording permission (macOS
asks for it, the portal asks every time), they capture whatever covers the
window, and they can only blank a region after the fact, from rectangles,
with no way to hide something Quill drew over it. Rendering the scene
again needs no permission, never shows another app, and runs only when an
agent asks: nothing is built, allocated or scheduled until then. It costs
the frames no time either: the paint filter is one `Option` check per
tagged element while unset.

`interface_zoom.rs`'s `ZoomWindow` forwards the new method (its
`forwards_every_trait_method` test caught the gap).

Details that matter for the pixels:

- Every readback is premultiplied, and the flatten onto the grey assumes
  it. Metal and Direct3D blend straight alpha (`SourceAlpha`,
  `OneMinusSourceAlpha`) onto a transparent target, which leaves the
  colour multiplied by coverage. wgpu builds its pipelines for the
  surface's alpha mode, so the capture renders in that mode too (it used
  to pass "straight" always, which was wrong on a premultiplied surface):
  the opaque mode premultiplies through the blend, the premultiplied mode
  already is.
- Direct3D copies the back buffer into a one-sample staging texture with
  `CopyResource`, which needs matching sample counts. The back buffer of
  a flip-model swap chain always has one sample (the 4x MSAA texture is
  path rasterization's own and is resolved before it reaches the target),
  so no `ResolveSubresource` is needed; `render_to_image` checks the
  count and refuses with an error rather than copy if that ever changes.
- Metal waits for the capture with `wait_until_completed`, which has no
  timeout, as upstream's present already waits on every frame
  (`wait_until_scheduled`). wgpu has its own 5 s timeouts.
- Memory at full resolution: the encode shrinks the readback first (it is
  premultiplied, which is what `resize` filters, so the picture is the
  same) and drops the full-size copy, then flattens to RGB in place in
  the remaining buffer. Before, the flatten made a second full-size
  buffer and the shrink a third.

### Privacy: the paint filter

A screenshot must never show what the tree hides. Rectangles from the
tree would miss painted things that have no role (a chat's background
rows, a badge) and anything drawn over a hidden area by another element.
So the frame is cut where it is painted, by the rule that cuts clicks:

- GPUI gains `Window::set_paint_filter(Option<PaintFilter>)`. While set,
  each tagged element's paint asks the filter, with the stack of
  `a11y_tag`s above it (regions, parts, lists, `chat=<id>` rows), whether
  it reaches the scene; the scene drops what is refused. An element's tag
  now spans its own background and border, so a refused row loses its
  background too.
- Quill's filter is `redact::point_allowed`, the function that decides a
  click, with the grants and chat classes read once per screenshot. In the
  granted scope the window starts covered in a neutral grey (`#8a8a8a`),
  the allowed parts (the open granted chat, the search box and folders, a
  granted chat's row) are drawn on the theme's background, and an
  ungranted chat's row is grey. With "All other chats" on, everything is
  drawn except secret chats without their own grant, which are covered.
- It fails closed. Cached paint is not replayed under a filter (it was
  never filtered), so every view renders again; an element that carries a
  tag but does not push it is drawn as its parent decides; a deferred draw
  (toast, popover) carries only its own tags, so in the granted scope an
  untagged one is left out.
- What is opened on a row is cut with the row. A chat's context menu
  (chat list and the search's top people), its stories menu and its peek
  preview carry `chat=<id>` as well as their own tags, so over an
  ungranted chat (or, with All chats on, a secret chat without its own
  grant) they are neither in the tree nor in the picture. The archive
  menu carries `archive` inside its menu region, so the granted scope
  cuts it with the "Archived chats" row. A tooltip is drawn at the root
  after everything else, where no tags are above it, so GPUI now records
  the tags its owner was painted under and paints the tooltip under the
  same tags. A drag's picture follows the pointer at the root too and is
  not tied to its row, so no screenshot is taken while the user drags
  something ("The user is dragging something...").
- The surface check is the tree's: a blocked surface (Settings, a request
  to the user, sign-in, payments...) gets the same note as `read_ui_tree`
  and no frame is drawn. The tree in the reply is read right after the
  filtered frame and must name the same surface. That re-check is
  load-bearing: `region=chat` allows whatever chat is open, granted or
  not, so a chat that opened between the check and the frame (a click
  the user made, a link) would be drawn in full if the surface were not
  read again from the same frame. While the user has
  taken control or pressed Stop, no screenshot is taken, the one after an
  input included: the window is theirs.
- The takeover overlay is left out (`takeover_layer` returns nothing while
  the screenshot's frame is drawn). A normal frame is drawn again before
  anything is presented, so the user never sees the filtered one.

Each screenshot is one line in the activity log (`ui_screenshot`,
"Took a screenshot of the window", the open chat). Screenshots are limited
to one a second and 20 a minute across agents
(`quill::agent::ui_driver::shot::Limiter`); over the limit,
`take_screenshot` answers with how long to wait and an input tool runs
without the image and says why. The guide says screenshots are for
operating Quill, not for archiving chats.

### Cost

Before: nothing (no screenshots). Unasked, a frame pays one `Option`
check per tagged element. Asked, measured with the `ready-agent-screenshot`
demo (2400x1480 device pixels, full resolution, this Mac, 2026-10-25, two
runs each of the granted and All chats scopes; the timing lines were
temporary and are not in the code):

| Build | Draw + readback (UI thread) | PNG encode (background) | Answer |
| --- | --- | --- | --- |
| Debug | 206 ms | 640 to 740 ms | 880 to 980 ms |
| Release (LTO off, 16 codegen units) | 22 to 27 ms | 8 to 10 ms | 44 to 56 ms |

The UI-thread part is two extra frames (the filtered one and the normal
one after it) plus the readback; the encode runs on the background
executor. The default 1280-px image is smaller and encodes faster.

### Tests

- `agent_driver_shot_tests.rs` (window tests; the test platform has no
  GPU, so its "readback" paints the frame's solid quads on the CPU):
  ungranted rows hidden and the granted row drawn, with All chats on
  nothing covered, the default size and scale, three log lines; a blocked
  surface gets no screenshot and no log line; an input with a screenshot
  answers with the frame after it; the takeover pill is in the window and
  not in the screenshot; a refused row under a drawn container is covered
  and none of its paint is drawn; a chat menu over an ungranted row is
  absent from the tree and painted over, over a granted row it shows; with
  All chats on, the peek of an ungranted secret chat is cut and another
  chat's peek is drawn; a tooltip over a refused row is hidden in the
  filtered frame and one over an allowed row is drawn; no screenshot
  while the user drags the chat list's edge, one again after; the encode
  flattens a premultiplied readback (transparent, opaque, half-covered)
  and shrinks before it, as an RGB PNG of the asked size.
- `redact.rs`: a menu inside a granted or ungranted `chat=` row, in both
  scopes, and the archive menu's tag order.
- `shot.rs`: the 1280-px shrink, one a second, 20 a minute.
- `tools::ui_shot`: the schema options, the image block, the refusal with
  the driver off.
- Visual check with the demo: `QUILL_DEMO_SHOT_SCOPE=granted|everything
  QUILL_DEMO_SHOT_OUT=<file> quill --screenshot-demo
  ready-agent-screenshot <dir>`.

### Gaps

- Checked on macOS only. Windows (Direct3D) and Linux (wgpu) are
  type-checked from macOS; the readback has not run on either. The
  `quill-run` cloud skill can run the demo on Linux with lavapipe.
- A part the filter shows inside a refused one sits on the theme's
  background, not its real one (the chat list's tint is lost around a
  granted row).
- Text and images are drawn by the renderer like everything else, so the
  screenshot matches the screen. Video surfaces and webviews were not
  checked.
- Other popovers opened from a chat-related element (a message's menu in
  the open chat, the folder tab menu) carry only the menu tag. The open
  chat passed the surface check, so its menu is allowed; a folder's menu
  names the folder, not a chat.
- The limit is per process, not per agent; one busy agent can use the
  minute's budget for all.
- A deferred refusal (IV fallback, login answer, deep-link open) goes to
  whichever input command is open when it fires. The bridge runs one
  command at a time, so this is the command that caused it; two at once
  would credit the newer one.

## Phase 5: fixes from a live test

A fresh agent drove a release build through the MCP tools and found three
things the window tests had not.

### Hard surfaces never open

`press_keys "cmd-,"` opened Settings in the real window. The surface check
then refused the next step ("Quill is showing Settings"), but Settings
stayed open for the user to close. The cause: the surface is checked
before each step, and the chord itself reached `OpenSettings`
(`src/ui/app_render/mod.rs`), which opened Settings without asking whether
the driver was injecting. The chord list (`keys::forbidden`) only knows
quit, close, hide, minimise and paste, and could never be complete: every
shortcut can be rebound, and the Help menu's Keyboard Shortcuts, a main
menu entry or the in-window menu bar on Linux and Windows reach the same
handlers by click.

So the handlers refuse, whatever the route
(`guard::refuse_hard_if_injecting(HardSurface::…)`, a thin wrapper over
`refuse_if_injecting` whose enum is the list of what agents never open):

| Handler | Routes | Surface |
| --- | --- | --- |
| `open_settings` | `OpenSettings` (Cmd/Ctrl+,, app menu), `OpenShortcuts` (Help menu), the sidebar and main menu, every settings page opener (appearance, privacy, notifications, language, data, folders, proxy, sessions) | Settings, Agent access and log out included |
| `lock_by_passcode` | `LockApp` (Cmd/Ctrl+L, app menu), the lock button | the lock screen |
| `navigate` | every main-menu entry that stays in Settings, and Stars, Premium, subscriptions, gifts, send gift | Settings, passcode, security, account, payments |
| `open_passcode` | Settings > Privacy, main menu | passcode settings |
| `open_twofa`, `open_websites` | Settings > Privacy, main menu | two-step verification, logged-in websites |
| `open_sessions` | Settings > Devices, main menu | active sessions |
| `open_account_lifecycle`, `open_accounts` | main menu, account switcher | log out, delete account, accounts |
| `open_stars`, `open_premium`, `open_subscriptions`, `open_send_gift_in`, `start_invoice_checkout` | main menu, profiles, Premium badges, invoice messages and links | payments, Stars, Premium, gifts |
| `QuitApp`, `CloseWindow`, `MinimizeWindow` | Cmd/Ctrl+Q/W/M, File and Window menus | the window itself (Ctrl+M minimises on Linux and Windows, which `keys::forbidden` did not name) |
| `ToggleTheme` | View menu | a settings change |

The agent prompts are opened by tool threads, never by input, and stay
blocked by the surface check. Every other keybinding (chat and folder
navigation, search, history scrolling, selection, formatting, viewer keys)
opens nothing on the hard list; a chat it would open goes through
`agent_may_open`, a write it would send through `screen_request`. The
drawn title bar carries no region tag, so a point on it fails
`point_allowed` in the granted scope; in the Everything scope its close
button refuses too (`FrameTitleBar` in `src/ui/window_frame.rs` wraps the
callback), and its minimise and maximise buttons are kept off by a tag
(below, "Review fixes").

Tests: `agent_driver_hard_tests.rs`. An agent presses Cmd+, and Ctrl+,
(Settings stays closed, the reply carries "open Settings"), Cmd+L and
Ctrl+L with a passcode set (Quill stays unlocked) and Ctrl+M (refused);
the user's own Cmd+, still opens Settings. A second test runs the main
menu entries and the openers under the injecting flag: nothing opens, each
refusal is recorded, and the same entry opens for the user. In a real
window, `QUILL_DEMO_SHOT_KEYS="cmd-, cmd-l"` on the `ready-agent-screenshot`
demo has the agent press both chords in the message box: the reply lists
"open Settings" and "lock Quill", and the captured window still shows the
chat with the message box focused.

### The focused text box is reported

`focus` was null in every tree, even right after clicking the message box
and typing into it. The tree's focus comes from GPUI
(`A11yTreeSnapshot::focus`, read in `src/ui/agent_driver_tree.rs`), and
GPUI reported a node as focused only if the element that tracks the
focused handle pushed a node itself. gpui-component's `Input` puts the
`MultilineTextInput` role on its frame (`InputBase`, which tracks a frame
focus handle of its own), while the editor inside (`input-state` in
gpui-base, an id and no role) holds the focus, so `A11y::set_focus` found
no node and logged "it has an id but no role". The same held for every
kit text box, and for screen readers, which announced the whole window.

The fix is in the vendored `gpui-pre` (`third_party/gpui-pre/QUILL-CHANGES.md`):
a focused element with no node of its own reports the nearest enclosing
node that registered a focus handle in the same frame, unless one already
holds the focus. The text box's frame is that node. Nothing in Quill
changed; the driver, the redaction (a focused node cut from the tree
reports no focus) and the screenshot's tree read the same field.

Test: `the_clicked_message_box_is_the_focused_node`
(`agent_driver_tests.rs`) clicks the message box and checks that the
click's reply, the reply after `type_text` and a fresh `read_ui_tree` all
name it as `focus` with the `focused` state, and that blurring the window
drops it. It fails without the `gpui-pre` change (`focus` is `None`).

### The window's state is reported

With Quill's window behind other windows, every UI call answered "Quill's
window did not redraw in time. Make sure it is open and visible", while
`quill_status` gave no hint. On macOS an occluded window gets no frames
(the display link stops; GPUI's `WindowVisibility` is `Hidden`), so the
driver's wait for a frame (`wait_for_tree`, 90 polls of 16 ms) always ran
out, and nothing told the agent why.

One reading, `agent_driver::window_state` (`src/ui/agent_driver.rs`),
decides the state from the window itself: `minimised` when
`window_control::is_minimised` says so (`IsIconic` on Windows,
`isMiniaturized` on macOS, ICCCM `WM_STATE` = `IconicState` on X11, never
on Wayland, where a client cannot read it), otherwise `visible` or
`hidden` from `Window::is_visible`. Each platform reports visibility as far
as it can (GPUI's own notes: Windows and compositing X11 window managers
keep a covered window visible, and draw it). It feeds three places:

- `UiCommand::State` carries it as `UiState::window`, and `quill_status`
  asks for it (1.5 s; no answer in time is `not answering`: the window
  may be there with its thread busy) and reports `ui_driver.window` as
  `visible`, `hidden`, `minimised`, `not open` or `not answering`, with a
  line in the text when it is not visible. It is null while the UI
  answers no driver command (the driver off, paused or stopped), so the
  status never reveals more than a command would.
- Every other command answers at once when the window is not visible,
  before any frame is waited for or input injected
  (`policy::no_frame(state)`: "Quill's window is hidden (behind other
  windows, on another desktop or on a sleeping display), so it draws no
  frames and agents can't drive it. Ask the user to bring it to the front,
  then retry."), since GPUI documents that the platform requests no frames
  for a hidden window.
- A frame that still does not come (the window is visible but stuck, or
  went away mid-command) answers with the state read at that moment; a
  window that is gone is `not open`.

Tests: `a_hidden_window_says_so_in_its_state_and_its_missing_frame`
(`agent_driver_tests.rs`) hides the test window
(`simulate_visibility_change`): `State` says `hidden`, a read and a click
are refused with "is hidden" and nothing is logged, and shown again the
state is `visible` and the tree reads. `policy` tests the wording for each
state. The minimised readings are platform calls the test platform does
not have; none of them ran, and only the macOS one was compiled here.

### Review fixes

A review of the three fixes above found openers the handler guards had
missed, and gaps around the window itself.

**Dialogs that open from state.** About twenty openers had no guard
(`open_paid_box`, the gift transfer, resale, value, auction and code
boxes, the giveaway form, the suggested-post price, the live story Stars
box, Grams, mini app invoices, the remove-fee box, the paid price, Earn,
Money, the Stars gift box, a payment receipt, the boost extras), so an
agent's click left a hard dialog on screen. Guarding them one by one
would miss the next one. Quill's kit dialogs do not open through one call:
the shell derives the dialog from the app's open flags on every render
(`QuillShell::sync_kit_dialogs_in`, the registry built by
`register_dialogs!`), so there is no `open_dialog` call to refuse. The
open state is derived instead, at the one place every injected input
passes:

- `perform_step` (`src/ui/agent_driver.rs`) reads which hard dialogs
  (`HARD_DIALOGS`) and hard layers (`HARD_LAYERS`) are open before the
  step, and again after it, still under the injecting flag
  (`agent_driver_surface::block_opened_by_agent`). A dialog or layer that
  became open is recorded as a refusal (`guard::refuse_hard_dialog`: the
  step's reply carries "Quill closed a dialog an agent's input opened"),
  and queued.
- The deferred halves of a step (a clicked link run at render, a deep
  link TDLib resolved later, the instant view fallback) run through
  `QuillApp::agent_step`, which does the same around
  `guard::with_injecting`; after the command answered, the refusal goes to
  the activity log.
- The shell, before it picks the dialog to show, closes every queued one
  with the dialog's own close (`DialogSpec::closes`, the clear its builder
  hands `on_close_kind`; a layer with its Escape close), repeating up to
  eight times for one that steps back. No frame runs between the step and
  that render, so the dialog never shows. A test requires a close for
  every hard dialog except the agent prompts, which tool threads open and
  input never does.

Only a dialog that went from closed to open during the agent's own input
counts: a call ringing or an agent prompt arriving in the same frame is
not the step's, and the user's input never runs under the flag. The
handler guards stay: their refusals say what was refused ("open
Settings", "lock Quill"), and they cover what is not a dialog (Settings,
the lock, quitting).

**Calls and accounts.** Answering a call from the `BackgroundCall`
dialog switched to the ringing account with no guard. `switch_account`
(`src/ui/accounts.rs`) now refuses with `HardSurface::Account` on every
route (the switcher, the main menu, the tray, a notification), and
`answer_background_call` refuses before it touches the ring ("answer
calls for the user"). `BackgroundCall` is a hard dialog: a ringing call
blocks driving in any scope.

**The window.** `ZoomWindow` and `ToggleFullscreen` (F11, Cmd+Ctrl+F, the
Window menu) refuse with `HardSurface::Window`, and so do the viewer's
video full screen and the group call stage when they would make the
window full screen (leaving full screen stays allowed). The kit's
`TitleBar` calls `minimize_window` and `zoom_window` itself, with no
callback to wrap (only close has one), and its drag area takes a double
click (maximise, or the macOS title-bar action) and, on Linux, a right
click (the window manager's menu). `FrameTitleBar` wraps the bar in a
`window-chrome` tag, and the app's own parts of it (the menu bar, the
search button) in `window-chrome-content`. `redact::on_window_chrome`
says a point's topmost chrome tag is `window-chrome`, and the pointer and
focus checks refuse it in every scope (input only: `point_allowed` also
decides the screenshot's paint filter, which keeps showing the bar), and `agent_pointer_allowed` lets only a single left click, a hover
or a scroll through anywhere on the bar, since a double or right click on
a button there still reaches the bar.

**Linux minimised reading.** `window_control::is_minimised` and
`set_visible` built their arguments as a tuple, so every call opened an X
connection, on Wayland too. They now read the X window first and connect
only for an X11 window, and the connection (with the `WM_STATE` atom) is
kept for the UI thread, so a check is one `GetProperty` round trip. A
Wayland window never connects.

**Smaller fixes.** `QUILL_DEMO_SHOT_KEYS` is read only in a
`demo-capture` build. The live story top-up returns before it marks the
Stars packs open when the Stars box is refused. The main menu's Night
mode and archive toggles are refused as "change a setting"
(`HardSurface::Setting`). In `gpui-pre`, a node that owns the focused
handle replaces the focus fallback instead of tripping the "set_focus
called more than once" assertion (`QUILL-CHANGES.md`).

Tests (`agent_driver_hard_tests.rs`): with "All other chats" on, an agent
right-clicks the Star chip under a message: the reply carries "Quill
closed a dialog", the amount box is not open and no dialog shows; the
same opener run as a deferred half (`agent_step`) leaves nothing open; the
user's own call opens the box. An agent's `switch_account` and Answer are
refused and the account stays. F11 and Cmd+Ctrl+F are refused and the
window is not full screen; Zoom is refused. `window_frame_tests.rs`
hovers the drawn bar: the drag area reports only `window-chrome` and
refuses, the menu and the action report the content tag first and pass.
`redact` and `agent_driver_surface` unit tests cover the tag rule and the
closes. The X11 code was type-checked against x11rb 0.13.2 in a scratch
crate (the Linux target does not build here: no cross C toolchain); it did
not run on a Linux desktop, nor did the Windows and macOS minimised
readings change.
