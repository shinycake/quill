//! The write tools: everything an agent can change, in chats granted
//! read_write. Sends go through one paced queue and wait for delivery;
//! every write is logged for the user (Settings > Agent access); with
//! "Confirm each send" on, sends, edits, deletes, reactions and pins wait
//! for the user's click in Quill.

mod change;
pub(super) mod common;
mod manage;
pub(super) mod send;

use super::Tool;

pub(super) const TOOLS: &[Tool] = &[
    Tool {
        name: "send_message",
        title: "Send a message",
        description: "Use to send a text message as the user in a chat granted read_write. Waits until Telegram delivers it. \
Example input: {\"chat\": \"saved\", \"text\": \"Remember the *demo*\", \"format\": \"markdown\"}, or {\"chat\": \"@some_bot\", \"text\": \"/start\"}. \
Output: chat, messages [{id, date, from, kind, text, styles}] with the final message id, scheduled. styles lists the formatted spans, e.g. [{style: bold, text: agent}].",
        schema: send::send_message_schema,
        run: send::send_message,
        read_only: false,
        confirms_sends: true,
    },
    Tool {
        name: "send_file",
        title: "Send files",
        description: "Use to send files from this computer: photos, videos, audio, voice notes or documents; 2 to 10 go as one album. \
Example input: {\"chat\": \"saved\", \"paths\": [\"/Users/ann/report.pdf\"], \"caption\": \"Q3\"}. \
Output: chat, messages [{id, kind, file}].",
        schema: send::send_file_schema,
        run: send::send_file,
        read_only: false,
        confirms_sends: true,
    },
    Tool {
        name: "send_poll",
        title: "Send a poll",
        description: "Use to start a poll or a quiz. \
Example input: {\"chat\": -1001234567890, \"question\": \"Lunch?\", \"options\": [\"Pizza\", \"Sushi\"]}. \
Output: chat, messages [{id, kind: poll}].",
        schema: send::send_poll_schema,
        run: send::send_poll,
        read_only: false,
        confirms_sends: true,
    },
    Tool {
        name: "send_sticker",
        title: "Send a sticker",
        description: "Use to send one of the user's stickers for an emoji. \
Example input: {\"chat\": \"Ann Lee\", \"emoji\": \"👍\"}. Output: chat, messages [{id, kind: sticker}].",
        schema: send::send_sticker_schema,
        run: send::send_sticker,
        read_only: false,
        confirms_sends: true,
    },
    Tool {
        name: "forward_messages",
        title: "Forward messages",
        description: "Use to forward messages from a chat you may read to a chat granted read_write. \
Example input: {\"from_chat\": \"saved\", \"message_ids\": [4194304], \"to_chat\": \"@some_bot\"}. \
Output: chat (the destination), messages [{id, forwarded_from}].",
        schema: send::forward_schema,
        run: send::forward_messages,
        read_only: false,
        confirms_sends: true,
    },
    Tool {
        name: "edit_message",
        title: "Edit a message",
        description: "Use to change the text or caption of one of the user's own messages. \
Example input: {\"chat\": \"saved\", \"message_id\": 4194304, \"text\": \"Fixed typo\"}. Output: chat, message (the edited message, with styles).",
        schema: change::edit_schema,
        run: change::edit_message,
        read_only: false,
        confirms_sends: true,
    },
    Tool {
        name: "delete_messages",
        title: "Delete messages",
        description: "Use to delete the user's own messages (never other people's), for everyone where Telegram allows it. \
Example input: {\"chat\": \"saved\", \"message_ids\": [4194304]}. Output: chat, deleted, for_everyone (false where Telegram keeps one copy, as in Saved Messages).",
        schema: change::delete_schema,
        run: change::delete_messages,
        read_only: false,
        confirms_sends: true,
    },
    Tool {
        name: "react",
        title: "React to a message",
        description: "Use to add (or with remove: true, take back) an emoji reaction. \
Example input: {\"chat\": -1001234567890, \"message_id\": 4194304, \"emoji\": \"👍\"}. Output: chat, message_id, emoji, removed.",
        schema: change::react_schema,
        run: change::react,
        read_only: false,
        confirms_sends: true,
    },
    Tool {
        name: "pin_message",
        title: "Pin a message",
        description: "Use to pin or unpin a message in a chat. \
Example input: {\"chat\": \"saved\", \"message_id\": 4194304}. Output: chat, message_id, pinned.",
        schema: change::pin_schema,
        run: change::pin_message,
        read_only: false,
        confirms_sends: true,
    },
    Tool {
        name: "set_draft",
        title: "Set the draft",
        description: "Use to put text in a chat's message box for the user to review and send themselves. \
Example input: {\"chat\": \"Ann Lee\", \"text\": \"Sounds good, see you at 6\"}. Output: chat, draft (the text, or null once cleared).",
        schema: manage::draft_schema,
        run: manage::set_draft,
        read_only: false,
        confirms_sends: false,
    },
    Tool {
        name: "mark_read",
        title: "Mark a chat as read",
        description: "Use when the user asks to mark a chat read. Reading tools never do this by themselves. \
Example input: {\"chat\": \"@some_bot\"}. Output: chat, read.",
        schema: manage::mark_read_schema,
        run: manage::mark_read,
        read_only: false,
        confirms_sends: false,
    },
    Tool {
        name: "mute_chat",
        title: "Mute a chat",
        description: "Use to mute a chat's notifications, for a while or until unmuted; minutes: 0 unmutes. \
Example input: {\"chat\": -1001234567890, \"minutes\": 480}. Output: chat, mute_for_seconds.",
        schema: manage::mute_schema,
        run: manage::mute_chat,
        read_only: false,
        confirms_sends: false,
    },
    Tool {
        name: "archive_chat",
        title: "Archive a chat",
        description: "Use to move a chat to the archive, or back with archive: false. \
Example input: {\"chat\": \"@some_bot\"}. Output: chat, archived.",
        schema: manage::archive_schema,
        run: manage::archive_chat,
        read_only: false,
        confirms_sends: false,
    },
    Tool {
        name: "set_chat_folder",
        title: "Add a chat to a folder",
        description: "Use to add a chat to one of the user's folders, or take it out with remove: true. \
Example input: {\"chat\": \"Ann Lee\", \"folder\": \"Work\"}. Output: chat, folder {id, name}, in_folder.",
        schema: manage::folder_schema,
        run: manage::set_chat_folder,
        read_only: false,
        confirms_sends: false,
    },
];
