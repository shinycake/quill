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
        "edit_message" => tr(keys::Q_AGENT_TOOL_EDIT_MESSAGE).to_string(),
        "delete_message" => tr(keys::Q_AGENT_TOOL_DELETE_MESSAGE).to_string(),
        other => {
            let spaced = other.replace('_', " ");
            let mut chars = spaced.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect()
            })
        }
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
        assert_eq!(tool_label("pin_message"), "Pin message");
        assert_eq!(tool_label(""), "");
    }
}
