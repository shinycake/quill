use crate::auth::{AuthView, view_for};
use crate::calls::engine::{RemoteVideoState, TransportState};
use crate::chatlist_style::{ChatPreviewStyle, preview_sender_name, preview_style};
use crate::composer::{CommandMenuItem, merge_command_menu_items};
use crate::data_settings::{AutoDownloadNetSettings, DataStoragePrefs, NetworkKind};
use crate::diagnostics::{Diagnostic, DiagnosticSink};
use crate::emoji::EmojiPanel;
use crate::ids::{
    AccountGeneration, AccountKey, ChatId, FileId, MessageId, RequestId, ViewGeneration,
};
use crate::notify::{self, OsNotification, QueuedNotification};
use crate::privacy::{PrivacyKeyState, PrivacyRuleDetail};
use crate::settings::BadgePrefs;
use crate::settings::{
    AUTO_DOWNLOAD_FILE, AUTO_DOWNLOAD_GIF, AUTO_DOWNLOAD_MAX_BYTES, AUTO_DOWNLOAD_MUSIC,
    AUTO_DOWNLOAD_PHOTO, AUTO_DOWNLOAD_VIDEO, AUTO_DOWNLOAD_VIDEO_NOTE, AUTO_DOWNLOAD_VOICE,
    CallPrefs, ContactPrefs, MediaPrefs,
};
use crate::sticker_suggest::StickerSuggestMode;
use crate::story_page::{
    ArchivedStories, ChatPageStories, StoryPageOp, StoryPageOpState, story_page_op_label,
};
use crate::telegram::client::OwnedEnvelope;
use crate::telegram::envelope::{
    AnimationItem, AuthorizationState, BotCommand, BotInfo, CallbackQueryAnswer,
    CanPostStoryResult, ChannelMemberStatus, ChatAction, ChatActiveStoriesView, ChatAdminRights,
    ChatAdministratorEntry, ChatDraft, ChatFolderInfo, ChatFolderSpec, ChatJoinResult, ChatKind,
    ChatList, ChatNotificationSettings, ChatPermissions, ChatPositionUpdate, ChatStatistics,
    ConnectionState, EnvelopePayload, EphemeralMessageContent, ErrorClass, ForumTopic,
    GameHighScore, GameInfo, InlineQueryResultSummary, InlineQueryResultsButton,
    InlineQueryResultsPage, InviteGroupCallParticipantResult, LinkPreview, LoginUrlInfo,
    MessageAutoDelete, MessageContent, MessageForwardInfo, MessageInteractionInfo, MessageOrigin,
    MessageReaction, MessageReplyTo, MessageSelfDestruct, MessageSender, NotificationSettingsScope,
    NotificationSound, OptionValue, ParsedCall, ParsedChatEvent, ParsedChatInviteLink,
    ParsedChatJoinRequest, ParsedChatMember, ParsedCommunity, ParsedCommunityFullInfo, ParsedFile,
    ParsedGroupCall, ParsedGroupCallMessage, ParsedGroupCallParticipant, ParsedMessage,
    ParsedSecretChat, ParsedSession, ParsedStory, ParsedUser, ParsedVideoChat, ParsedWebsite,
    ParsedWelcomeMessage, PasswordState, PaymentFormData, PaymentReceiptData, Poll,
    ReactionNotificationSettings, ReplyKeyboard, ReplyMarkup, ReportChatOutcome, ReportOption,
    ReportSponsoredResult, ReportStoryResult, RichMessageContent, ScopeNotificationSettings,
    SecretChatState, SponsoredMessage, StarSubscriptionsData, StickerFormat, StickerItem,
    StickerSetInfo, StorageStats, StoryAvailableReactionView, StoryInteractionView,
    StoryInteractionsView, StoryListView, TdError, UsernameCheckResult, ValidatedOrderInfoData,
    effective_content, reply_markup_demands_reply,
};
use crate::telegram::envelope::{CallState, ReadyParams};
use crate::telegram::envelope_story::ParsedStoryAlbum;
use crate::telegram::profile_accent::ProfileAccentColor;
use crate::telegram::requests::{
    ArchiveChatListSettings, CallPrivacySetting, ChatEventLogFilterSet, PrivacyWho,
};
use crate::telegram::requests_privacy::PrivacySettingKey;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;

mod call_types;
mod chat_types;
mod history_types;
mod info_types;
mod member_list;
mod request_purpose;
mod requests;
mod search_types;
mod session;
mod session_apply;
mod session_apply_chats;
mod session_apply_error;
mod session_apply_messages;
mod session_apply_ok;
mod session_apply_queries;
mod session_apply_users;
mod session_calls;
mod session_chat_caps;
mod session_chat_search;
mod session_chatlist;
mod session_files;
mod session_forum;
mod session_forward;
mod session_members;
mod session_notifications;
mod session_requests;
mod session_search;
mod session_sponsored;
mod session_stickers;
mod session_stories;
mod session_updates;
mod shared_media_types;
mod sticker_gif_types;
mod story_types;

pub use call_types::*;
pub use chat_types::*;
pub use history_types::*;
pub use info_types::*;
pub use member_list::*;
pub use request_purpose::*;
pub use requests::*;
pub use search_types::*;
pub use session::*;
pub(crate) use session_chat_search::history_message;
pub use shared_media_types::*;
pub use sticker_gif_types::*;
pub use story_types::*;

#[cfg(test)]
mod tests;
