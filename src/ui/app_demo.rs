//! new_with_demo constructor (slimmed), demo_seed_for seed table, demo_pending_attachments.

use super::app::{ChatListFilter, QuillApp};
use super::connect_ui::{ConnectUiStatus, bootstrap_connect};
use super::demo::{
    demo_media_allowlist, seed_ready_chats_session, seed_ready_downloads_session,
    seed_ready_media_session, seed_ready_offline_session, seed_ready_reconnecting_session,
    seed_ready_send_media_session, seed_ready_unread_read_session, seed_ready_unread_session,
};
use super::history::HistoryShared;
use super::screenshot_demo::ScreenshotDemo;
use super::synthetic::SyntheticChat;
use super::*;
use gpui_kit::component::input::{InputEvent, TextareaState};
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
        ScreenshotDemo::ReadyDeepLinkInfo | ScreenshotDemo::ReadyDeepLinkInvite | ScreenshotDemo::ReadyChats | ScreenshotDemo::ReadyChatsComposer | ScreenshotDemo::ReadyAppearance | ScreenshotDemo::ReadySpellcheck | ScreenshotDemo::ReadySpellcheckPanel | ScreenshotDemo::ReadySpellcheckToggle | ScreenshotDemo::ReadyAccounts => (
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
        ScreenshotDemo::ReadyTyping => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — peer typing (injected updateChatAction)".into(),
            AuthorizationState::Ready,
        ),
        ScreenshotDemo::ReadyStickers => (
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
        ScreenshotDemo::ReadyGifs => (
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
        ScreenshotDemo::ReadySponsored => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — sponsored / recommended channel rows".into(),
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
        ScreenshotDemo::ReadyProfileEdit | ScreenshotDemo::ReadyUsername => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — edit profile dialog (injected, no live Telegram)".into(),
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
        ScreenshotDemo::ReadyTextEntities => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — text entities in text + caption".into(),
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
        ScreenshotDemo::ReadyVideoPlayback => (
            Some(seed_ready_media_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — in-viewer video playback".into(),
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
        ScreenshotDemo::ReadyTopicPost => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — posting to a forum topic".into(),
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
        ScreenshotDemo::ReadyPrivacy => (
            Some(seed_ready_chats_session as fn(Arc<MemorySink>) -> Session),
            ConnectUiStatus::DemoReadyChats,
            "screenshot demo — privacy settings (injected, no live Telegram)".into(),
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
                .placeholder("Message — Enter sends, Shift+Enter newline. IME Enter must not send.")
                .auto_grow(2, 6)
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
            TextareaState::new(window, cx)
                .placeholder("Two-step password")
                .auto_grow(1, 1)
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
            TextareaState::new(window, cx)
                .placeholder("Current password")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let twofa_new_password = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("New password")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let twofa_hint = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Hint (optional)")
                .auto_grow(1, 1)
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
                this.sync_composer_typing(&text);
                this.note_open_draft(true, cx);
                // Phase 3.3: the `/` menu tracks the composer text (Blur
                // dismisses it); Enter picks the highlighted command
                // instead of sending while the menu is open.
                match event {
                    InputEvent::Blur => {
                        this.close_command_menu(cx);
                        this.close_inline_results(cx);
                    }
                    _ => {
                        this.sync_command_menu(cx);
                        this.sync_inline_mode(cx);
                        // parity:platform-spellcheck: cheap re-check of
                        // the draft (suggestions stay cached until the
                        // word set changes).
                        this.sync_spellcheck(&text, cx);
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
                        } else if this.pick_command_menu_selection(window, cx) {
                            // Enter was consumed by the open menu.
                        } else if !text.trim().is_empty() {
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
                this.sync_search_query(&text, cx);
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
                this.sync_chat_search_query(&text, cx);
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
            None => bootstrap_connect(credentials.clone()),
        };

        let pending_attachments = demo_pending_attachments(demo);

        let mut app = Self {
            chat,
            composer,
            // kit Phase 7: in-window menu bar (menus installed by
            // `setup_app_menus` at startup).
            menu_bar: AppMenuBar::new(cx),
            // kit Phase 3: chat list + message history virtualization.
            chat_list_scroll: VirtualListScrollHandle::new(),
            chat_list_items: Vec::new(),
            history_scroller: cx.new(|cx| MessageScrollerState::new(0, cx)),
            history_rows: Vec::new(),
            history_shared: HistoryShared::default(),
            history_key: None,
            history_ends: None,
            last_highlight: None,
            group_call_composer,
            command_menu_open: false,
            command_menu_selected: 0,
            inline_results_open: false,
            inline_results_selected: 0,
            inline_query_token: 0,
            inline_query_armed: None,
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
            account_lifecycle: AccountLifecycleState::new(window, cx),
            accounts_ui: AccountsUiState::new(window, cx),
            credentials,
            // Slice S3: privacy screen state.
            privacy_open: false,
            privacy_editor: None,
            privacy_exceptions: None,
            exception_picker_open: false,
            block_picker_open: false,
            unblock_confirm: None,
            search_input,
            chat_search_input,
            forward_search_input,
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
            connect_status,
            live,
            status_note,
            demo_auth_inputs: matches!(
                demo,
                Some(
                    ScreenshotDemo::WaitPhone
                        | ScreenshotDemo::WaitCode
                        | ScreenshotDemo::WaitPassword
                        | ScreenshotDemo::WaitQr
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
            attach_menu_open: false,
            scheduled_dialog_open: false,
            rich_editor_open: false,
            message_menu: None,
            chat_menu: None,
            chat_preview: None,
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
            comment_thread_dialog: None,
            poll_voters_dialog: None,
            welcome_dialog: None,
            event_log_search: None,
            event_log_admin_filter: None,
            storage_usage_open: false,
            appearance: Self::load_appearance(),
            chat_prefs,
            appearance_open: false,
            appearance_applied: None,
            // parity:platform-spellcheck: engine + persisted user words.
            spellchecker: {
                let mut sc = quill::spellcheck::SpellChecker::new();
                sc.set_custom_words(
                    quill::settings::load_spellcheck_words(&Self::appearance_paths()).words,
                );
                sc
            },
            spell_misspellings: Vec::new(),
            spell_suggestions: Vec::new(),
            spellcheck_open: false,
            data_storage_editor: None,
            data_storage_confirm_clear: false,
            sessions_open: false,
            sessions_confirm: None,
            websites_open: false,
            websites_confirm: None,
            chat_filter: ChatListFilter::All,
            new_secret_picker_open: false,
            pending_forward: None,
            forward_picker_open: false,
            forward_result: None,
            pending_react: None,
            mute_menu_open: false,
            ttl_picker_open: false,
            group_call_title_dialog: None,
            group_call_start_dialog: None,
            group_call_invite_open: false,
            notif_sound_picker_open: false,
            notification_defaults_open: false,
            defaults_sound_picker: None,
            defaults_exceptions_scope: None,
            notifications_confirm: None,
            notify_sound_inflight: Arc::new(AtomicUsize::new(0)),
            voice_capture: None,
            video_note_capture: None,
            record_locked: false,
            record_discard_confirm: false,
            voice_tick: false,
            slow_mode_tick_chat: None,
            self_destruct_tick_chat: None,
            call_tick_active: false,
            playing_voice: None,
            playing_audio: None,
            pending_audio_play: None,
            voice_player: None,
            playback_clock: None,
            playback_path: None,
            seek_slider: None,
            seek_scrubbing: false,
            seek_preview_secs: None,
            playback_tick: false,
            playback_positions: HashMap::new(),
            playing_animation: None,
            animation_frames: Vec::new(),
            animation_frame: 0,
            animation_tick: false,
            animation_cache_file: None,
            pending_gif_play: None,
            playing_video: None,
            video_frames: Vec::new(),
            video_frame: 0,
            video_tick: false,
            video_cache_file: None,
            pending_video_play: None,
            sponsored_demo: false,
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
            pending_deep_link_open: None,
            dismissed_keyboards: std::collections::HashSet::new(),
            permissions_dialog: None,
            username_dialog: None,
            community_ui: CommunityUi::default(),
            restrict_dialog: None,
            group_confirm_dialog: None,
            quote_reply_dialog: None,
            media_viewer: MediaViewer::closed(),
            viewer_zoom: ViewerZoom::new(),
            viewer_drag: None,
            viewer_video: None,
            viewer_video_path: None,
            viewer_player: None,
            viewer_clock: None,
            viewer_tick: false,
            viewer_pending_play: None,
            viewer_rotation: 0,
            viewer_rotated: None,
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
            edit_profile_dialog: None,
            import_contacts_dialog: None,
        };

        app.demo_setup_composer(demo, window, cx);
        app.demo_setup_messages(demo, window, cx);
        app.demo_setup_chat_list(demo, window, cx);
        app.demo_setup_media(demo, window, cx);
        app.demo_setup_groups(demo, window, cx);
        app.demo_setup_security(demo, window, cx);
        app.demo_setup_calls(demo, window, cx);
        app.demo_setup_privacy_media(demo, window, cx);
        app.demo_setup_payments(demo, window, cx);
        app.demo_setup_stories(demo, window, cx);
        app.demo_setup_groups_admin(demo, window, cx);
        app.demo_setup_bots_profile(demo, window, cx);

        let menu_app = cx.weak_entity();
        cx.intercept_keystrokes(move |event, _window, cx| {
            if event.keystroke.modifiers.modified() {
                return;
            }
            let handled = match event.keystroke.key.as_str() {
                "escape" => menu_app
                    .update(cx, |this, cx| {
                        this.close_inline_results(cx) || this.close_command_menu(cx)
                    })
                    .unwrap_or(false),
                "up" => menu_app
                    .update(cx, |this, cx| {
                        this.step_inline_results(-1, cx) || this.step_command_menu(-1, cx)
                    })
                    .unwrap_or(false),
                "down" => menu_app
                    .update(cx, |this, cx| {
                        this.step_inline_results(1, cx) || this.step_command_menu(1, cx)
                    })
                    .unwrap_or(false),
                _ => false,
            };
            if handled {
                cx.stop_propagation();
            }
        })
        .detach();
        if app.live.is_some() {
            app.spawn_poll_loop(cx);
        }
        // Settings → Appearance: apply the persisted prefs (theme +
        // accent) before the first frame, then re-evaluate auto-night
        // (scheduled/system) once a minute. `apply_appearance` only
        // notifies when the effective theme actually changed, so the
        // tick is free when idle.
        app.apply_appearance(cx);
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
