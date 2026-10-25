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

### Tools

`list_chats`, `list_folders`, `list_unread`, `get_chat_info`,
`list_members`, `list_contacts`, `download_file`, `wait_for_message`,
`request_access`, and `search_messages` across all granted chats. Only
granted chats ever appear: a search across chats drops matches in other
chats without saying how many there were. `list_unread` reads history with
`getChatHistory`, which does not mark anything read. `download_file` copies
from TDLib's cache to a path the agent names, or returns the cache path.

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
