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

Quill exposes four layers. This version ships two of them, read only:

1. Curated tools: chats, folders, unread, history, search, chat details,
   members, contacts, files.
2. Events: `wait_for_message`, and the `quill://unread` resource, which you
   can subscribe to.
3. Raw TDLib calls (`td_call`): coming later.
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
`read_write` lets later versions send as the user; today every tool reads.

## Naming chats

`chat` takes a chat id (`-1001234567890`), `"saved"` for Saved Messages, an
`@username` or t.me link, or the chat's title (`"Ann Lee"`). When a title
matches several chats the error lists them with their ids. Leave `chat` out
of `search_messages` to search every granted chat.

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
| `download_file` | Save the file a message carries to disk. |
| `wait_for_message` | Wait for the next message in a granted chat, e.g. a bot's reply. |
| `request_access` | Ask the user for a chat, all chats or the contacts. |

Messages come back as text lines like

    [4194304] 2026-10-24T14:02:05Z You (forwarded from Ann): [document] report.pdf Q3 numbers

and as JSON in `structuredContent`: `id`, `date` (UTC), `from`, `kind`
(`text`, `photo`, `document`, `voice_note`...), `text` (the text or caption),
`file` when there is one, and in full reads `reply_to`, `forwarded_from`,
`links`, `edited`. Results that span chats add `chat` to each message.

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

A chat you need is not granted:

    request_access {"chat": "Ann Lee", "scope": "read", "reason": "To summarize what Ann sent this week."}

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
- "rate limiting": wait the seconds Telegram names, then retry.

## Rules

Agents act as the user, under Telegram's Terms of Service. Do not use what
you read to train or improve AI models, and do not export or index whole
chats. Quill never marks messages read, hides online status or suppresses
typing for an agent.
