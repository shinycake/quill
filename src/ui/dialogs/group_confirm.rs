use quill::ids::ChatId;
/// Slice G1: confirmations that need an explicit tap: deleting a chat
/// (`deleteChat`), leaving a group/channel, the one-way broadcast
/// upgrade (`toggleSupergroupIsBroadcastGroup`), and banning a member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupConfirmAction {
    DeleteChat,
    LeaveChat,
    BroadcastUpgrade,
    /// Slice CL1: `deleteChatHistory` (schema 1.8.67, line 11845);
    /// `revoke` clears for everyone (`chat.can_be_deleted_for_all_users`).
    ClearHistory {
        revoke: bool,
    },
    /// Slice CL1: chat-list "Delete chat" — `deleteChatHistory` with
    /// `remove_from_chat_list: true` (Telegram X `Tdlib.deleteChat`),
    /// not the destructive `deleteChat` constructor.
    RemoveFromList,
    /// Slice CL3: row-menu Report — `reportChat` spam report (schema
    /// 1.8.67, line 15693).
    ReportChat,
    /// Slice CL3: row-menu Block/Unblock user —
    /// `setMessageSenderBlockList` (schema 1.8.67, line 14492).
    BlockUser {
        block: bool,
    },
    /// Slice A6: user-panel Block/Unblock — the user-scoped
    /// `setMessageSenderBlockList` twin of `BlockUser` for users reached
    /// from the Contacts tab, where there is no chat to resolve through
    /// (`driver.set_user_blocked`). The dialog's `chat_id` is a dummy.
    BlockContact {
        user_id: i64,
        block: bool,
    },
    /// Slice A6: user-panel "Delete contact" — `removeContacts`
    /// (schema 1.8.67, line 14528; TGX `DeleteContactConfirm`). The
    /// dialog's `chat_id` is a dummy.
    DeleteContact {
        user_id: i64,
    },
    /// Slice A6: "Delete synced contacts" — `clearImportedContacts` +
    /// `removeContacts` (TGX `SyncContactsDeleteInfo`). The dialog's
    /// `chat_id` is a dummy.
    DeleteSyncedContacts,
    /// Slice CL3: multi-select bulk delete — `deleteChatHistory` with
    /// `remove_from_chat_list: true` for every selected chat. The ids
    /// are read from the live selection at submit time (the dialog
    /// blocks selection changes while open).
    RemoveSelectedChats,
    /// Slice B2: "Restart bot" — clears the bot chat's history
    /// (`deleteChatHistory`, schema line 11845, kept in the chat list)
    /// and re-sends `sendBotStartMessage` with an empty parameter (schema
    /// line 12216). Telegram X's "Restart" (the unblock-slot action) only
    /// re-sends; the clear is the profile-action contract here.
    RestartBot,
    /// Slice A2: abort the pending recovery-email setup
    /// (`cancelRecoveryEmailAddressVerification`, schema 1.8.67, line
    /// 11467). TGX confirms via `AbortRecoveryEmailConfirm`; the abort
    /// carries no chat, so the dialog's chat id is a dummy.
    AbortRecoveryEmailSetup,
    /// Slice payments: "Clear saved payment/shipping info" —
    /// `deleteSavedOrderInfo` + `deleteSavedCredentials` (schema 1.8.67,
    /// lines 15286 / 15289). The dialog's chat id is a dummy.
    ClearPaymentInfo,
    RemoveInstalledStickerSets,
    RemoveSavedGif {
        file_id: quill::ids::FileId,
    },
    RemoveStickerSet {
        set_id: i64,
    },
}

pub struct GroupConfirmDialog {
    pub(crate) chat_id: ChatId,
    pub(crate) action: GroupConfirmAction,
}
