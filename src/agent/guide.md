# Using Quill as an agent

Quill is a desktop Telegram client. With Agent access turned on (Quill:
Settings > Advanced > Agent access), an agent on the same computer can use
the user's Telegram account through the Quill that is already running. Quill
talks to Telegram; the agent talks to Quill. Nothing leaves the computer
except the Telegram traffic Quill would make anyway.

## Start here

Call `quill_status`. It names the signed-in account, what you may use and
with what scope, and the tools that work right now.

## What you can do

Quill exposes four layers. This version ships three of them:

1. Curated tools. Reading: chats, folders, unread, history, search, chat
   details, members, contacts, files. Writing: send text, files, albums,
   voice notes, polls and stickers; forward; edit and delete the user's own
   messages; react; pin; drafts; mark read; mute; archive; folders.
2. Events: `wait_for_message`, and the `quill://unread` resource, which you
   can subscribe to.
3. Raw TDLib calls (`td_call`): any TDLib request, under the same grants.
4. UI driver (click, type and look at Quill's window): coming later.

## Grants

The user decides what agents may use, per account, in Quill's settings:

- By default: Saved Messages and bots (read and write), and channels the
  user owns (read).
- "All other chats" is off unless the user turns it on. Secret chats are
  never covered by it; each one must be granted by itself.
- The contact list (names and usernames, never phone numbers) is off by
  default.

A chat that is not granted answers "Chat not granted". Then call
`request_access` with the chat, the scope you need and one sentence on why:
Quill shows the user a prompt and the call waits up to two minutes for the
answer. Ask for one thing per call, and do not ask again after a "no".
Write tools need `read_write` on the chat.

## Naming chats

`chat` takes a chat id (`-1001234567890`), `"saved"` for Saved Messages, an
`@username` or t.me link, or the chat's title (`"Ann Lee"`). When a title
matches several granted chats the error lists them with their ids. Leave
`chat` out of `search_messages` to search every granted chat. Names only
ever match granted chats, so "No granted chat is called…" does not say
whether such a chat exists; if the user wants you to use it, ask with
`request_access`, which takes the same name and shows the user the match.
`list_chats` shows only granted chats.

## Tools

| Tool | Use it to |
| --- | --- |
| `quill_status` | See the account, grants and tools. |
| `list_chats` | Find chats and their ids, with unread counts; filter by folder, unread or title. |
| `list_folders` | See the user's chat folders. |
| `list_unread` | Catch up: unread messages in every granted chat. Does not mark them read. |
| `list_messages` | See the latest messages in one chat, one line each. |
| `read_history` | Page through a chat in full, older or newer from a message id. |
| `search_messages` | Find messages by words, in one chat or every granted chat. |
| `get_message` | Read one message in full (reply target, forward origin, file, links). |
| `get_chat_info` | Learn about a chat: kind, username, bio or description, members, link. |
| `list_members` | See or find the members of a group or channel. |
| `list_contacts` | Find a person among the contacts (needs the contacts grant). |
| `download_file` | Save the file a message carries to disk (never overwrites; a taken name gets " (2)"). |
| `wait_for_message` | Wait for the next message in a granted chat, e.g. a bot's reply. |
| `request_access` | Ask the user for a chat, all chats or the contacts. |
| `send_message` | Send text (plain, MarkdownV2 or HTML), as a reply or scheduled. |
| `send_file` | Send files from this computer; 2 to 10 go as an album. |
| `send_poll` | Start a poll or quiz. |
| `send_sticker` | Send one of the user's stickers for an emoji. |
| `forward_messages` | Forward messages from a readable chat to a writable one. |
| `edit_message` | Change the text or caption of the user's own message. |
| `delete_messages` | Delete the user's own messages. |
| `react` | Add or remove an emoji reaction. |
| `pin_message` | Pin or unpin a message. |
| `set_draft` | Leave text in the message box for the user to send. |
| `mark_read` | Mark a chat read (reading never does this by itself). |
| `mute_chat` | Mute or unmute a chat. |
| `archive_chat` | Archive or unarchive a chat. |
| `set_chat_folder` | Add a chat to a folder or take it out. |

Messages come back as text lines like

    [4194304] 2026-10-24T14:02:05Z You (forwarded from Ann): [document] report.pdf Q3 numbers

and as JSON in `structuredContent`: `id`, `date` (UTC), `from`, `kind`
(`text`, `photo`, `document`, `voice_note`...), `text` (the text or caption),
`file` when there is one, and in full reads `reply_to`, `forwarded_from`,
`links`, `styles` (formatted spans, like `{"style": "bold", "text": "agent"}`),
`edited`. Results that span chats add `chat` to each message.

## Recipes

Catch up on what is new:

    list_unread {}

Find a chat and read it:

    list_chats {"query": "design"}
    list_messages {"chat": -1001234567890, "limit": 30}

Find a link the user saved:

    search_messages {"chat": "saved", "query": "github"}
    get_message {"chat": "saved", "message_id": <id from the search>}

Wait for a bot's next message:

    wait_for_message {"chat": "@some_bot", "timeout_seconds": 60}

Something you need is not granted. Ask for one of these per call:

    request_access {"chat": "Ann Lee", "scope": "read", "reason": "To summarize what Ann sent this week."}
    request_access {"all_chats": true, "scope": "read", "reason": "To find every message about the trip."}
    request_access {"contacts": true, "reason": "To find Ann's chat."}

Send a message and wait for the bot's answer:

    send_message {"chat": "@some_bot", "text": "/start"}
    wait_for_message {"chat": "@some_bot", "timeout_seconds": 60}

Leave a reply for the user to check and send:

    set_draft {"chat": "Ann Lee", "text": "Sounds good, see you at 6"}

## Sending

Everything you send goes out as the user, in their name. Sends wait until
Telegram delivers them and return the final message id. Quill paces them:
messages to one chat are spaced out, a flood wait from Telegram pauses every
agent send for the seconds it names, and at most five conversations a day may
start with someone who never wrote to the user. Agents cannot spend Stars:
chats that charge for messages refuse agent sends. Every write shows in the
user's activity log in Quill, and if the user turned on "Confirm each send"
each send, edit, delete, reaction and pin waits for their click. Edits,
reactions, pins and the other changes are spaced out like sends.

The bots default covers bots the user already talks to. A bot you just found
by @username needs `request_access` with `read_write` before you write to it.
`send_file` never sends files from Quill's own data folder. If the user turns
Agent access off while a write waits its turn, nothing goes out.

## Raw TDLib calls

Use `td_call` only when no curated tool does the job. It takes a TDLib
request as JSON (the `@type` is the method; fields as in the
[TDLib API](https://core.telegram.org/tdlib/docs/td__api_8h.html)):

    td_call {"request": {"@type": "getChatPinnedMessage", "chat_id": -1001234567890}}

- Grants apply to every chat the request names: `chat_id`, `from_chat_id`,
  `chat_ids`, and the chat behind a `supergroup_id`, `basic_group_id` or
  `secret_chat_id`. Methods that start with `get`, `search` or `check` need
  `read`; changes need `read_write` on the chat they act in. A change aimed
  at a `user_id` acts in the private chat with that user.
- Sends (`sendMessage`, `sendMessageAlbum`, `forwardMessages`...) share the
  paced queue with `send_message`.
- Reads that name no chat (`getMe`, `getChats`, `searchPublicChats`) just
  run; their answers are filtered as below. Sends wait for delivery and
  return the final message ids.
- The account itself (sign-out, sessions, password, phone, privacy, the
  profile, any change that names no chat) and anything that can spend Stars,
  TON or money wait for the user to click Allow in Quill. Pass `reason`:
  the user reads it. One Allow covers one call.
- Quill's own plumbing never runs: `setOption`, the sign-in flow, closing
  the client, logs.
- Answers hide what you may not read: a chat that is not granted comes back
  as `{"@type": "chat", "id": …, "hidden": "not granted to agents"}`, a
  message from one as `{"@type": "message", "chat_id": …, "hidden": …}`,
  and `chat_ids` lists keep only granted chats (with `total_count` to match). Phone numbers are removed.
  Sponsored messages (ads) carry `"sponsored": true`; never present them as
  something the channel posted.
- `getContacts` and `searchContacts` need the contacts grant.

## Events

`wait_for_message` returns as soon as a message arrives (or the time runs
out; call it again to keep waiting). MCP clients that support resource
subscriptions can subscribe to `quill://unread` and get
`notifications/resources/updated` whenever a message from someone else lands
in a granted chat; read the resource again to see it.

## Errors

Every error says what to do next. The common ones:

- "Agent access is off in Quill": ask the user to turn it on.
- "Quill is not signed in": ask the user to sign in to Quill.
- "Chat not granted": call `request_access`.
- "Several chats match": pass the chat id from the list.
- "rate limiting" or "slow down": wait the seconds named, then retry.
- "granted for reading only": call `request_access` with `read_write`.
- "declined this in Quill": the user said no; do not retry.

## Rules

Agents act as the user, under Telegram's Terms of Service. Do not use what
you read to train or improve AI models, and do not export or index whole
chats. Quill never marks messages read, hides online status or suppresses
typing for an agent.

## UI driver

For what no chat tool does: Quill's settings, a dialog, a picker, checking
what the user sees. Reading and sending messages stay with the chat tools,
which are faster, work with the window hidden and go through Quill's
grants, pacing and confirmations.

The user turns it on separately: "Let agents drive Quill's window" in
Settings > Advanced > Agent access (off by default). `quill_status` shows
`ui_driver.available`.

| Tool | Use it to |
| --- | --- |
| `read_ui_tree` | See the window: nodes with `id`, `role`, `name`, `value`, `bounds` `[x, y, w, h]` in window pixels, `states`, `actions`, `children`. `filter` and `role` narrow it. |
| `click_element` | Click a node by id, or a point; `button` right or middle, `count` 2 for a double click. |
| `type_text` | Type into the focused box (click it first). |
| `press_keys` | Press chords like `cmd-k`, `escape`, `ctrl-shift-a`, several separated by spaces. |
| `scroll_view` | Scroll what is under a node or point; `dy` > 0 scrolls down. |
| `hover_element` | Move the pointer over a node, for a tooltip or hover controls. |
| `open_menu` | Right-click a node and get the menu's items. |

Open settings and read them:

    press_keys {"keys": "cmd-,"}
    read_ui_tree {"role": "Button"}

Find the search box and look for a chat:

    read_ui_tree {"filter": "search"}
    click_element {"target": {"node": "<id of the search box>"}}
    type_text {"text": "design"}
    read_ui_tree {"role": "ListItem"}

Every input tool answers with the focused node and the nodes that appeared
after it (a dialog, a menu), so you usually do not need another
`read_ui_tree` to see what happened. Ids stay the same while an element
stays on screen; after the window changes a lot, read it again.

Rules, all enforced by Quill:

- The chat open in the window must be granted: `read` to read the window,
  `read_write` for any click, key or scroll. Otherwise the tool says so and
  names the chat for `request_access`.
- Controls that send, schedule, delete or pay (Stars, Premium, gifts) are
  never clicked, and Enter is not pressed while it would send the message
  in the focused box. Use `send_message`, `set_draft`, `delete_messages`.
  `type_text` types a line break as the chord that inserts one.
- Keys that quit, close or hide Quill are never pressed.
- While the user is using the window the driver pauses and the tools say
  so. Wait and retry, or use the chat tools.

What it cannot reach: native menus (the macOS menu bar), file and folder
pickers, the input method editor, and system dialogs. Quill's window must
be open and visible so it draws frames; a hidden or minimised window
answers "did not redraw". Bounds are in window pixels, not screen pixels.
`focus` is reported only when the focused element itself carries a role.
