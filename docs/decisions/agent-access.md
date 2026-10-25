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
