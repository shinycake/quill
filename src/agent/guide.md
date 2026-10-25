# Using Quill as an agent

Quill is a desktop Telegram client. With Agent access turned on (Quill:
Settings > Advanced > Agent access), an agent on the same computer can use
the user's Telegram account through the Quill that is already running. Quill
talks to Telegram; the agent talks to Quill. Nothing leaves the computer
except the Telegram traffic Quill would make anyway.

## Start here

Call `quill_status`. It names the signed-in account, the chats you may use
and with what scope, and the tools that work right now.

## What you can do

Quill exposes four layers. This version ships the first one, read only:

1. Curated tools: list, read, search and get messages.
2. Raw TDLib calls (`td_call`): coming later.
3. UI driver (click, type and look at Quill's window): coming later.
4. Events (new messages as notifications, `wait_for_message`): coming later.

## Grants

The user decides which chats agents may use. Right now agents may read
Saved Messages only. Pass `"chat": "saved"` (or leave `chat` out). Other
chats answer "Chat not granted".

## Tools

| Tool | Use it to |
| --- | --- |
| `quill_status` | See the account, grants and tools. |
| `list_messages` | See the latest messages in a chat, one line each. |
| `read_history` | Page through a chat in full, older or newer from a message id. |
| `search_messages` | Find messages by words. |
| `get_message` | Read one message in full (reply target, forward origin, file, links). |

Messages come back as text lines like

    [4194304] 2026-10-24T14:02:05Z You (forwarded from Ann): [document] report.pdf Q3 numbers

and as JSON in `structuredContent`: `id`, `date` (UTC), `from`, `kind`
(`text`, `photo`, `document`, `voice_note`...), `text` (the text or caption),
and in full reads `reply_to`, `forwarded_from`, `file`, `links`, `edited`.

## Recipes

Latest notes the user saved:

    list_messages {"chat": "saved", "limit": 20}

Everything saved last week: call `read_history` with no ids, then keep
passing `older_cursor` as `before_message_id` until dates pass the week.

Find a link the user saved:

    search_messages {"chat": "saved", "query": "github"}
    get_message {"chat": "saved", "message_id": <id from the search>}

## Errors

Every error says what to do next. The common ones:

- "Agent access is off in Quill": ask the user to turn it on.
- "Quill is not signed in": ask the user to sign in to Quill.
- "Chat not granted": only Saved Messages is readable in this version.
- "rate limiting": wait the seconds Telegram names, then retry.

## Rules

Agents act as the user, under Telegram's Terms of Service. Do not use what
you read to train or improve AI models, and do not export or index whole
chats.
