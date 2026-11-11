//! Generated-once classification of every `MessagesPurpose` variant.
use super::RequestKind;
use crate::state::MessagesPurpose;

impl MessagesPurpose {
    /// What this request is. Exhaustive: a new variant must be classified here.
    pub(crate) fn kind(&self) -> RequestKind {
        match self {
            Self::AiCompose(inner) => inner.kind(),
            Self::GetHistory
            | Self::GetFullRichMessage { .. }
            | Self::OpenChat
            | Self::CloseChat
            | Self::ViewMessages
            | Self::GetPinnedMessages
            | Self::GetHistoryAround
            | Self::GetHistoryNewer
            | Self::PreloadHistory
            | Self::GetMessageAvailableReactions { .. }
            | Self::SearchMentionMembers
            | Self::GetPollVoters { .. }
            | Self::GetPollVoteStatistics { .. }
            | Self::GetMessageLink
            | Self::GetMessageLinkProperties { .. }
            | Self::GetComposerReplyTarget { .. }
            | Self::GetMessageMenuActions { .. }
            | Self::GetSharingProtection { .. }
            | Self::GetMessageAuthor { .. }
            | Self::GetPollOptionProperties { .. }
            | Self::GetMessageViewers { .. }
            | Self::GetMessageReadDate { .. }
            | Self::GetMessageAddedReactions { .. }
            | Self::GetWebPageInstantView
            | Self::GetLinkPreview
            | Self::GetChatScheduledMessages
            | Self::OpenMessageContent
            | Self::GetChatSponsoredMessages
            | Self::ViewSponsoredMessages
            | Self::SearchShareChats
            | Self::SearchShareChatsOnServer
            | Self::GetChatAvailableMessageSenders => RequestKind::Lookup,
            Self::SendMessage
            | Self::SendMessageAlbum
            | Self::JumpToUnread { .. }
            | Self::ReadAllUnreadMarkers { .. }
            | Self::EditMessage
            | Self::DeleteMessages
            | Self::ForwardMessages
            | Self::AddMessageReaction
            | Self::RemoveMessageReaction
            | Self::PinChatMessage
            | Self::SetPollAnswer
            | Self::AddPollOption
            | Self::MarkChecklistTasks
            | Self::AddChecklistTasks
            | Self::StopPoll
            | Self::StopLiveLocation
            | Self::UnpinChatMessage
            | Self::UnpinAllChatMessages
            | Self::ReportMessages
            | Self::DeletePollOption
            | Self::DeleteChatMessagesBySender
            | Self::ReportSupergroupSpam
            | Self::DeleteMessageReactionsFromSender { .. }
            | Self::ResendMessages
            | Self::EditMessageSchedulingState { .. }
            | Self::SendChatAction
            | Self::RecognizeSpeech
            | Self::SetMessageFactCheck
            | Self::SetChatDraftMessage
            | Self::ReportChatSponsoredMessage
            | Self::ToggleHasSponsoredMessagesEnabled
            | Self::ClickChatSponsoredMessage
            | Self::SetChatMessageSender
            | Self::ToggleChatIsTranslatable
            | Self::ToggleSupergroupAutoTranslate
            | Self::TranslateJob { .. }
            | Self::FixTextWithAi
            | Self::ComposeTextWithAi
            | Self::ComposeRichMessageWithAi
            | Self::CreateRichMessageWithAi
            | Self::FixRichMessageWithAi => RequestKind::UserAction,
        }
    }
}
