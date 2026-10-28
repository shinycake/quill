---
name: quill
description: Read and send the user's Telegram messages through Quill, the desktop Telegram client running on this computer. Use when the user asks about their Telegram chats, unread messages, saved links, groups, channels or bots, or wants a message sent, a reply drafted or a chat searched, and Quill's Agent access is on.
---

# Quill

Quill is the user's desktop Telegram client. With Agent access on, you use
the user's Telegram account through the Quill that is already running, as
the user, under the chats they granted. Quill talks to Telegram and you talk
to Quill. Nothing goes to any server except the Telegram traffic Quill would
make anyway.

The full reference is [guide.md](guide.md), next to this file. It is the
same text as Quill's `quill://guide` resource. Read it before you use a
tool for the first time in a session.

## Setup

Agent access is off by default. The user turns it on in Quill under
Settings > Advanced > Agent access. You can't turn it on yourself.

If you have Quill's MCP tools (`quill_status`, `list_chats` and so on), use
them. If not, use the command line, which reaches the same tools in the
same running Quill:

    quill agent status
    quill agent --help

`quill` is Quill's own executable. The installers put it here:

- macOS: `/Applications/Quill.app/Contents/MacOS/quill`
- Windows: `%LOCALAPPDATA%\Programs\Quill\quill.exe`
- Linux: `~/.local/bin/quill`

On Windows, run the command line through `quill-cli.exe` in the same
folder (`quill-cli agent status`). It waits for the answer and passes the
exit code on. `quill.exe` is a windowed program, so a terminal doesn't
wait for it and `%ERRORLEVEL%` never gets its code.

`quill agent connect claude-code --print` prints the path of the copy you
ran. To add Quill as an MCP server instead, the user runs:

    quill agent connect claude-code      # prints the `claude mcp add` line
    quill agent connect claude-desktop   # shows or writes (--yes) the config
    quill agent connect cursor

## Start here

Call `quill_status` (`quill agent status`). It shows the signed-in account,
the chats you may use and with what scope, and the tools that work right
now. `quill agent describe` prints every tool's input schema, an example
and the output shape as JSON.

## Grants

The user decides what you may use, per account, in Quill's settings:

- By default: Saved Messages and bots (read and write), and channels the
  user owns (read).
- Other chats need a grant. "All other chats" is off unless the user turns
  it on, and secret chats always need their own grant.
- The contact list (names and usernames, never phone numbers) is off by
  default.

A chat that isn't granted answers "Chat not granted". Then call
`request_access` with the chat, the scope (`read` or `read_write`) and one
sentence on why. Quill shows the user a prompt and waits up to two minutes.
Ask for one thing at a time, and don't ask again after a no.

## Tools

- Reading: `list_chats`, `list_folders`, `list_unread`, `list_messages`,
  `read_history`, `search_messages`, `get_message`, `get_chat_info`,
  `list_members`, `list_contacts`, `download_file`, `wait_for_message`.
  Reading never marks messages read; `mark_read` does that.
- Writing: `send_message`, `send_file`, `send_poll`, `send_sticker`,
  `forward_messages`, `edit_message`, `delete_messages`, `react`,
  `pin_message`, `set_draft`, `mark_read`, `mute_chat`, `archive_chat`,
  `set_chat_folder`. They need `read_write` on the chat.
- `td_call`: any TDLib request, under the same grants. Use it only when no
  tool above does the job. Requests that name no chat, touch the account or
  could spend money wait for the user to click Allow. Signing out and
  deleting the account are never available.
- UI driver: `read_ui_tree`, `click_element`, `type_text`, `press_keys`,
  `scroll_view`, `hover_element`, `open_menu`, `take_screenshot`. For
  dialogs, pickers and panels no chat tool covers. It needs a second
  switch, "Let agents drive Quill's window", follows the chat grants, and
  the user can take control at any moment. Settings, sign-in, payments and
  sessions are never reachable.

Chats are named by id (`-1001234567890`), `"saved"`, `@username`, a t.me
link or the chat's title.

## How to behave

- You act as the user. Everything you send goes out in their name and shows
  in their activity log in Quill. Ask the user before you send anything
  they didn't ask for, and prefer `set_draft` when they should check the
  wording.
- Respect pacing. Quill spaces out sends, obeys Telegram's flood waits and
  allows at most five new conversations a day. When a tool says to wait,
  wait the seconds it names; don't retry in a loop.
- Don't use what you read to train or improve AI models. Telegram's terms
  forbid it.
- No bulk export. Don't download, archive or index whole chats or accounts.
  Read what the task needs.
- Quill never hides read receipts, online status or typing for you, and
  you shouldn't try to get around that.
- Sponsored messages carry `"sponsored": true`. Never present them as
  something the channel posted.

## Recipes

Catch up on what's new:

    quill agent unread
    # MCP: list_unread {}

Find a chat and read it:

    quill agent chats --query design
    quill agent messages --chat -1001234567890 --limit 30

Find a link the user saved:

    quill agent search --chat saved --query github
    quill agent message --chat saved --message-id 4194304

Message a bot and wait for its answer:

    quill agent send --chat @some_bot --text /start
    quill agent wait --chat @some_bot --timeout-seconds 60

Leave a reply for the user to check and send:

    quill agent set_draft --chat "Ann Lee" --text "Sounds good, see you at 6"

Any tool, from a script, with JSON in and out:

    quill agent call read_history --json '{"chat": "saved", "limit": 10}'

## Errors

Every error says what to do next. `quill agent` also exits with a code that
tells scripts what went wrong (`--json` prints `{"error": {"kind",
"exit_code", "message"}}`):

| Exit | Kind | What to do |
| --- | --- | --- |
| 0 | ok | Done. |
| 1 | tool_error | Read the message: "Chat not granted" means `request_access`, "declined this in Quill" means the user said no. |
| 2 | usage | Fix the command; `quill agent help <command>`. |
| 3 | not_running | Ask the user to open Quill, or pass `--start`. |
| 4 | access_off | Ask the user to turn on Agent access in Quill. |
| 5 | token | Quill has no token yet or refused it, its data folder wasn't found, or it's another user's Quill. Retry; if it keeps failing, ask the user to restart Quill. |
| 6 | window_control_off | The UI tools need "Let agents drive Quill's window". The chat tools still work. |
| 7 | connection | Quill quit or closed the connection. Retry. |
| 8 | local_file | A local file couldn't be written, or holds edits `--force` would replace. |
