//! What the window is showing, as far as the driver is concerned: the one
//! value every command is checked against. The UI computes it from its own
//! state (`src/ui/agent_driver_surface.rs`); nothing here looks at the
//! element tree, so a label can never widen it.

use crate::agent::grants::Scope;

/// How far the driver reaches, from the account's grants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverScope {
    /// Only the chats granted one by one or by a class default: the tree is
    /// cut to the chat list's granted rows and the open granted chat, and
    /// every other surface is blocked.
    Granted,
    /// "All other chats" is on: every chat but secret and blocked ones
    /// counts as granted, the chat list, search and contacts show whole,
    /// and the surfaces that are not on the hard list are allowed.
    Everything,
}

/// The surface the window shows, as the driver sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Surface {
    /// No chat is open and no overlay is in the way: the chat list.
    ChatList,
    /// A chat the driver may read is open, with the scope it is granted.
    Chat { chat_id: i64, scope: Scope },
    /// Something agents never see or use: the reason, worded for the user.
    Blocked(String),
}

impl Surface {
    /// The chat the surface is about.
    pub fn open_chat(&self) -> Option<i64> {
        match self {
            Surface::Chat { chat_id, .. } => Some(*chat_id),
            _ => None,
        }
    }

    /// Whether the surface takes input at all: a blocked surface never, a
    /// chat only at `read_write`.
    pub fn input_refusal(&self) -> Option<String> {
        match self {
            Surface::ChatList => None,
            Surface::Chat { scope, .. } if scope.allows(Scope::ReadWrite) => None,
            Surface::Chat { chat_id, .. } => Some(format!(
                "The chat open in Quill's window ({chat_id}) is granted for reading only, so no clicks or keys go to it. Call request_access {{\"chat\": {chat_id}, \"scope\": \"read_write\", \"reason\": ...}}, or ask the user to open another chat."
            )),
            Surface::Blocked(what) => Some(blocked_note(what)),
        }
    }

    /// Whether reading the tree is allowed; the note otherwise.
    pub fn read_refusal(&self) -> Option<String> {
        match self {
            Surface::Blocked(what) => Some(blocked_note(what)),
            _ => None,
        }
    }

    /// Whether `other` is the same surface, so a command that started on
    /// this one may go on.
    pub fn same_as(&self, other: &Surface) -> bool {
        self == other
    }

    /// A short name for refusals and the log.
    pub fn describe(&self) -> String {
        match self {
            Surface::ChatList => "the chat list".to_string(),
            Surface::Chat { chat_id, scope } => {
                format!("chat {chat_id} ({})", scope.as_str())
            }
            Surface::Blocked(what) => what.clone(),
        }
    }
}

/// What `read_ui_tree` returns instead of a tree while the surface is
/// blocked.
pub fn blocked_note(what: &str) -> String {
    format!(
        "Quill is showing {what}; agents can't see or use it. Ask the user, or use the chat tools (list_chats, read_history, send_message...)."
    )
}

/// The surface changed between two steps of one command.
pub fn changed_refusal(expected: &Surface, now: &Surface) -> String {
    format!(
        "Quill's window changed from {} to {} before the command finished, so nothing more was sent. Call read_ui_tree and go on from there.",
        expected.describe(),
        now.describe()
    )
}

/// An ungranted chat was about to open through the window.
pub fn ungranted_chat(chat_id: i64) -> String {
    format!(
        "Chat {chat_id} is not granted to agents, so Quill did not open it. Call request_access {{\"chat\": {chat_id}, \"scope\": \"read\", \"reason\": ...}}, or ask the user."
    )
}

/// A screen reader is on (`crate::screen_reader`): the driver stays out of
/// its way.
pub fn screen_reader_active() -> String {
    "A screen reader is on, so agents do not drive Quill's window until the user turns it off. Use the chat tools, or ask the user.".to_string()
}

#[cfg(test)]
mod tests {
    use super::{Surface, blocked_note, changed_refusal};
    use crate::agent::grants::Scope;

    #[test]
    fn a_chat_takes_input_only_at_read_write() {
        assert_eq!(Surface::ChatList.input_refusal(), None);
        let read = Surface::Chat {
            chat_id: 7,
            scope: Scope::Read,
        };
        assert!(read.read_refusal().is_none());
        assert!(
            read.input_refusal()
                .is_some_and(|r| r.contains("reading only"))
        );
        let write = Surface::Chat {
            chat_id: 7,
            scope: Scope::ReadWrite,
        };
        assert_eq!(write.input_refusal(), None);
        assert_eq!(write.open_chat(), Some(7));
        let blocked = Surface::Blocked("Settings".into());
        assert_eq!(blocked.read_refusal(), Some(blocked_note("Settings")));
        assert!(
            blocked
                .input_refusal()
                .is_some_and(|r| r.contains("Settings"))
        );
        assert_eq!(blocked.open_chat(), None);
    }

    #[test]
    fn a_change_names_both_surfaces() {
        let text = changed_refusal(&Surface::ChatList, &Surface::Blocked("Settings".into()));
        assert!(text.contains("the chat list"));
        assert!(text.contains("Settings"));
    }
}
