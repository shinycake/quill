//! The chats of a full export: every chat in the main list and the
//! archive, classified the way tdesktop's `DialogInfo::Type` is, filtered
//! by the chosen chat types, and numbered `chats/chat_NN/` in list order
//! (tdesktop's `FinalizeDialogsInfo`).
use super::settings::{AccountSettings, Section};
use crate::export::worker::{ExportError, QUERY_TIMEOUT, Tdlib, query};
use serde_json::{Value, json};
use std::collections::HashSet;

/// A big enough `getChats` limit: TDLib returns what it has loaded.
const CHATS_LIMIT: i64 = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatKind {
    SavedMessages,
    Personal,
    Bot,
    PrivateGroup,
    PrivateSupergroup,
    PublicSupergroup,
    PrivateChannel,
    PublicChannel,
}

impl ChatKind {
    /// The export option that includes this kind of chat.
    pub fn section(self) -> Section {
        match self {
            Self::SavedMessages | Self::Personal => Section::PersonalChats,
            Self::Bot => Section::BotChats,
            Self::PrivateGroup | Self::PrivateSupergroup => Section::PrivateGroups,
            Self::PublicSupergroup => Section::PublicGroups,
            Self::PrivateChannel => Section::PrivateChannels,
            Self::PublicChannel => Section::PublicChannels,
        }
    }

    /// `type` in `result.json` (tdesktop's JSON names).
    pub fn json_type(self) -> &'static str {
        match self {
            Self::SavedMessages => "saved_messages",
            Self::Personal => "personal_chat",
            Self::Bot => "bot_chat",
            Self::PrivateGroup => "private_group",
            Self::PrivateSupergroup => "private_supergroup",
            Self::PublicSupergroup => "public_supergroup",
            Self::PrivateChannel => "private_channel",
            Self::PublicChannel => "public_channel",
        }
    }

    /// The type shown in the chats list (tdesktop's HTML names).
    pub fn list_type(self) -> &'static str {
        match self {
            Self::SavedMessages | Self::Personal => "private",
            Self::Bot => "bot",
            Self::PrivateGroup | Self::PrivateSupergroup | Self::PublicSupergroup => "group",
            Self::PrivateChannel | Self::PublicChannel => "channel",
        }
    }

    /// The name of a chat whose name is gone.
    pub fn deleted_name(self) -> &'static str {
        match self {
            Self::SavedMessages | Self::Personal | Self::Bot => "Deleted Account",
            Self::PrivateGroup | Self::PrivateSupergroup | Self::PublicSupergroup => {
                "Deleted Group"
            }
            Self::PrivateChannel | Self::PublicChannel => "Deleted Channel",
        }
    }
}

/// One chat to export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dialog {
    pub chat_id: i64,
    pub kind: ChatKind,
    /// Display name ("Saved messages" for the viewer's own chat).
    pub name: String,
    /// Only the viewer's own messages are exported from this chat.
    pub only_my: bool,
    /// `chats/chat_NN/`, relative to the export root.
    pub folder: String,
}

/// Every chat id in the main list and the archive, in list order, once.
fn all_chat_ids<T: Tdlib>(tdlib: &mut T) -> Result<Vec<i64>, ExportError> {
    let mut seen = HashSet::new();
    let mut ids = Vec::new();
    for list in ["chatListMain", "chatListArchive"] {
        // `loadChats` answers 404 once the whole list is loaded.
        loop {
            let answer = tdlib.call(
                json!({"@type": "loadChats", "chat_list": {"@type": list}, "limit": 100}),
                QUERY_TIMEOUT,
            )?;
            if answer["@type"] == "error" {
                if answer["code"] == 404 {
                    break;
                }
                return Err(ExportError::Tdlib(
                    answer["message"]
                        .as_str()
                        .unwrap_or("could not load the chat list")
                        .into(),
                ));
            }
        }
        let answer = query(
            tdlib,
            json!({"@type": "getChats", "chat_list": {"@type": list}, "limit": CHATS_LIMIT}),
        )?;
        for id in answer["chat_ids"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(Value::as_i64)
        {
            if seen.insert(id) {
                ids.push(id);
            }
        }
    }
    Ok(ids)
}

/// What kind of chat `chat` is, or `None` for chats the export skips
/// (secret chats, which tdesktop's export never sees either).
fn classify<T: Tdlib>(
    tdlib: &mut T,
    chat: &Value,
    me: i64,
) -> Result<Option<(ChatKind, String)>, ExportError> {
    let kind = &chat["type"];
    let title = chat["title"].as_str().unwrap_or_default().to_string();
    Ok(Some(match kind["@type"].as_str() {
        Some("chatTypePrivate") => {
            let user_id = kind["user_id"].as_i64().unwrap_or(0);
            if user_id == me {
                (ChatKind::SavedMessages, "Saved messages".to_string())
            } else {
                let user = tdlib.call(
                    json!({"@type": "getUser", "user_id": user_id}),
                    QUERY_TIMEOUT,
                )?;
                let bot = user["type"]["@type"] == "userTypeBot";
                (
                    if bot {
                        ChatKind::Bot
                    } else {
                        ChatKind::Personal
                    },
                    title,
                )
            }
        }
        Some("chatTypeBasicGroup") => (ChatKind::PrivateGroup, title),
        Some("chatTypeSupergroup") => {
            let supergroup = tdlib.call(
                json!({"@type": "getSupergroup", "supergroup_id": kind["supergroup_id"]}),
                QUERY_TIMEOUT,
            )?;
            let public = supergroup["usernames"]["active_usernames"]
                .as_array()
                .is_some_and(|names| !names.is_empty());
            let kind = match (kind["is_channel"] == true, public) {
                (true, true) => ChatKind::PublicChannel,
                (true, false) => ChatKind::PrivateChannel,
                (false, true) => ChatKind::PublicSupergroup,
                (false, false) => ChatKind::PrivateSupergroup,
            };
            (kind, title)
        }
        _ => return Ok(None),
    }))
}

/// The chats to export, and how many chosen chats were left out because
/// their content is protected (Quill never copies protected content).
pub fn list<T: Tdlib>(
    tdlib: &mut T,
    settings: &AccountSettings,
    me: i64,
    on_found: &dyn Fn(usize),
) -> Result<(Vec<Dialog>, usize), ExportError> {
    let mut dialogs = Vec::new();
    let mut protected = 0;
    for chat_id in all_chat_ids(tdlib)? {
        let chat = tdlib.call(
            json!({"@type": "getChat", "chat_id": chat_id}),
            QUERY_TIMEOUT,
        )?;
        if chat["@type"] == "error" {
            continue;
        }
        let Some((kind, name)) = classify(tdlib, &chat, me)? else {
            continue;
        };
        let section = kind.section();
        if !settings.sections.contains(section) {
            continue;
        }
        if chat["has_protected_content"] == true {
            protected += 1;
            continue;
        }
        let name = if name.trim().is_empty() {
            kind.deleted_name().to_string()
        } else {
            name
        };
        dialogs.push(Dialog {
            chat_id,
            kind,
            name,
            only_my: settings.only_my(section),
            folder: String::new(),
        });
        on_found(dialogs.len());
    }
    number(&mut dialogs);
    Ok((dialogs, protected))
}

/// `chats/chat_1/` ... padded to the width of `count - 1`, as tdesktop's
/// `FinalizeDialogsInfo` does.
pub fn number(dialogs: &mut [Dialog]) {
    let digits = dialogs.len().saturating_sub(1).to_string().len();
    for (i, dialog) in dialogs.iter_mut().enumerate() {
        dialog.folder = format!("chats/chat_{:0digits$}/", i + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dialog() -> Dialog {
        Dialog {
            chat_id: 1,
            kind: ChatKind::Personal,
            name: "A".into(),
            only_my: false,
            folder: String::new(),
        }
    }

    #[test]
    fn folders_are_numbered_like_tdesktop() {
        let mut one = vec![dialog()];
        number(&mut one);
        assert_eq!(one[0].folder, "chats/chat_1/");
        let mut twelve = vec![dialog(); 12];
        number(&mut twelve);
        assert_eq!(twelve[0].folder, "chats/chat_01/");
        assert_eq!(twelve[11].folder, "chats/chat_12/");
        // tdesktop pads to the width of `count - 1`: ten chats stay unpadded.
        let mut ten = vec![dialog(); 10];
        number(&mut ten);
        assert_eq!(ten[0].folder, "chats/chat_1/");
        assert_eq!(ten[9].folder, "chats/chat_10/");
    }

    #[test]
    fn kinds_map_to_options_and_names() {
        assert_eq!(ChatKind::SavedMessages.section(), Section::PersonalChats);
        assert_eq!(
            ChatKind::PrivateSupergroup.section(),
            Section::PrivateGroups
        );
        assert_eq!(ChatKind::PublicSupergroup.section(), Section::PublicGroups);
        assert_eq!(ChatKind::PublicChannel.json_type(), "public_channel");
        assert_eq!(ChatKind::Bot.list_type(), "bot");
        assert_eq!(ChatKind::PrivateChannel.deleted_name(), "Deleted Channel");
    }
}
