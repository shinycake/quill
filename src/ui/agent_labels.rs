//! Words for what agents did and who is connected, shared by the Agent
//! access page and the connected strip.
#![deny(clippy::indexing_slicing)]

use quill::lang::{keys, tr, tr_with};
use quill::local_time::{civil_local, hhmm};

pub(super) fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// `send_message` as a person reads it: `Send message`.
pub(super) fn tool_label(tool: &str) -> String {
    match tool {
        "send_message" => tr(keys::Q_AGENT_TOOL_SEND_MESSAGE).to_string(),
        "send_file" => tr(keys::Q_AGENT_TOOL_SEND_FILE).to_string(),
        "send_poll" => tr(keys::Q_AGENT_TOOL_SEND_POLL).to_string(),
        "send_sticker" => tr(keys::Q_AGENT_TOOL_SEND_STICKER).to_string(),
        "forward_messages" => tr(keys::Q_AGENT_TOOL_FORWARD_MESSAGES).to_string(),
        "edit_message" => tr(keys::Q_AGENT_TOOL_EDIT_MESSAGE).to_string(),
        "delete_messages" => tr(keys::Q_AGENT_TOOL_DELETE_MESSAGES).to_string(),
        "react" => tr(keys::Q_AGENT_TOOL_REACT).to_string(),
        "pin_message" => tr(keys::Q_AGENT_TOOL_PIN_MESSAGE).to_string(),
        "set_draft" => tr(keys::Q_AGENT_TOOL_SET_DRAFT).to_string(),
        "mark_read" => tr(keys::Q_AGENT_TOOL_MARK_READ).to_string(),
        "mute_chat" => tr(keys::Q_AGENT_TOOL_MUTE_CHAT).to_string(),
        "archive_chat" => tr(keys::Q_AGENT_TOOL_ARCHIVE_CHAT).to_string(),
        "set_chat_folder" => tr(keys::Q_AGENT_TOOL_SET_CHAT_FOLDER).to_string(),
        "remove_reaction" => tr(keys::Q_AGENT_TOOL_REMOVE_REACTION).to_string(),
        "unpin_message" => tr(keys::Q_AGENT_TOOL_UNPIN_MESSAGE).to_string(),
        "clear_draft" => tr(keys::Q_AGENT_TOOL_CLEAR_DRAFT).to_string(),
        "unmute_chat" => tr(keys::Q_AGENT_TOOL_UNMUTE_CHAT).to_string(),
        "unarchive_chat" => tr(keys::Q_AGENT_TOOL_UNARCHIVE_CHAT).to_string(),
        "remove_from_folder" => tr(keys::Q_AGENT_TOOL_REMOVE_FROM_FOLDER).to_string(),
        other => {
            let spaced = other.replace('_', " ");
            let mut chars = spaced.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect()
            })
        }
    }
}

/// The title of the confirmation for a write of `kind`:
/// `{agent} wants to send a message in "{chat}"`.
pub(super) fn confirm_title_key(kind: &str) -> &'static str {
    match kind {
        "send_file" => keys::Q_AGENT_CONFIRM_SEND_FILE,
        "send_poll" => keys::Q_AGENT_CONFIRM_SEND_POLL,
        "send_sticker" => keys::Q_AGENT_CONFIRM_SEND_STICKER,
        "forward_messages" => keys::Q_AGENT_CONFIRM_FORWARD,
        "edit_message" => keys::Q_AGENT_CONFIRM_EDIT,
        "delete_messages" => keys::Q_AGENT_CONFIRM_DELETE,
        "react" => keys::Q_AGENT_CONFIRM_REACT,
        "remove_reaction" => keys::Q_AGENT_CONFIRM_REMOVE_REACTION,
        "pin_message" => keys::Q_AGENT_CONFIRM_PIN,
        "unpin_message" => keys::Q_AGENT_CONFIRM_UNPIN,
        _ => keys::Q_AGENT_CONFIRM_SEND_MESSAGE,
    }
}

/// The label over a confirmation's preview: what the preview is.
pub(super) fn confirm_preview_key(kind: &str) -> &'static str {
    match kind {
        "forward_messages" => keys::Q_AGENT_CONFIRM_PREVIEW_FORWARD,
        "edit_message" => keys::Q_AGENT_CONFIRM_PREVIEW_EDIT,
        "delete_messages" => keys::Q_AGENT_CONFIRM_PREVIEW_DELETE,
        "react" | "remove_reaction" => keys::Q_AGENT_CONFIRM_PREVIEW_REACTION,
        "pin_message" | "unpin_message" => keys::Q_AGENT_CONFIRM_PREVIEW_MESSAGE,
        _ => keys::Q_AGENT_REQUEST_WRITE_PREVIEW,
    }
}

/// "since 14:02" for an agent that connected at `unix`.
pub(super) fn since_label(unix: i64) -> String {
    tr_with(
        keys::Q_AGENT_CONNECTED_SINCE,
        &[("time", &hhmm(&civil_local(unix)))],
    )
}

/// An agent's name as shown, with a fallback for clients that gave none.
pub(super) fn agent_label(agent: &str) -> String {
    let name = agent.trim();
    if name.is_empty() {
        tr(keys::Q_AGENT_REQUEST_UNKNOWN_AGENT).to_string()
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::tool_label;

    #[test]
    fn tool_names_read_as_words() {
        assert_eq!(tool_label("send_message"), "Send message");
        assert_eq!(tool_label("pin_message"), "Pin or unpin");
        assert_eq!(tool_label("future_tool"), "Future tool");
        assert_eq!(tool_label(""), "");
    }
}
