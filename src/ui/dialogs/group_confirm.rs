use quill::ids::ChatId;
/// Slice G1: confirmations that need an explicit tap: deleting a chat
/// (`deleteChat`), leaving a group/channel, the one-way broadcast
/// upgrade (`toggleSupergroupIsBroadcastGroup`), and banning a member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupConfirmAction {
    RemoveEmojiSet {
        set_id: i64,
    },
    DeleteChat {
        chat_id: ChatId,
    },
    LeaveChat {
        chat_id: ChatId,
    },
    /// First step of the broadcast group conversion: what it offers
    /// (tdesktop `lng_gigagroup_convert_*`). Confirming opens the warning.
    BroadcastIntro {
        chat_id: ChatId,
    },
    /// Second step: the permanent consequence, then the conversion runs.
    BroadcastUpgrade {
        chat_id: ChatId,
    },
    /// Slice CL1: chat-list "Delete chat" — `deleteChatHistory` with
    /// `remove_from_chat_list: true` (Telegram X `Tdlib.deleteChat`),
    /// not the destructive `deleteChat` constructor.
    RemoveFromList {
        chat_id: ChatId,
    },
    /// Slice CL3: row-menu Block/Unblock user —
    /// `setMessageSenderBlockList` (schema 1.8.67, line 14492).
    BlockUser {
        chat_id: ChatId,
        block: bool,
    },
    /// Slice A6: user-panel Block/Unblock — the user-scoped
    /// `setMessageSenderBlockList` twin of `BlockUser` for users reached
    /// from the Contacts tab, where there is no chat to resolve through
    /// (`driver.set_user_blocked`).
    BlockContact {
        user_id: i64,
        block: bool,
    },
    /// Slice A6: user-panel "Delete contact" — `removeContacts`
    /// (schema 1.8.67, line 14528; TGX `DeleteContactConfirm`).
    DeleteContact {
        user_id: i64,
    },
    /// Slice A6: "Delete synced contacts" — `clearImportedContacts` +
    /// `removeContacts` (TGX `SyncContactsDeleteInfo`).
    DeleteSyncedContacts,
    /// Slice CL3: multi-select bulk delete — `deleteChatHistory` with
    /// `remove_from_chat_list: true` for every chat in `ids`, the
    /// selection captured when the box opened.
    RemoveSelectedChats {
        ids: Vec<ChatId>,
    },
    /// Slice B2: "Restart bot" — clears the bot chat's history
    /// (`deleteChatHistory`, schema line 11845, kept in the chat list)
    /// and re-sends `sendBotStartMessage` with an empty parameter (schema
    /// line 12216). Telegram X's "Restart" (the unblock-slot action) only
    /// re-sends; the clear is the profile-action contract here.
    RestartBot {
        chat_id: ChatId,
    },
    /// Slice payments: "Clear saved payment/shipping info" —
    /// `deleteSavedOrderInfo` + `deleteSavedCredentials` (schema 1.8.67,
    /// lines 15286 / 15289).
    ClearPaymentInfo,
    RemoveSavedGif {
        file_id: quill::ids::FileId,
    },
    /// Subsection tabs: the tab menu's "Delete" — `deleteForumTopic`
    /// (schema 1.8.67, line 12736) for the dialog's chat.
    DeleteForumTopic {
        chat_id: ChatId,
        forum_topic_id: i32,
    },
    /// Community info panel "Delete community" — `deleteCommunity`
    /// (TDLib 1.8.68; owner only).
    DeleteCommunity {
        community_id: i64,
    },
    /// Saved Messages: "Delete chat" on a sublist —
    /// `deleteSavedMessagesTopicHistory` (schema 1.8.67, line 11781).
    DeleteSavedSublist {
        topic_id: i64,
    },
    /// Member list: "Remove from group" (tdesktop `lng_profile_sure_kick`)
    /// — `banChatMember` in a basic group, ban then lift in a supergroup.
    RemoveMember {
        chat_id: ChatId,
        user_id: i64,
    },
}

/// The open confirm box. Each action carries its own target.
pub struct GroupConfirmDialog {
    pub(crate) action: GroupConfirmAction,
}

impl GroupConfirmAction {
    /// The chat a chat-scoped action works on; `None` for the actions on a
    /// user, a community, a list of chats or the account.
    pub(crate) fn chat_id(&self) -> Option<ChatId> {
        match self {
            Self::DeleteChat { chat_id }
            | Self::LeaveChat { chat_id }
            | Self::BroadcastIntro { chat_id }
            | Self::BroadcastUpgrade { chat_id }
            | Self::RemoveFromList { chat_id }
            | Self::BlockUser { chat_id, .. }
            | Self::RestartBot { chat_id }
            | Self::DeleteForumTopic { chat_id, .. }
            | Self::RemoveMember { chat_id, .. } => Some(*chat_id),
            _ => None,
        }
    }
}
