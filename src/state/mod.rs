use crate::auth::{AuthView, view_for};
use crate::calls::engine::{RemoteVideoState, TransportState};
use crate::chatlist_style::{ChatPreviewStyle, preview_sender_name, preview_style};
use crate::composer::{CommandMenuItem, ComposerScheduling, merge_command_menu_items};
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
    CallPrefs, ContactPrefs, LanguagePrefs, MediaPrefs,
};
use crate::sticker_suggest::StickerSuggestMode;
use crate::story_page::{
    ArchivedStories, ChatPageStories, StoryPageOp, StoryPageOpState, story_page_op_label,
};
use crate::telegram::client::OwnedEnvelope;
use crate::telegram::envelope::{
    AnimationItem, AuthorizationState, Background, BotCommand, BotInfo, CallbackQueryAnswer,
    CanPostStoryResult, CanTransferOwnershipResult, ChannelMemberStatus, ChatAction, ChatActionBar,
    ChatActiveStoriesView, ChatAdminRights, ChatAdministratorEntry, ChatDraft, ChatFolderInfo,
    ChatFolderInviteLink, ChatFolderInviteLinkInfo, ChatFolderSpec, ChatJoinResult, ChatKind,
    ChatList, ChatNotificationSettings, ChatPermissions, ChatPositionUpdate, ChatStatistics,
    ConnectionState, EnvelopePayload, EphemeralMessageContent, ErrorClass, ForumTopic,
    GameHighScore, GameInfo, InlineQueryResultSummary, InlineQueryResultsButton,
    InlineQueryResultsPage, InviteGroupCallParticipantResult, LinkPreview, LoginUrlInfo,
    MessageAutoDelete, MessageContent, MessageExtras, MessageForwardInfo, MessageInteractionInfo,
    MessageOrigin, MessageReaction, MessageReplyTo, MessageSelfDestruct, MessageSender,
    NotificationSettingsScope, NotificationSound, OptionValue, ParsedCall, ParsedChatEvent,
    ParsedChatInviteLink, ParsedChatJoinRequest, ParsedChatMember, ParsedCommunity,
    ParsedCommunityFullInfo, ParsedFile, ParsedGroupCall, ParsedGroupCallMessage,
    ParsedGroupCallParticipant, ParsedMessage, ParsedSecretChat, ParsedSession, ParsedStory,
    ParsedUser, ParsedVideoChat, ParsedWebsite, ParsedWelcomeMessage, PasswordState,
    PaymentFormData, PaymentReceiptData, Poll, ReactionNotificationSettings, RecommendedChatFolder,
    ReplyKeyboard, ReplyMarkup, ReportChatOutcome, ReportOption, ReportSponsoredResult,
    ReportStoryResult, RichMessageContent, ScopeNotificationSettings, SecretChatState,
    SponsoredMessage, StarSubscriptionsData, StickerItem, StickerSetInfo, StorageStats,
    StoryAvailableReactionView, StoryInteractionView, StoryInteractionsView, StoryListView,
    TdError, UsernameCheckResult, ValidatedOrderInfoData, effective_content,
    reply_markup_demands_reply,
};
use crate::telegram::envelope::{AvailableMessageSender, CallState, ReadyParams};
use crate::telegram::envelope_story::ParsedStoryAlbum;
use crate::telegram::profile_accent::ProfileAccentColor;
use crate::telegram::requests::{
    ArchiveChatListSettings, CallPrivacySetting, ChatEventLogFilterSet, PrivacyWho,
};
use crate::telegram::requests_privacy::PrivacySettingKey;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;

mod account_notices;
mod call_types;
mod chat_activity;
mod chat_types;
mod domains;
mod history_calendar;
mod history_trim;
mod history_types;
mod info_types;
mod map_thumbs;
mod media_library;
mod member_list;
mod ownership_types;
mod paging;
mod redraw;
mod request_purpose;
mod request_rollback;
mod requests;
mod saved_types;
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
mod session_chat_look;
mod session_chat_search;
mod session_chatlist;
mod session_connection;
mod session_date_jump;
mod session_files;
mod session_forum;
mod session_main_profile_tab;
pub use session_forum::{FORUM_COLUMN_COLLAPSE_BELOW, FORUM_COLUMN_WIDTH, ForumColumn};
mod session_forward;
mod session_group_admin;
mod session_history_window;
mod session_links_boosts;
mod session_members;
mod session_message_menu;
mod session_new;
mod session_notifications;
mod session_privacy_data;
mod session_profile_panels;
mod session_proxy;
mod session_reply;
mod session_reply_keyboard;
mod session_requests;
mod session_saved;
mod session_search;
mod session_service;
mod session_share;
mod session_sponsored;
mod session_stickers;
mod session_stories;
pub use session_stories::STORY_CUSTOM_EMOJI_CAP;
mod session_subsection_tabs;
mod session_thread;
mod session_translate;
mod session_updates;
mod session_web_apps;
mod shared_media_types;
mod sticker_gif_types;
mod story_insights;
mod story_types;
mod thread_types;
mod updates_sync;
mod web_app_types;

pub use account_notices::*;
pub use call_types::*;
pub use chat_activity::*;
pub use chat_types::*;
pub use domains::*;
pub use history_calendar::*;
pub use history_trim::{
    HISTORY_WINDOW_CAP, HISTORY_WINDOW_TRIM_TO, RowWindowShift, WindowEnd, row_window_shift,
};
pub use history_types::*;
pub use info_types::*;
pub use map_thumbs::{
    MAP_THUMB_HEIGHT, MAP_THUMB_SCALE, MAP_THUMB_WIDTH, MAP_THUMB_ZOOM, MapKey, MapThumbs,
};
pub use media_library::{MAX_LIBRARY_LOADS, MediaLibrary, MessageReactionOptions, ReactionChoice};
pub use member_list::*;
pub use ownership_types::*;
pub use redraw::{RedrawNeed, redraw_need};
pub use request_purpose::*;
pub use request_rollback::*;
pub use requests::*;
pub use saved_types::*;
pub use search_types::*;
pub use session::*;
pub use session_chat_caps::FastButtonTarget;
pub(crate) use session_chat_search::history_message;
pub use session_connection::*;
pub use session_group_admin::*;
pub use session_history_window::MentionSearch;
pub use session_links_boosts::*;
pub use session_message_menu::{
    Audience, CustomEmojiPreview, MessageAudience, MessageReportFlow, MessageReportStage,
    StickerSetView, StickerSetViewStage, reaction_filter_key,
};
pub use session_privacy_data::{PasswordCheck, PrivacyData};
pub use session_proxy::LINK_PING_ID;
pub use session_reply::{
    ForwardHeader, ForwardLink, ReplyHeader, ReplyState, ReplyTarget, footer_tooltip,
    thumb_candidates,
};
pub use session_reply_keyboard::*;
pub use session_saved::{SAVED_PAGE, SAVED_TOPICS_PAGE};
pub use session_share::ShareSearch;
pub use session_subsection_tabs::{BotTopics, TopicBadge};
pub use session_translate::*;
pub use shared_media_types::*;
pub use sticker_gif_types::*;
pub use story_insights::*;
pub use story_types::*;
pub use thread_types::*;
pub use updates_sync::*;
pub use web_app_types::*;

#[cfg(test)]
mod tests;
