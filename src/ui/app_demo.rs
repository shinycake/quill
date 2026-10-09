//! new_with_demo constructor (slimmed), demo_seed_for seed table, demo_pending_attachments.

use super::app::{ChatListFilter, QuillApp};
use super::connect_ui::ConnectUiStatus;
use super::demo::{
    demo_media_allowlist, seed_ready_animated_emoji_session, seed_ready_chats_session,
    seed_ready_custom_emoji_session, seed_ready_downloads_session, seed_ready_media_session,
    seed_ready_offline_session, seed_ready_reconnecting_session, seed_ready_send_media_session,
    seed_ready_unread_read_session, seed_ready_unread_session,
};
use super::history::HistoryShared;
use super::screenshot_demo::ScreenshotDemo;
use super::synthetic::SyntheticChat;
use super::*;
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::component::menu::AppMenuBar;
use gpui_kit::component::message_scroller::MessageScrollerState;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::composer::{
    AttachmentKind, ComposerAttachment, ComposerScheduling, PreviewMediaSize, should_send_on_enter,
};
use quill::connect::ConnectBlocker;
use quill::credentials::TelegramCredentials;
use quill::diagnostics::MemorySink;
use quill::media_viewer::{MediaViewer, ViewerZoom};
use quill::state::Session;
use quill::story_composer::StoryComposer;
use quill::story_viewer::{StoryPlayback, StoryViewer};
use quill::telegram::envelope::AuthorizationState;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize};
use std::time::Duration;
/// Screenshot-demo seed table: maps each [`ScreenshotDemo`] variant to its
/// session seed, connect status, status note, and auth state. Pure lookup —
/// no behavior change from the inlined match arms it replaces.
pub(super) fn demo_seed_for(
    demo: ScreenshotDemo,
) -> (
    Option<fn(Arc<MemorySink>) -> Session>,
    ConnectUiStatus,
    String,
    AuthorizationState,
) {
    match demo {
        ScreenshotDemo::NeedTdjson => (
            None,
            ConnectUiStatus::NeedTdjson,
            ConnectBlocker::MissingTdjson.user_message().into(),
            AuthorizationState::WaitPhoneNumber,
        ),
        ScreenshotDemo::WaitPhone => (
            None,
            ConnectUiStatus::DemoWaitPhone,
            "screenshot demo — WaitPhoneNumber (injected auth, no live Telegram)".into(),
            AuthorizationState::WaitPhoneNumber,
        ),
        ScreenshotDemo::WaitCode => (
            None,
            ConnectUiStatus::DemoWaitCode,
            "screenshot demo — WaitCode (injected auth, no live Telegram)".into(),
            AuthorizationState::WaitCode {
                code_length: Some(5),
            },
        ),
        ScreenshotDemo::WaitPassword => (
            None,
            ConnectUiStatus::DemoWaitPassword,
            "screenshot demo — WaitPassword (injected auth, no live Telegram)".into(),
            AuthorizationState::WaitPassword {
                has_recovery_email: true,
            },
        ),
        ScreenshotDemo::ConnectionClosed => (
            None,
            ConnectUiStatus::DemoWaitPhone,
            "screenshot demo — Closed (injected auth, no live Telegram)".into(),
            AuthorizationState::Closed,
        ),
        ScreenshotDemo::WaitPremium => (
            None,
            ConnectUiStatus::DemoWaitPhone,
            "screenshot demo — WaitPremiumPurchase (injected auth, no live Telegram)".into(),
            AuthorizationState::WaitPremiumPurchase,
        ),
        ScreenshotDemo::WaitQr => (
            None,
            ConnectUiStatus::DemoWaitQr,
            "screenshot demo — WaitOtherDeviceConfirmation (injected auth, no live Telegram)"
                .into(),
            // Fake link for the demo QR; never touches the network.,
            AuthorizationState::WaitOtherDeviceConfirmation {
                link: "tg://login/?token=demo_qr_login_token_not_for_network".into(),
            },
        ),
        ScreenshotDemo::ReadyUpdateInstall | ScreenshotDemo::ReadyUpdateChangelog | ScreenshotDemo::ReadyUpdateFailure | ScreenshotDemo::ReadyTrayBehavior | ScreenshotDemo::ReadyDeepLinkInfo | ScreenshotDemo::ReadyDeepLinkInvite | ScreenshotDemo::ReadyDeepLinkShare | ScreenshotDemo::ReadyChats | ScreenshotDemo::ReadyChatsComposer | ScreenshotDemo::ReadySuggestHashtag | ScreenshotDemo::ReadySuggestEmoji | ScreenshotDemo::ReadyAppearance | ScreenshotDemo::ReadySpellcheck | ScreenshotDemo::ReadySpellcheckPanel | ScreenshotDemo::ReadySpellcheckToggle | ScreenshotDemo::ReadyKeybindings | ScreenshotDemo::ReadyAccounts | ScreenshotDemo::ReadyPasscodeSettings | ScreenshotDemo::ReadyPasscodeCreate | ScreenshotDemo::ReadyLockScreen => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — Ready chat list (injected updates, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        // Slice parity:platform-offline-indicator — same chat list, but
        // the fixture reports WaitingForNetwork so the offline banner
        // renders.
        ScreenshotDemo::ReadyOffline => (
            Some(seed_ready_offline_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — offline indicator (injected updates, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        // Slice parity:platform-offline-errors — offline banner + kit toast
        // with the product offline-send note (status line).
        ScreenshotDemo::ReadyOfflineToast => (
            Some(seed_ready_offline_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "You're offline — will send when you reconnect".into(),
            AuthorizationState::Ready,
        ),
        // Slice parity:platform-reconnect-states — same chat list, but
        // the fixture reports Updating so the transitional strip renders
        // with its per-state label.
        ScreenshotDemo::ReadyReconnecting => (
            Some(seed_ready_reconnecting_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — reconnecting indicator (injected updates, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyUnread => (
            Some(seed_ready_unread_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — unread badge (injected updates, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyUnreadRead => (
            Some(seed_ready_unread_read_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — after mark-read (injected updates, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyMedia => (
            Some(seed_ready_media_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — photo/document (injected updates, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyDownloads => (
            Some(seed_ready_downloads_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — downloads manager (injected updates, no live Telegram)"
                .into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadySendMedia => (
            Some(seed_ready_send_media_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — outgoing photo/document send (injected, no live Telegram)"
                .into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyPasteImage => (
            Some(seed_ready_send_media_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — paste clipboard image as photo attachment (injected, no live Telegram)"
                .into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadySearch => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — sidebar search (injected searchChats / searchMessages)"
                .into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadySearchInChat => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — in-chat search (injected searchChatMessages)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyReply => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — reply to message (injected reply_to)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyEditDelete => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — edit + delete own messages (injected)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyForward => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — forward message(s) (injected forwardMessages)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyShareBox
        | ScreenshotDemo::ReadyForwardBar
        | ScreenshotDemo::ReadySendAs => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — share box, forward bar and send as (injected)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadySelectMode => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — message selection mode (injected)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyReplyMedia => (
            Some(seed_ready_media_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — reply bar with a media thumbnail (injected)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyEditMedia => (
            Some(seed_ready_send_media_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — edit bar with a media thumbnail (injected)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyReveal => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — new message reveal (injected)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyReactions => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — emoji reactions (injected interaction_info)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyPin => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — pin / unpin (injected updateMessageIsPinned)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyMuteArchive => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — mute / archive (injected notification + chat list updates)"
                .into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyChatListMenu => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — chat list: pinned, archived, marked unread, row menu".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyChatList => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — chat list: folders, category filters, pinned drag, archive"
                .into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyChatListArchive => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — chat list: archive settings dialog".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyArchiveRow
        | ScreenshotDemo::ReadyArchiveBar
        | ScreenshotDemo::ReadyArchiveMenu
        | ScreenshotDemo::ReadyPinDrag => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — chat list: archive row · story rings · pinned drag".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyChatListSearch => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — chat list: search empty state".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadySharedMedia => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — shared media gallery: per-tab empty states".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyChatList3 => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — chat list: mentions · reactions · multi-select".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyChatPreview => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — chat list peek preview (injected updates, no live Telegram)"
                .into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyNotificationSound => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — notification sounds + settings (injected saved sounds + chat/scope settings)"
                .into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadySwipeMute
        | ScreenshotDemo::ReadySwipeReached
        | ScreenshotDemo::ReadyStoriesExpanded
        | ScreenshotDemo::ReadyStoriesCollapsing
        | ScreenshotDemo::ReadyStoriesCollapsed => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — chat list: swipe actions · stories strip".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyChatRows => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — chat rows: drafts · send state · title badges".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyJoinBar
        | ScreenshotDemo::ReadyTopBars
        | ScreenshotDemo::ReadySearchPreviews
        | ScreenshotDemo::ReadyMultilineRows => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — join bar · search and chat-row previews".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyTyping => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — peer typing (injected updateChatAction)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyStickers | ScreenshotDemo::ReadyStickerPlayback => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — sticker panel + sticker in history".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyVoice => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — voice record bar + history playback".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyGameCard => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — game card + high scores".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyLinkPreview => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — link + web page preview".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyComposerPreview => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — composer link preview chip".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyPreviewCards => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — embedded + album preview cards".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyCaptionPosition => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — caption above vs below".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyGifs | ScreenshotDemo::ReadyGifPlayback => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — saved GIFs + history playback".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyVideo => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — video bubble playback".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyVideoNote => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — round video note playback".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyAudio => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — audio file playback".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyPlayerBar => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — player bar".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyVideoSend => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — local video attach + own-sent playback".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyVideoNoteSend => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — local video note attach + own-sent round note".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyDrafts => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — restored private-chat draft".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyAlbums => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — received album and own-sent album".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyEmojiPanel => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — emoji panel".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyMentions => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — @ member suggestions".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadySponsored => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — sponsored / recommended channel rows".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyEmojiPacks | ScreenshotDemo::ReadyCustomEmoji => (
            Some(seed_ready_custom_emoji_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — custom emoji rendered inline in message text".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyAnimatedEmoji => (
            Some(seed_ready_animated_emoji_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — animated emoji suggestion above the composer".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyChannels => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — broadcast channel posts + join footer".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyChannelsAdmin => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — broadcast channel, admin composer".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyChannelStats => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — channel statistics".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyInviteLinks => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — invite links".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyAdminManagement => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — admin management".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyAdminLog => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — recent actions".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyGroupManage => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — group management".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyGroupInfoEdit => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — group info edit".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyGroups2 => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — groups/channels G2".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyCommunityCreate | ScreenshotDemo::ReadyCommunityHub | ScreenshotDemo::ReadyCommunityInfo => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — communities G10".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyBotChat => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — bot chat with info panel".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyInlineResults => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — inline-mode results dropdown".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyBotKeyboard => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — B1 bot keyboards: inline buttons, custom keyboard, force reply".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyRichMessage => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — rich message blocks".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyRichEditor => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — rich editor".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyRichAiTools => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — rich editor AI tools".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyRichPremiumGate => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "Rich messages require Telegram Premium".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyMemberModeration => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — member moderation (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyGroupAdminSettings => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — group and channel settings (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyProfilePanels => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — profile and contact panels (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyProfileEdit | ScreenshotDemo::ReadyUsername => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — edit profile dialog (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        // Slice parity:platform-shortcuts-reference: the shortcuts dialog
        // opens over the seeded chat list (see demo_setup.rs).
        ScreenshotDemo::ReadyShortcuts => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — keyboard shortcuts reference (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        // `parity:proxy-settings`: proxy list / editor / link box over the
        // seeded chat list (`QUILL_DEMO_PROXY=list|edit|link|link-bad`).
        ScreenshotDemo::ReadyProxy => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — proxy settings (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyScheduled => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — scheduled messages (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyJumpDate
        | ScreenshotDemo::ReadySearchFrom
        | ScreenshotDemo::ReadySearchFromHits
        | ScreenshotDemo::ReadySearchFilters
        | ScreenshotDemo::ReadySearchFrequent
        | ScreenshotDemo::ReadySearchPublic => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — find in history (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyBotCommandMenu => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — bot chat with / command menu".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyBotProfile => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — B2 bot profile actions".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyUnsupportedMessage => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — unsupported message (no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyTextEntities => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — text entities in text + caption".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyBlockquoteExpandable => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — expandable block quotes".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyRtlComposer => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — RTL composer draft".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyRtlPolish => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — RTL polish".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyMessageMenu => (
            Some(super::message_menu_demo::seed_ready_message_menu_session
                as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — message menu".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyServiceMessages => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — service messages".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyThreads => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — comments and threads".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyForumsSaved => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — forums and saved sublists".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyBubbleHeaders => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — bubble headers".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyReplyKeyboard => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — bot reply keyboard".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyTranslate => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — translation".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyShowcase => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — showcase".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyPoll => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — polls: voted + closed".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyPayments => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — payments: invoice + checkout".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadySubscriptions => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — ⭐ subscriptions (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyMarketplaceGift => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — collectible gift quote (no purchase)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyLocation => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — location / venue / contact".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyDice => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — dice rolls".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyMediaViewer => (
            Some(seed_ready_media_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — fullscreen media viewer".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyVideoPlayback | ScreenshotDemo::ReadyVideoPip => (
            Some(seed_ready_media_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — in-viewer video playback".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyViewerGif | ScreenshotDemo::ReadyViewerShared => (
            Some(seed_ready_custom_emoji_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — viewer GIF loop / Shared Media paging".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyStories => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — story viewer".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyStoryPost => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — story reactions / reply / delete".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyStoryAlbums => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — story albums / chat page / archive".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyStoryComposer => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — story posting composer".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyStoryViewers => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — story viewers list".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyStoryEdit => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — story edit composer".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyStoryAreas => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — clickable story areas".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadySeekBars => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — audio/voice seek bars".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyForumTopics => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — forum topics".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyBotTopics
        | ScreenshotDemo::ReadyBotTopicsBottom
        | ScreenshotDemo::ReadyBotTopicsLeft => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — bot topic tabs".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyTopicPost => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — posting to a forum topic".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyAvatarProfile => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — group member profile from an avatar click".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyContacts => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — contacts & profile".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyContactsManage => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — contacts management".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyBlockUser => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — block user confirm".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyFolders => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — chat folder tabs (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyFoldersManage => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — folder management (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyChatAvatars => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — chat avatars & channel header (injected, no live Telegram)"
                .into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadySlowMode => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — slow-mode enforcement (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadySecretChat => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — secret chat lifecycle (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadySecretPicker => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — new secret chat picker (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadySecretBotAlert => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — inline-bot warning (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyStorageUsage => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — data & storage (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::Ready2faManage => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — two-step verification (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadySessions => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — active sessions (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyNewLogin => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — ready-new-login (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyLoginPrevented => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — ready-login-prevented (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyServiceNotice => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — ready-service-notice (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyTerms => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — ready-terms (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyLocalStorage => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — ready-local-storage (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::Ready2faForgot => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — ready-2fa-forgot (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::Ready2faReset => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — ready-2fa-reset (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyLoginEmail => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — ready-login-email (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyRecoveryEmail => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — recovery email pending (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyAccountLifecycle => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — account lifecycle (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyWebSessions => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — connected websites (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadySessionToggles => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — session acceptance toggles (injected, no live Telegram)"
                .into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyKeyVerification => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — secret chat key verification (injected, no live Telegram)"
                .into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadySelfDestruct => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — self-destructing media (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyCall => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — incoming call (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyCallSwap => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — swap prompt (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyCallVideo => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — connected video call (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyCallScreenShare | ScreenshotDemo::ReadyCallScreenShareReceive => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — connected video call, screen-share (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyCallDevices => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — connected call audio devices (injected, no live Telegram)"
                .into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyCallReconnecting => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — reconnecting call audio (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyMuteCustom => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — mute menu with custom duration (injected, no live Telegram)"
                .into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyAutoDelete => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — auto-delete timer in a regular chat (injected, no live Telegram)"
                .into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyChatTtl => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — chat self-destruct timer (injected, no live Telegram)"
                .into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyGroupCall => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — group voice chat (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyGroupCallInvite => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — group voice chat invite picker (injected, no live Telegram)"
                .into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyGroupCallInvitation => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — incoming voice-chat invitation (injected, no live Telegram)"
                .into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyGroupCallManage => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — voice chat management (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyGroupCallScheduled => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — scheduled voice chat (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyCallsSettings => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — recent calls + call settings (injected, no live Telegram)"
                .into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyPrivacy | ScreenshotDemo::ReadyPrivacyGifts => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — privacy settings (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadySessionDetails | ScreenshotDemo::ReadyFileOpenConfirm => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — security settings (injected, no live Telegram)".into(),
            AuthorizationState::Ready,
        ),
    }
}

/// Builds the composer pending-attachment list for media screenshot demos.
pub(super) fn demo_pending_attachments(demo: Option<ScreenshotDemo>) -> Vec<ComposerAttachment> {
    let mut pending_attachments: Vec<ComposerAttachment> = Vec::new();
    if matches!(demo, Some(ScreenshotDemo::ReadySendMedia))
        && let Some(att) = ComposerAttachment::pick(
            &demo_media_allowlist().join("demo-notes.txt"),
            AttachmentKind::Document,
        )
    {
        ComposerAttachment::push_attachment(&mut pending_attachments, att);
    }
    if matches!(demo, Some(ScreenshotDemo::ReadyPasteImage))
        && let Some(att) = ComposerAttachment::pick(
            &demo_media_allowlist().join("demo-thumb.png"),
            AttachmentKind::Photo,
        )
    {
        ComposerAttachment::push_attachment(&mut pending_attachments, att);
    }
    if matches!(demo, Some(ScreenshotDemo::ReadyVideoSend))
        && let Some(att) = ComposerAttachment::pick(
            &demo_media_allowlist().join("demo-clip.mp4"),
            AttachmentKind::Video,
        )
    {
        ComposerAttachment::push_attachment(&mut pending_attachments, att);
    }
    if matches!(demo, Some(ScreenshotDemo::ReadyVideoNoteSend))
        && let Some(att) = ComposerAttachment::pick(
            &demo_media_allowlist().join("demo-video-note.mp4"),
            AttachmentKind::VideoNote,
        )
    {
        ComposerAttachment::push_attachment(&mut pending_attachments, att);
    }
    // Phase B3: pending photo for the self-destruct picker demo.
    if matches!(demo, Some(ScreenshotDemo::ReadySelfDestruct))
        && let Some(att) = ComposerAttachment::pick(
            &demo_media_allowlist().join("demo-thumb.png"),
            AttachmentKind::Photo,
        )
    {
        ComposerAttachment::push_attachment(&mut pending_attachments, att);
    }
    if matches!(demo, Some(ScreenshotDemo::ReadyAlbums)) {
        for (path, kind) in [
            (
                demo_media_allowlist().join("demo-thumb.png"),
                AttachmentKind::Photo,
            ),
            (
                demo_media_allowlist().join("demo-clip.mp4"),
                AttachmentKind::Video,
            ),
        ] {
            if let Some(att) = ComposerAttachment::pick(&path, kind) {
                ComposerAttachment::push_attachment(&mut pending_attachments, att);
            }
        }
    }
    pending_attachments
}

impl QuillApp {
    pub fn new_with_demo(
        window: &mut Window,
        cx: &mut Context<Self>,
        credentials: Option<TelegramCredentials>,
        demo: Option<ScreenshotDemo>,
    ) -> Self {
        let chat = cx.new(SyntheticChat::new);
        // Send-key mode drives kit's newline-vs-submit behavior: plain
        // Enter submits only in Enter mode; in CtrlEnter mode it inserts
        // a newline and Ctrl/Cmd+Enter sends.
        let chat_prefs = Self::load_chat_prefs();
        let submit_on_enter = chat_prefs.send_key_mode == quill::composer::SendKeyMode::Enter;
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Write a message...")
                .auto_grow(1, 8)
                .submit_on_enter(submit_on_enter)
        });
        // Phase C2h: in-call group-chat composer for the voice-chat
        // overlay (sendGroupCallMessage).
        let group_call_composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Message the voice chat — Enter sends")
                .auto_grow(1, 3)
                .submit_on_enter(submit_on_enter)
        });
        // Phase C2i: comment field for the call-rating detail card.
        let rating_comment_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("What went wrong? (optional)")
                .auto_grow(1, 3)
                .submit_on_enter(false)
        });
        let emoji_status_hours_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Custom duration in hours")
                .auto_grow(1, 1)
        });
        let emoji_set_search_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Search emoji packs")
                .auto_grow(1, 1)
        });
        let emoji_search_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Search emoji")
                .auto_grow(1, 1)
        });
        let reaction_search_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Search emoji")
                .auto_grow(1, 1)
        });
        cx.subscribe(
            &reaction_search_input,
            |_this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            },
        )
        .detach();
        cx.subscribe(&emoji_search_input, |_this, _, event: &InputEvent, cx| {
            // The panel's rows follow the query on the next render.
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        })
        .detach();
        let gif_search_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Search GIFs")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let sticker_search_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Search stickers and sets")
                .auto_grow(1, 1)
        });
        let registration_first_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("First name")
                .auto_grow(1, 1)
        });
        let registration_last_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Last name (optional)")
                .auto_grow(1, 1)
        });
        let email_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Email address")
                .auto_grow(1, 1)
                .submit_on_enter(true)
        });
        let phone_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Phone (+country code)")
                .auto_grow(1, 1)
                .submit_on_enter(true)
        });
        let code_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Verification code")
                .auto_grow(1, 1)
                .submit_on_enter(true)
        });
        let password_input = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder("Two-step password")
                .submit_on_enter(true)
        });
        let recovery_code_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Recovery code from email")
                .auto_grow(1, 1)
                .submit_on_enter(true)
        });
        // Slice A2: two-step verification overlay inputs. Passwords live
        // here only and are cleared on submit/close — never on the
        // session.
        let twofa_current_password = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder("Current password")
                .submit_on_enter(false)
        });
        let twofa_new_password = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder("New password")
                .submit_on_enter(false)
        });
        let twofa_hint = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Hint (optional)")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let twofa_code = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Code")
                .submit_on_enter(false)
        });
        let twofa_email = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Recovery email")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let search_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Search")
                .auto_grow(1, 1)
                .submit_on_enter(true)
        });
        let chat_search_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Search in chat")
                .auto_grow(1, 1)
                .submit_on_enter(true)
        });
        let forward_search_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Search chats")
                .auto_grow(1, 1)
                .submit_on_enter(true)
        });
        let share_comment_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Add a comment")
                .auto_grow(1, 3)
                .submit_on_enter(false)
        });
        let story_reply_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Reply to story")
                .auto_grow(1, 3)
                .submit_on_enter(true)
        });
        // Phase 9.5: report details draft — shown when the server answers
        // `reportStoryResultTextRequired`.
        let story_report_text_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Report details (optional)")
                .auto_grow(1, 3)
                .submit_on_enter(true)
        });
        // Phase 9.3: story composer inputs — media path (path entry; no
        // native file-picker infrastructure yet), caption, and the
        // selected-users search.
        let story_composer_path = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("/path/to/photo.jpg")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let story_composer_caption = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Caption… (**bold** markup supported)")
                .auto_grow(1, 3)
                .submit_on_enter(false)
        });
        let story_composer_user_search = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Search contacts")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        // Phase 9.4: story area inputs — link sticker URL and
        // suggested-reaction emoji (space-separated).
        let story_composer_link = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("https://… (optional, Premium)")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let story_composer_reaction = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("❤️ (optional, space-separated)")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        // Phase 9.5: cover-frame seconds input (viewer cover editor) and
        // the privacy editor's contact search.
        let story_cover_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Cover frame time in seconds, e.g. 1.5")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let story_privacy_user_search = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Search contacts")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        cx.subscribe_in(
            &composer,
            window,
            |this, state, event: &InputEvent, window, cx| {
                let text = state.read(cx).value().to_string();
                if matches!(event, InputEvent::Change) {
                    this.sync_composer_typing(&text);
                    this.note_open_draft(true, cx);
                }
                // Phase 3.3: the `/` menu tracks the composer text (Blur
                // dismisses it); Enter picks the highlighted command
                // instead of sending while the menu is open.
                match event {
                    InputEvent::Blur => {
                        this.close_command_menu(cx);
                        this.close_inline_results(cx);
                        this.close_mention_menu(cx);
                        this.close_suggest_menu(false, cx);
                    }
                    _ => {
                        this.sync_command_menu(cx);
                        this.sync_mention_menu(cx);
                        this.sync_suggest_menu(cx);
                        this.sync_inline_mode(cx);
                        // codex:spellcheck-native: shift underlines with
                        // the edit and debounce a background re-check.
                        this.sync_spellcheck(&text, cx);
                        this.sync_sticker_suggestions(&text, cx);
                        this.sync_animated_emoji_suggestion(&text, cx);
                    }
                }
                if let InputEvent::PressEnter { secondary, shift } = event {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if should_send_on_enter(
                        quill::composer::enter_event_from_kit(*shift, *secondary, marked),
                        this.chat_prefs.send_key_mode,
                    ) {
                        if this.pick_inline_result_selection(window, cx) {
                            // Enter was consumed by the inline results.
                        } else if this.pick_mention_selection(window, cx) {
                            // Enter completed the highlighted mention.
                        } else if this.pick_suggest_selection(window, cx) {
                            // Enter inserted the highlighted hashtag/emoji.
                        } else if this.pick_command_menu_selection(window, cx) {
                            // Enter was consumed by the open menu.
                        } else if !text.trim().is_empty()
                            || !this.pending_attachments.is_empty()
                            || this.forward_bar_here()
                        {
                            this.submit_composer(
                                quill::composer::send_text_on_enter(
                                    text,
                                    this.chat_prefs.send_key_mode,
                                ),
                                window,
                                cx,
                            );
                        }
                    }
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &group_call_composer,
            window,
            |this, state, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { secondary, shift } = event {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if should_send_on_enter(
                        quill::composer::enter_event_from_kit(*shift, *secondary, marked),
                        this.chat_prefs.send_key_mode,
                    ) {
                        this.send_group_call_message(window, cx);
                    }
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &email_input,
            window,
            |this, state, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { secondary, shift } = event {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if should_send_on_enter(
                        quill::composer::enter_event_from_kit(*shift, *secondary, marked),
                        quill::composer::SendKeyMode::Enter,
                    ) {
                        this.submit_email(window, cx);
                    }
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &phone_input,
            window,
            |this, state, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { secondary, shift } = event {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if should_send_on_enter(
                        quill::composer::enter_event_from_kit(*shift, *secondary, marked),
                        quill::composer::SendKeyMode::Enter, // not a chat composer — the send-key setting does not apply
                    ) {
                        this.submit_phone(window, cx);
                    }
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &code_input,
            window,
            |this, state, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { secondary, shift } = event {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if should_send_on_enter(
                        quill::composer::enter_event_from_kit(*shift, *secondary, marked),
                        quill::composer::SendKeyMode::Enter, // not a chat composer — the send-key setting does not apply
                    ) {
                        this.submit_code(window, cx);
                    }
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &password_input,
            window,
            |this, state, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { secondary, shift } = event {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if should_send_on_enter(
                        quill::composer::enter_event_from_kit(*shift, *secondary, marked),
                        quill::composer::SendKeyMode::Enter, // not a chat composer — the send-key setting does not apply
                    ) {
                        this.submit_password(window, cx);
                    }
                }
            },
        )
        .detach();
        auth_recovery::subscribe_recovery_input(&recovery_code_input, window, cx);
        cx.subscribe_in(
            &search_input,
            window,
            |this, state, event: &InputEvent, window, cx| {
                let text = state.read(cx).value().to_string();
                match event {
                    InputEvent::Change => this.sync_search_query(&text, cx),
                    InputEvent::Focus if !this.search_is_open() => this.open_search_ui(window, cx),
                    _ => {}
                }
                if let InputEvent::PressEnter { secondary, shift } = event {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if should_send_on_enter(
                        quill::composer::enter_event_from_kit(*shift, *secondary, marked),
                        quill::composer::SendKeyMode::Enter, // not a chat composer — the send-key setting does not apply
                    ) {
                        this.activate_first_search_result(window, cx);
                    }
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &chat_search_input,
            window,
            |this, state, event: &InputEvent, window, cx| {
                let text = state.read(cx).value().to_string();
                if matches!(event, InputEvent::Change) {
                    this.sync_chat_search_query(&text, cx);
                }
                if let InputEvent::PressEnter { secondary, shift } = event {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if should_send_on_enter(
                        quill::composer::enter_event_from_kit(*shift, *secondary, marked),
                        quill::composer::SendKeyMode::Enter, // not a chat composer — the send-key setting does not apply
                    ) {
                        this.jump_selected_chat_search_hit(cx);
                    }
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &forward_search_input,
            window,
            |this, state, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    let text = state.read(cx).value().to_string();
                    this.sync_share_search(&text, cx);
                }
                if let InputEvent::PressEnter { secondary, shift } = event {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if should_send_on_enter(
                        quill::composer::enter_event_from_kit(*shift, *secondary, marked),
                        quill::composer::SendKeyMode::Enter, // not a chat composer — the send-key setting does not apply
                    ) {
                        this.activate_first_forward_destination(cx);
                    }
                }
            },
        )
        .detach();

        let demo_sink = Arc::new(MemorySink::new());
        let mut demo_session = None;
        let (connect_status, live, status_note, auth_demo) = match demo {
            Some(d) => {
                let (seed, status, note, auth) = demo_seed_for(d);
                if let Some(seed) = seed {
                    demo_session = Some(seed(demo_sink.clone()));
                }
                (status, None, note, auth)
            }
            None => (
                ConnectUiStatus::Live,
                None,
                "Connecting to Telegram…".into(),
                AuthorizationState::WaitTdlibParameters,
            ),
        };

        let pending_attachments = demo_pending_attachments(demo);

        let audio_output = super::audio::SharedOutput::default();
        super::audio::share_output_with_video(&audio_output);
        let (spellchecker, spell_info) = Self::new_spellchecker(true);
        let mut app = Self {
            update_state: if demo.is_none() {
                quill::update_install::startup_state()
            } else {
                quill::updater::UpdateState::Idle
            },
            update_banner_dismissed: false,
            chat,
            composer,
            // kit Phase 7: in-window menu bar (menus installed by
            // `setup_app_menus` at startup).
            menu_bar: AppMenuBar::new(cx),
            // kit Phase 3: chat list + message history virtualization.
            chat_list_scroll: VirtualListScrollHandle::new(),
            chat_list_items: Vec::new(),
            chat_swipe: Default::default(),
            story_strip: Default::default(),
            history_scroller: cx.new(|cx| MessageScrollerState::new(0, cx)),
            history_rows: Vec::new(),
            rendered_history_rows: std::cell::RefCell::new(Vec::new()),
            reported_visible: None,
            history_window_active: false,
            history_shared: HistoryShared::default(),
            history_key: None,
            thread_root_jump: false,
            history_ends: None,
            history_window_epoch: 0,
            history_anchor_pending: false,
            history_had_newer: false,
            sidebar_width: px(quill::settings::load_window_state()
                .map_or(quill::settings::DEFAULT_SIDEBAR_WIDTH, |state| {
                    state.sidebar_width
                })),
            window_state_save_pending: false,
            history_rows_key: None,
            history_media_signature: (0, 0),
            last_highlight: None,
            highlight_fade: None,
            reaction_fly: None,
            scroll_date: Default::default(),
            scroll_probe: Default::default(),
            scroll_top_probe: Default::default(),
            scroll_view_probe: Default::default(),
            group_call_composer,
            command_menu_open: false,
            command_menu_selected: 0,
            mention_selected: 0,
            suggest: super::composer_suggest::SuggestUi::load(),
            inline_results_open: false,
            inline_results_selected: 0,
            inline_query_token: 0,
            inline_query_armed: None,
            sticker_search_input,
            media_panel: super::media_panel::MediaPanel::default(),
            emoji_search_input,
            reaction_search_input,
            emoji_set_search_input,
            emoji_status_hours_input,
            gif_search_input,
            marketplace_open: false,
            marketplace_name_input: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("Collectible gift name, e.g. PlushPepe-123")
            }),
            marketplace_comment_input: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("Personal comment")
                    .submit_on_enter(false)
            }),
            marketplace_private: true,
            marketplace_error: None,
            registration_first_input,
            registration_last_input,
            accepted_registration_terms: None,
            registration_notify_contacts: false,
            email_input,
            phone_input,
            code_input,
            password_input,
            recovery_code_input,
            recovery_mode: false,
            twofa_open: false,
            twofa_view: TwofaView::Status,
            twofa_current_password,
            twofa_new_password,
            twofa_hint,
            twofa_email,
            twofa_notice: None,
            twofa_code,
            twofa_confirm: None,
            account_lifecycle: AccountLifecycleState::new(window, cx),
            accounts_ui: AccountsUiState::new(window, cx),
            passcode_ui: super::passcode::PasscodeUi::new(window, cx),
            credentials,
            // Slice S3: privacy screen state.
            privacy_open: false,
            privacy_ui: super::privacy_extra::PrivacyUi::new(window, cx),
            privacy_editor: None,
            privacy_exceptions: None,
            exception_picker_open: false,
            block_picker_open: false,
            unblock_confirm: None,
            search_input,
            chat_search_input,
            forward_search_input,
            share_comment_input,
            story_reply_input,
            story_viewers_open: false,
            story_report_open: false,
            story_report_text_input,
            story_page: None,
            story_composer: StoryComposer::default(),
            story_composer_path,
            story_composer_caption,
            story_composer_user_search,
            story_composer_link,
            story_composer_reaction,
            story_cover_target: None,
            story_cover_input,
            story_cover_sent: false,
            story_privacy_edit: None,
            story_privacy_user_search,
            story_privacy_sent: false,
            auth_demo,
            focus_sidebar: cx.focus_handle(),
            context_menu_focus: cx.focus_handle(),
            context_menu_was_open: false,
            context_menu_previous_focus: None,
            connect_status,
            connection_generation: 0,
            connection_lost: false,
            live,
            status_note,
            status_seen: String::new(),
            status_shown_at: None,
            demo_auth_inputs: matches!(
                demo,
                Some(
                    ScreenshotDemo::WaitPhone
                        | ScreenshotDemo::WaitCode
                        | ScreenshotDemo::WaitPassword
                        | ScreenshotDemo::WaitPremium
                        | ScreenshotDemo::WaitQr
                        | ScreenshotDemo::ConnectionClosed
                )
            ),
            demo_session,
            demo_call_devices: None,
            demo_selected_devices: (None, None),
            demo_remote_frame: None,
            demo_local_frame: None,
            demo_screen_frame: None,
            demo_selected_camera: None,
            call_remote_image: None,
            call_local_image: None,
            qr_login_cache: None,
            group_video_images: HashMap::new(),
            demo_group_frames: HashMap::new(),
            demo_seq: AtomicU64::new(0),
            demo_sink,
            notify_clicks: Arc::new(Mutex::new(Vec::new())),
            notify_inflight: Arc::new(AtomicUsize::new(0)),
            pending_attachments,
            composer_self_destruct: None,
            composer_caption_above: false,
            composer_silent: false,
            composer_preview_disabled: false,
            composer_preview_above: false,
            composer_preview_media: PreviewMediaSize::Auto,
            composer_preview_token: 0,
            composer_scheduling: ComposerScheduling::None,
            schedule_popup_open: false,
            schedule_picker: None,
            scheduled_dialog_open: false,
            rich_editor_open: false,
            message_menu: None,
            chat_menu: None,
            archive_menu: None,
            pin_reorder: None,
            pin_reorder_archived: false,
            pin_drag_anchor: None,
            chat_preview: None,
            profile_modal: None,
            preview_press: None,
            selected_chats: HashSet::new(),
            swipe_reply_start: None,
            pending_reply: None,
            clear_draft_on_success: None,
            pending_edit: None,
            saved_edit_draft: String::new(),
            saved_edit_reply: None,
            pending_delete: None,
            pending_stop_poll: None,
            pending_close_secret_chat: None,
            pending_inline_bot_alert: None,
            inline_bot_alert_shown: false,
            forum_manage_dialog: None,
            saved_tag_dialog: None,
            poll_voters_dialog: None,
            poll_add_option: None,
            checklist_dialog: None,
            welcome_dialog: None,
            event_log_search: None,
            event_log_admin_filter: None,
            storage_usage_open: false,
            appearance: Self::load_appearance(),
            chat_prefs,
            translate_ui: super::translate_ui::TranslateUi::load(),
            appearance_open: false,
            settings_open: false,
            settings_page: None,
            keybinding_capture: None,
            keybinding_error: None,
            keybinding_focus: cx.focus_handle(),
            keybindings_applied: false,
            keybindings_screenshot: false,
            appearance_applied: None,
            // codex:spellcheck-native: platform engine + persisted app words.
            spellchecker,
            spell_info,
            spell_misspellings: Vec::new(),
            spell_checked_text: String::new(),
            spell_task: None,
            shortcuts_open: false,
            proxy_ui: Default::default(),
            sticker_settings_open: false,
            data_storage_editor: None,
            storage_confirm: None,
            storage_selected: Default::default(),
            sessions_open: false,
            device_qr_scanner: None,
            device_login_qr: None,
            device_link_notice: None,
            sessions_confirm: None,
            websites_open: false,
            websites_confirm: None,
            chat_filter: ChatListFilter::All,
            new_secret_picker_open: false,
            pending_forward: None,
            selection_anchor: None,
            selection_drag: None,
            forward_picker_open: false,
            share_selection: quill::share_box::ShareSelection::default(),
            forward_bar_dest: None,
            send_as_open: false,
            forward_result: None,
            reactions_expanded: false,
            mute_menu_open: false,
            mute_custom_open: false,
            mute_custom: quill::mute_menu::CustomMute::default(),
            ttl_picker_open: false,
            ttl_custom_open: false,
            ttl_custom_secs: 86_400,
            pinned_cursor: HashMap::new(),
            hidden_pinned: HashMap::new(),
            pinned_list_open: false,
            inline_videos: Default::default(),
            animation_demand: Default::default(),
            row_fx: Default::default(),
            animation_targets: Default::default(),
            animation_sound: Default::default(),
            polled_redraw: super::notifications::PolledRedraw::new(std::time::Instant::now()),
            window_active: std::cell::Cell::new(true),
            presence: Default::default(),
            login_prevented: None,
            terms_step: Default::default(),
            terms_age_ok: false,
            terms_age_error: false,
            media_roots_frame: Default::default(),
            frame_clock_running: Default::default(),
            motion: Default::default(),
            composer_link_dialog: None,
            send_morph: Default::default(),
            slices: Default::default(),
            stream_reveal: Default::default(),
            vanishing: Default::default(),
            group_call_title_dialog: None,
            group_call_start_dialog: None,
            group_call_invite_open: false,
            notif_sound_picker_open: false,
            story_sound_picker_open: false,
            notification_defaults_open: false,
            defaults_sound_picker: None,
            defaults_exceptions_scope: None,
            notifications_confirm: None,
            voice_capture: None,
            video_note_capture: None,
            record_locked: false,
            record_discard_confirm: false,
            voice_tick: false,
            recording_auto_send: false,
            round_preview: Default::default(),
            slow_mode_tick_chat: None,
            self_destruct_tick_chat: None,
            call_tick_active: false,
            call_window: None,
            group_call_window: None,
            group_call_window_opening: false,
            group_call_window_closed_by_user: None,
            group_call_chat_shown: false,
            call_window_opening: false,
            call_window_raised: false,
            call_window_closed_by_user: None,
            call_ended_at: None,
            call_sounds: super::call_sounds::CallSounds::new(audio_output.clone()),
            call_sound_marks: Default::default(),
            player: Default::default(),
            playing_voice: None,
            playing_audio: None,
            pending_audio_play: None,
            pending_voice_play: None,
            audio: super::audio::AudioEngine::new(audio_output.clone()),
            notification_sounds: super::audio::NotificationSounds::new(audio_output.clone()),
            playback_clock: None,
            playback_path: None,
            seek_slider: None,
            seek_scrubbing: false,
            seek_preview_secs: None,
            playback_tick: false,
            playback_positions: HashMap::new(),
            sticker_playback: Default::default(),
            emoji_playback: Default::default(),
            playing_animation: None,
            animation_frames: Vec::new(),
            autoplayed_gifs: Default::default(),
            animation_frame: 0,
            animation_tick: false,
            animation_fps: 8.0,
            animation_started_at: None,
            animation_extract_child: None,
            animation_extract_cancel: None,
            animation_extract_epoch: 0,
            animation_cache_file: None,
            pending_gif_play: None,
            playing_video: None,
            video_frames: Vec::new(),
            video_frame: 0,
            video_tick: false,
            video_cache_file: None,
            pending_video_play: None,
            sponsored_about_open: false,
            rendered_sponsored: std::cell::RefCell::new(Vec::new()),
            spoiler_revealed: HashSet::new(),
            poll_dialog: None,
            payment_dialog: None,
            invite_link_dialog: None,
            admin_dialog: None,
            create_chat_dialog: None,
            member_dialog: None,
            callback_password_dialog: None,
            login_url_confirm: None,
            pending_deep_link: None,
            deep_link_dialog: None,
            deep_link_invite: None,
            pending_deep_link_ui: None,
            share_link_text: None,
            pending_media_seek: None,
            pending_deep_link_open: None,
            pending_link: None,
            right_clicked_link: None,
            message_menu_link: None,
            link_tooltip: None,
            open_link_confirm: None,
            link_popup: None,
            pending_viewer_seek: None,
            dismissed_keyboards: std::collections::HashSet::new(),
            collapsed_keyboards: std::collections::HashSet::new(),
            request_share: None,
            permissions_dialog: None,
            username_dialog: None,
            community_ui: CommunityUi::default(),
            restrict_dialog: None,
            ownership_dialog: None,
            group_confirm_dialog: None,
            message_menu_selection: None,
            message_menu_ui: super::message_menu_ui::MessageMenuUi::new(window, cx),
            media_viewer: MediaViewer::closed(),
            photo_editor: None,
            viewer_zoom: ViewerZoom::new(),
            viewer_frame: (720.0, 480.0),
            viewer_drag: None,
            viewer_video: None,
            pip_window: None,
            viewer_video_path: None,
            viewer_audio: super::audio::AudioEngine::new(audio_output.clone()),
            viewer_clock: None,
            capture_blocked: false,
            capture_notice_dismissed: false,
            viewer_tick: false,
            viewer_pending_play: None,
            viewer_orientation: Default::default(),
            viewer_rotated: None,
            viewer_open_gen: 0,
            viewer_last_activity: std::time::Instant::now(),
            viewer_controls_hidden: false,
            viewer_controls_gen: 0,
            viewer_over_controls: false,
            viewer_hide_timer: false,
            viewer_extra: Default::default(),
            viewer_seek_slider: None,
            viewer_seek_scrubbing: false,
            viewer_seek_preview_secs: None,
            viewer_volume_slider: None,
            viewer_volume_scrubbing: false,
            playback_speed: 1.0,
            playback_volume: 1.0,
            playback_unmuted_volume: 1.0,
            playback_error: None,
            composer_group_media: None,
            viewer_video_frames: Vec::new(),
            viewer_native: None,
            status_traced: String::new(),
            viewer_video_fps: 0.0,
            viewer_frame_cache_file: None,
            viewer_extracting: false,
            viewer_extract_child: None,
            viewer_extract_cancel: None,
            viewer_extract_epoch: 0,
            viewer_demo_sync_frames: false,
            story_viewer: StoryViewer::closed(),
            story_playback: StoryPlayback::default(),
            story_tick_active: false,
            pending_story_open: None,
            story_reaction_picker_open: false,
            story_reply_open: false,
            contacts_tab_open: false,
            calls_tab_open: false,
            call_confirm: None,
            rating_detail: None,
            rating_comment_input,
            folder_tab: None,
            folder_manage_open: false,
            folder_editor: None,
            folder_delete_confirm: None,
            folder_menu_open: false,
            add_contact_dialog: None,
            block_bar_dialog: None,
            join_requests_dialog: None,
            edit_profile_dialog: None,
            profile_dialog: None,
            group_settings_dialog: None,
            pending_profile_gallery: None,
            import_contacts_dialog: None,
        };

        if demo == Some(ScreenshotDemo::ReadyTrayBehavior) {
            app.appearance.minimize_to_tray = true;
        }
        if matches!(
            demo,
            Some(
                ScreenshotDemo::ReadyUpdateInstall
                    | ScreenshotDemo::ReadyUpdateChangelog
                    | ScreenshotDemo::ReadyUpdateFailure
            )
        ) {
            let release = quill::updater::ReleaseInfo {
                version: "0.2.0".into(),
                notes: "Update complete: improved navigation, video playback and accessibility."
                    .into(),
                url: format!("{}/tag/v0.2.0", quill::updater::RELEASES_URL),
                asset: Some(quill::updater::ReleaseAsset {
                    url: format!(
                        "{}/download/v0.2.0/{}",
                        quill::updater::RELEASES_URL,
                        quill::updater::binary_asset_name()
                    ),
                    size: 120,
                    sha256: "a".repeat(64),
                }),
            };
            app.update_state = match demo {
                Some(ScreenshotDemo::ReadyUpdateChangelog) => {
                    quill::updater::UpdateState::Installed(release)
                }
                Some(ScreenshotDemo::ReadyUpdateFailure) => {
                    quill::updater::UpdateState::DownloadFailed(
                        release,
                        "Download interrupted. Retry the update.",
                    )
                }
                _ => quill::updater::UpdateState::Available(release),
            };
            app.appearance_open = true;
        }
        app.demo_setup_composer(demo, window, cx);
        app.demo_setup_messages(demo, window, cx);
        app.demo_setup_chat_list(demo, window, cx);
        app.demo_setup_media(demo, window, cx);
        app.demo_setup_groups(demo, window, cx);
        app.demo_setup_share(demo, window, cx);
        app.demo_setup_security(demo, window, cx);
        app.demo_setup_calls(demo, window, cx);
        app.demo_setup_privacy_media(demo, window, cx);
        app.demo_setup_payments(demo, window, cx);
        app.demo_setup_viewer_extras(demo, cx);
        app.demo_setup_stories(demo, window, cx);
        app.demo_setup_groups_admin(demo, window, cx);
        app.demo_setup_bots_profile(demo, window, cx);
        app.demo_setup_proxy(demo, window, cx);
        app.demo_setup_profile_panels(demo, window, cx);
        app.demo_setup_member_moderation(demo, window, cx);
        app.demo_setup_group_admin_settings(demo, cx);
        if matches!(demo, Some(ScreenshotDemo::ReadyMessageMenu)) {
            app.demo_setup_message_menu(window, cx);
        }

        let menu_app = cx.weak_entity();
        cx.intercept_keystrokes(move |event, window, cx| {
            // Shortcut capture owns the key. This interceptor is registered
            // first so it observes `keybinding_capture` before the capture
            // handler clears it.
            let capturing = menu_app
                .update(cx, |this, _| this.keybinding_capture_active())
                .unwrap_or(false);
            if !capturing {
                let menu_handled = menu_app
                    .update(cx, |this, cx| {
                        if this.message_menu.is_none()
                            && this.chat_menu.is_none()
                            && this.archive_menu.is_none()
                        {
                            return false;
                        }
                        if event.keystroke.key == "escape" {
                            this.message_menu = None;
                            this.chat_menu = None;
                            this.archive_menu = None;
                            cx.notify();
                            return true;
                        }
                        super::keybindings::context_menu_captures_key(&event.keystroke)
                    })
                    .unwrap_or(false);
                if menu_handled {
                    cx.stop_propagation();
                    return;
                }
            }
            // Viewer playback keys (Space/K/J/L/Enter, tdesktop
            // `handleKeyPress`). A dialog over the viewer (delete
            // confirmation) keeps its own Enter and Space.
            if !capturing && !window.has_active_dialog(cx) {
                let viewer_handled = menu_app
                    .update(cx, |this, cx| {
                        this.handle_viewer_key(&event.keystroke, window, cx)
                    })
                    .unwrap_or(false);
                if viewer_handled {
                    cx.stop_propagation();
                    return;
                }
            }
            if capturing || event.keystroke.modifiers.modified() {
                return;
            }
            let handled = match event.keystroke.key.as_str() {
                "escape" => menu_app
                    .update(cx, |this, cx| {
                        this.close_context_menus(cx)
                            || this.close_media_viewer_on_escape(cx)
                            || this.close_media_panel(cx)
                            || this.close_inline_results(cx)
                            || this.close_mention_menu(cx)
                            || this.close_suggest_menu(true, cx)
                            || this.close_command_menu(cx)
                    })
                    .unwrap_or(false),
                "up" => menu_app
                    .update(cx, |this, cx| {
                        this.step_inline_results(-1, cx)
                            || this.step_mention_menu(-1, cx)
                            || this.step_suggest_menu(-1, false, cx)
                            || this.step_command_menu(-1, cx)
                    })
                    .unwrap_or(false),
                "down" => menu_app
                    .update(cx, |this, cx| {
                        this.step_inline_results(1, cx)
                            || this.step_mention_menu(1, cx)
                            || this.step_suggest_menu(1, false, cx)
                            || this.step_command_menu(1, cx)
                    })
                    .unwrap_or(false),
                // Tab completes the highlighted `@` / `#` / `:` suggestion.
                "tab" => menu_app
                    .update(cx, |this, cx| {
                        this.pick_mention_selection(window, cx)
                            || this.pick_suggest_selection(window, cx)
                    })
                    .unwrap_or(false),
                // The emoji strip is horizontal (tdesktop steps with
                // Left/Right too); the key keeps moving the caret otherwise.
                "left" => menu_app
                    .update(cx, |this, cx| this.step_suggest_menu(-1, true, cx))
                    .unwrap_or(false),
                "right" => menu_app
                    .update(cx, |this, cx| this.step_suggest_menu(1, true, cx))
                    .unwrap_or(false),
                _ => false,
            };
            if handled {
                cx.stop_propagation();
            }
        })
        .detach();
        // GPUI matches keybindings before `on_key_down`. While a shortcut
        // row is capturing, consume the key here so Escape cannot dismiss
        // Appearance and quit/close/other chords cannot fire underneath.
        // The element `on_key_down` also stops propagation for keys that
        // reach the bubble phase.
        let capture_app = cx.weak_entity();
        cx.intercept_keystrokes(move |event, _window, cx| {
            let keystroke = event.keystroke.clone();
            let armed = capture_app
                .update(cx, |this, _| this.keybinding_capture_active())
                .unwrap_or(false);
            if !armed {
                return;
            }
            cx.stop_propagation();
            capture_app
                .update(cx, |this, cx| {
                    this.handle_keybinding_capture(&keystroke, cx);
                })
                .ok();
        })
        .detach();
        // Up in an empty, focused composer edits the last own message
        // (Telegram Desktop). The textarea binds Up to a cursor move, and
        // bindings match before `on_key_down`, so intercept the keystroke.
        let edit_app = cx.weak_entity();
        cx.intercept_keystrokes(move |event, window, cx| {
            let keystroke = &event.keystroke;
            if keystroke.key != "up" || keystroke.modifiers.modified() {
                return;
            }
            let handled = edit_app
                .update(cx, |this, cx| this.try_edit_last_message(window, cx))
                .unwrap_or(false);
            if handled {
                cx.stop_propagation();
            }
        })
        .detach();
        let notification_app = cx.weak_entity();
        cx.on_system_notification_response(move |response, cx| {
            let Some((account, chat_id)) = quill::notify::parse_notification_tag(&response.tag)
            else {
                return;
            };
            let _ = notification_app.update(cx, |this, cx| {
                if this
                    .session()
                    .is_none_or(|s| account != format!("account:{}", s.account.0))
                {
                    return;
                }
                let action = quill::notify::NotificationAction::from_id(
                    response.action_id.as_ref().map(|id| id.as_ref()),
                );
                if let Ok(mut clicks) = this.notify_clicks.lock() {
                    clicks.push((chat_id, action));
                }
                cx.notify();
            });
            // A hidden (close-to-tray) or minimized window may never render
            // again on its own: bring the app forward from the click itself
            // so `flush_notifications` runs and opens the chat.
            cx.activate(true);
            for window in cx.windows() {
                let _ = window.update(cx, |_, window, _| window.activate_window());
            }
        });
        if demo.is_none() && !app.passcode_ui.deferred_connect {
            // With a local passcode the database key is wrapped: the
            // connection starts after the first unlock (tdesktop starts locked).
            app.start_connection(cx);
        }
        if demo.is_none() {
            app.spawn_passcode_tick(cx);
        }
        // Settings → Appearance: apply the persisted prefs (theme +
        // accent) before the first frame, then re-evaluate auto-night
        // (scheduled/system) once a minute. `apply_appearance` only
        // notifies when the effective theme actually changed, so the
        // tick is free when idle.
        app.apply_appearance(cx);
        app.init_slices(cx);
        // Animations stop behind another app and resume on activation:
        // update the gate now and redraw (the content asks for ticks again).
        cx.observe_window_activation(window, |this, window, cx| {
            let active = window.is_window_active() || super::frame_clock::assume_active();
            this.window_active.set(active);
            this.inline_videos.borrow_mut().set_window_active(active);
            cx.notify();
        })
        .detach();
        // Remember the window's geometry when the user moves or resizes it.
        cx.observe_window_bounds(window, |this, window, cx| {
            this.schedule_window_state_save(window, cx);
        })
        .detach();
        // Performance fixture: a steady stream of synthetic TDLib updates
        // (`demo_stream`), to measure what an idle signed-in window costs.
        if demo.is_some()
            && let Some(rate) = super::demo_stream::update_stream_rate()
        {
            app.spawn_demo_update_stream(rate, cx);
        }
        // Performance fixture: keep rendering at ~60 Hz so a profiler sees
        // steady-state frames (`QUILL_DEMO_STRESS_REDRAW=0`: only what the
        // app itself asks for, to measure idle animation cost).
        if demo.is_some()
            && super::demo::demo_stress_size().is_some()
            && std::env::var_os("QUILL_DEMO_STRESS_REDRAW").is_none_or(|v| v != "0")
        {
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(16))
                        .await;
                    if this.update(cx, |_, cx| cx.notify()).is_err() {
                        break;
                    }
                }
            })
            .detach();
        }
        if demo.is_none()
            && app.appearance.check_updates_on_launch
            && app.update_state == quill::updater::UpdateState::Idle
        {
            app.check_for_updates(cx);
        }
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(60))
                    .await;
                let alive = this
                    .update(cx, |this, cx| this.apply_appearance(cx))
                    .is_ok();
                if !alive {
                    break;
                }
            }
        })
        .detach();
        // The scroller notifies on every scroll: drives the floating date.
        cx.observe(&app.history_scroller, |this, _, cx| {
            this.note_history_scroll(cx);
        })
        .detach();
        app
    }

    /// Phase C2l: synthetic peer screen-share frame for the screenshot
    /// fixture — a 16:9 "desktop" test pattern (dark gradient, window
    /// rectangles, a taskbar strip) so the receive tile is visually
    /// distinct from the camera patterns. NOT a real share: screenshot
    /// demos only.
    pub(super) fn demo_screen_frame() -> quill::calls::engine::VideoFrame {
        const W: usize = 480;
        const H: usize = 270;
        let mut rgba = Vec::with_capacity(W * H * 4);
        for y in 0..H {
            for x in 0..W {
                let fx = x as f32 / (W - 1) as f32;
                let fy = y as f32 / (H - 1) as f32;
                // Dark blue-gray desktop gradient.
                let (mut r, mut g, mut b) = (
                    (24.0 + 20.0 * fx) as u8,
                    (32.0 + 28.0 * fy) as u8,
                    (52.0 + 30.0 * fx) as u8,
                );
                // Two window rectangles.
                let in_win = |x0: usize, y0: usize, x1: usize, y1: usize| {
                    x >= x0 && x < x1 && y >= y0 && y < y1
                };
                if in_win(40, 30, 220, 170) || in_win(250, 50, 440, 200) {
                    (r, g, b) = (200, 208, 220);
                }
                // Window title bars.
                if in_win(40, 30, 220, 48) || in_win(250, 50, 440, 68) {
                    (r, g, b) = (70, 110, 180);
                }
                // Taskbar strip.
                if y >= H - 24 {
                    (r, g, b) = (18, 20, 26);
                }
                rgba.extend_from_slice(&[r, g, b, 255]);
            }
        }
        quill::calls::engine::VideoFrame {
            seq: 0,
            width: W as u16,
            height: H as u16,
            rgba,
            is_local: false,
            participant_user_id: None,
            is_screen: true,
        }
    }
}
