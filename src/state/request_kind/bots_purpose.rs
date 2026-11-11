//! Generated-once classification of every `BotsPurpose` variant.
use super::RequestKind;
use crate::state::BotsPurpose;

impl BotsPurpose {
    /// What this request is. Exhaustive: a new variant must be classified here.
    pub(crate) fn kind(&self) -> RequestKind {
        match self {
            Self::GetInlineQueryResults { .. }
            | Self::ResolveInlineBot { .. }
            | Self::GetCommands
            | Self::GetCallbackQueryAnswer
            | Self::GetCallbackQueryAnswerWithPassword
            | Self::GetCallbackQueryAnswerGame
            | Self::GetGameHighScores
            | Self::GetLoginUrlInfo
            | Self::GetLoginUrl
            | Self::GetBotSimilarBots
            | Self::GetChatReplyMarkupMessage
            | Self::GetRecentInlineBots
            | Self::GetWebAppUrl { .. }
            | Self::GetMainWebApp { .. }
            | Self::GetWebAppLinkUrl { .. }
            | Self::SearchWebApp { .. }
            | Self::CanBotSendMessages { .. }
            | Self::GetAttachmentMenuBot { .. }
            | Self::GetGrossingWebAppBots
            | Self::GetBotEditText { .. }
            | Self::GetWidgetQueryAnswer { .. }
            | Self::GetWidgetPublicData => RequestKind::Lookup,
            Self::GroupBotUser
            | Self::StopPendingMessage { .. }
            | Self::SendInlineQueryResult
            | Self::DeleteChatReplyMarkup
            | Self::SendBotStartMessage
            | Self::ShareWithBot
            | Self::OpenWebApp { .. }
            | Self::CloseWebApp
            | Self::SendWebAppData
            | Self::AllowBotToSendMessages { .. }
            | Self::SharePhoneWithMiniApp { .. }
            | Self::ToggleBotEmojiStatusAccess { .. }
            | Self::ToggleBotInAttachmentMenu { .. }
            | Self::SendWebAppCustomRequest
            | Self::SetMessageSenderBotVerification
            | Self::RemoveMessageSenderBotVerification
            | Self::SetBotName { .. }
            | Self::SetBotAbout { .. }
            | Self::SetBotPhoto { .. }
            | Self::CheckBotUsername { .. }
            | Self::CreateBot { .. } => RequestKind::UserAction,
        }
    }
}
