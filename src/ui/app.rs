//! QuillApp struct definition and core app queries.

use super::connect_ui::ConnectUiStatus;
use super::demo::demo_media_allowlist;
use super::synthetic::SyntheticChat;
use gpui_kit::component::input::TextareaState;
use gpui_kit::component::menu::AppMenuBar;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::connect::LiveConnect;
use quill::credentials::TelegramCredentials;
use quill::settings::{AppearancePrefs, ChatPrefs};
use quill::state::Session;
use quill::telegram::envelope::AuthorizationState;
use std::path::PathBuf;
pub struct QuillApp {
    /// Settings boxes: appearance, shortcuts, storage, proxy, translation and app updates (`ui/settings_state.rs`).
    pub(super) settings: super::settings_state::SettingsUi,
    pub(super) chat: Entity<SyntheticChat>,
    pub(super) composer: Entity<TextareaState>,
    /// kit Phase 7: the in-window menu bar (Linux/Windows; macOS uses the
    /// native menu bar via `cx.set_menus`). Entity-owned so it survives
    /// re-renders.
    pub(super) menu_bar: Entity<AppMenuBar>,
    /// Chat list: rows, filter, menus, pinning, preview and side tabs (`ui/chat_list_state.rs`).
    pub(super) chat_list: super::chat_list_state::ChatListUi,
    /// Scroll positions of the shared-media grid tabs (`ui/shared_media_grid_state.rs`).
    pub(super) shared_media_grid: super::shared_media_grid_state::SharedMediaGridUi,
    /// Stories UI state: the strip, viewer, composer, story page and their boxes.
    pub(super) stories: super::stories_state::StoryUi,
    /// The Premium feature preview's clip player (`ui/premium_preview_state.rs`).
    pub(super) premium_preview: super::premium_preview_state::PremiumPreviewUi,
    /// The first-run onboarding before sign-in (`ui/first_run_state.rs`).
    pub(super) first_run: super::first_run_state::FirstRun,
    /// Message history: rows, window, scroll probes, highlights and pinned bar (`ui/history_state.rs`).
    pub(super) history: super::history_state::HistoryUi,
    /// Window and per-frame bookkeeping: animation demand, focus restore, quit guard (`ui/frame_state.rs`).
    pub(super) frame: super::frame_state::FrameUi,
    /// Composer state: menus, attachments, send options, reply and edit drafts (`ui/composer_state.rs`).
    pub(super) composer_ui: super::composer_state::ComposerUi,
    /// The next send's attachments and options, per account, chat and topic (`ui/composer_sends.rs`).
    pub(super) composer_sends: super::composer_sends::ComposerSends,
    /// Emoji, sticker and GIF panel and their search fields (`ui/pickers_state.rs`).
    pub(super) pickers: super::pickers_state::PickerUi,
    /// The sticker set box's editing flows (`ui/sticker_set_box/state.rs`).
    pub(super) sticker_box: super::sticker_set_box::state::StickerSetBoxUi,
    /// Payments and the gift marketplace (`ui/payments_state.rs`).
    pub(super) payments: super::payments_state::PaymentUi,
    /// Revenue pages, withdrawals and affiliate programs (`ui/earn_state.rs`).
    pub(super) earn: super::earn_state::EarnUi,
    /// Settings → Language: the open box and recent packs (`ui/language_state.rs`).
    pub(super) language: super::language_state::LanguageUi,
    /// The search field above a profile's groups in common.
    pub(super) common_groups: super::profile_panels::common_groups::CommonGroupsUi,
    /// Sign-in: inputs, registration, terms, recovery and the QR code (`ui/auth_state.rs`).
    pub(super) auth_ui: super::auth_state::AuthUi,
    /// Search fields: global search and search in chat (`ui/search_state.rs`).
    pub(super) search_ui: super::search_state::SearchUi,
    /// Send Location's map gesture, search field and timers (`ui/location_picker/state.rs`).
    pub(super) location_ui: super::location_picker::state::LocationPickerUi,
    /// The gallery's search field (`ui/shared_media_search_state.rs`).
    pub(super) media_search: super::shared_media_search_state::SharedMediaSearchUi,
    /// Forwarding and the share box (`ui/share_state.rs`).
    pub(super) share: super::share_state::ShareUi,
    /// Files sent to Quill from outside (`ui/start_files_state.rs`).
    pub(super) start_files: super::start_files_state::StartFilesUi,
    pub(super) ai_compose: super::ai_compose_state::AiComposeUi,
    /// The title bar update pill's motion (`ui/update_pill.rs`).
    pub(super) update_pill: super::update_pill::UpdatePill,
    /// Agent access: the account Settings edits and the all-chats consent (`ui/agent_state.rs`).
    pub(super) agent_ui: super::agent_state::AgentUi,
    /// The agent takeover's wake-up and Esc (`ui/agent_takeover_state.rs`).
    pub(super) takeover: super::agent_takeover_state::TakeoverUi,
    pub(super) focus_sidebar: FocusHandle,
    /// Connection status, the status line and presence sync (`ui/connection_state.rs`).
    pub(super) connection: super::connection_state::ConnectionUi,
    pub(super) live: Option<LiveConnect>,
    /// Slice auth-logout-warning: the startup credentials, kept so a
    /// `logOut`-driven Closed can restart the live connection and return
    /// the user to the login screen (same sensitivity class as the
    /// driver's own copy). Also read by the account switcher
    /// (parity:auth-multi-account) to reconnect as another account.
    pub(super) credentials: Option<TelegramCredentials>,
    /// Screenshot Ready list: same reducers as live, injected JSON only.
    pub(super) demo_session: Option<Box<Session>>,
    /// Screenshot-demo stand-ins for devices, call video and the log sink (`ui/demo_state.rs`).
    pub(super) demo_ui: super::demo_state::DemoUi,
    /// One-to-one call UI: the call window, tick, sounds, images and notifications.
    pub(super) calls: super::calls_state::CallUi,
    /// Notifications: OS clicks, sounds, mute and auto-delete menus, defaults (`ui/notify_state.rs`).
    pub(super) notify: super::notify_state::NotifyUi,
    /// Accounts that are not on screen, still connected (`ui/background_accounts`).
    pub(super) background: super::background_accounts::BackgroundAccounts,
    /// Accounts, account lifecycle, local passcode, freeze and age checks (`ui/account_state.rs`).
    pub(super) account: super::account_state::AccountUi,
    /// Spell checking: the engine, results and the dictionaries box (`ui/spell_state.rs`).
    pub(super) spell: super::spell_state::SpellUi,
    /// Message menu, selection, links and per-message toggles in the history (`ui/message_state.rs`).
    pub(super) message_ui: super::message_state::MessageUi,
    /// Standalone boxes: profiles, contacts, chat look, export, tags and call rating (`ui/dialogs_state.rs`).
    pub(super) dialogs: super::dialogs_state::DialogUi,
    /// Voice, music, GIF, sticker and inline video playback (`ui/playback_state.rs`).
    pub(super) playback: super::playback_state::PlaybackUi,
    /// Cached child views (chat list, conversation) the frame clock can
    /// redraw on their own, and the chat list's animation layer; see
    /// `app_slice`.
    pub(super) slices: super::app_slice::Slices,
    /// Settings → Appearance slice: client-side look-and-feel
    /// (theme/auto-night/accent/wallpaper/font-size/bubbles), persisted
    /// to `appearance_prefs.json`.
    pub(super) appearance: AppearancePrefs,
    /// Chat prefs slice: chat-composer behavior (send-key mode),
    /// persisted to `chat_prefs.json`.
    pub(super) chat_prefs: ChatPrefs,
    /// Privacy and security screens: rules, blocked users, sessions and websites (`ui/privacy_state.rs`).
    pub(super) privacy: super::privacy_state::PrivacyState,
    /// Two-step verification box: its view, inputs and pending confirmation.
    pub(super) twofa: super::twofa_state::TwoStepUi,
    /// Video chat UI: its window, dialogs, composer, pin and push-to-talk.
    pub(super) group_call: super::group_call_state::GroupCallUi,
    /// Screen sharing: the source chooser, privacy box and paused tiles.
    pub(super) screen_share: super::screen_share_state::ScreenShareUi,
    /// Voice and round video recording (`ui/recording_state.rs`).
    pub(super) recording: super::recording_state::RecordingUi,
    /// Group and channel management boxes: invite links, admins, members, permissions (`ui/admin_state.rs`).
    pub(super) admin: super::admin_state::AdminUi,
    /// The members block of the group info panel (`ui/inline_members_state.rs`).
    pub(super) inline_members: super::inline_members_state::InlineMembersUi,
    /// Deep links and bot link confirmations (`ui/links_state.rs`).
    pub(super) links: super::links_state::LinkUi,
    /// The giveaway form (`ui/giveaway_state.rs`).
    pub(super) giveaway: super::giveaway_state::GiveawayUi,
    /// Paid messages: price, confirmation, Remove Fee (`ui/paid_messages_state.rs`).
    pub(super) paid_messages: super::paid_messages_state::PaidMessagesUi,
    /// Suggested posts: price, time and decision boxes (`ui/suggested_posts_state.rs`).
    pub(super) suggested_posts: super::suggested_posts_state::SuggestedPostsUi,
    /// Gift resale, auctions and crafting boxes (`ui/gift_market_state.rs`).
    pub(super) gift_market: super::gift_market_state::GiftMarketUi,
    /// The Star reaction box (`ui/paid_reaction_state.rs`).
    pub(super) paid_reaction: super::paid_reaction_state::PaidReactionUi,
    /// `parity:platform-deep-links`: launch link from the CLI (`t.me` /
    /// `tg:`), set by `main.rs`. Fired once auth reaches Ready, then
    /// cleared (`pub(crate)` so the binary can set it).
    pub(crate) pending_deep_link: Option<String>,
    /// Mini apps: the helper window, its launch and the open box
    /// (`ui/web_app_ui.rs`, `DialogKind::WebAppConfirm`).
    pub(super) mini_apps: super::web_app_ui::MiniApps,
    /// Media viewer: zoom, video, controls and the photo editor (`ui/viewer_state.rs`).
    pub(super) viewer: super::viewer_state::ViewerUi,
    /// Chat folders UI: the tab strip, editor, manager, share and invite boxes.
    pub(super) folders: super::folders_state::FolderUi,
    /// Chats open in windows of their own and the frame scope that names the one being drawn (`ui/chat_window`).
    pub(super) windows: super::chat_window::ChatWindows,
    /// The QR code box (`ui/peer_qr`).
    pub(super) peer_qr: super::peer_qr::PeerQrUi,
    /// The info cover's video profile photo (`ui/video_avatar`).
    pub(super) video_avatar: super::video_avatar::VideoAvatarUi,
    /// The profile cover's rating and last seen boxes (`ui/profile_cover`).
    pub(super) profile_cover: super::profile_cover::ProfileCoverUi,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PaneMode {
    Synthetic,
    Connecting,
    Ready,
}

/// Slice CL2: chat-list category filter (TGX `ChatFilter` unread /
/// archive categories). View state on `QuillApp`, not the session —
/// it filters the already-loaded model, never the server query.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum ChatListFilter {
    #[default]
    All,
    Unread,
    Archived,
    /// Parity slice `parity:communities-chatlist-mode`: community
    /// chat-list mode — the main list shows only the selected
    /// community's chats (membership from the cached
    /// `communityFullInfo.chats` pack; see `community_mode`).
    Community(i64),
}

pub(super) fn pane_placeholder(
    title: &'static str,
    body: impl Into<SharedString>,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement {
    div()
        .id(title)
        .flex()
        .flex_col()
        .flex_1()
        .p_6()
        .gap_2()
        .child(div().font_semibold().child(title))
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(body.into()),
        )
}

impl Focusable for QuillApp {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_sidebar.clone()
    }
}

impl QuillApp {
    pub fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        credentials: Option<TelegramCredentials>,
    ) -> Self {
        Self::new_with_demo(window, cx, credentials, None)
    }

    /// The live receive bridge died on its own. Read from the bridge every
    /// time: a replaced connection has a fresh bridge, so there is no flag
    /// to clear and none to go stale.
    pub(super) fn connection_lost(&self) -> bool {
        self.live
            .as_ref()
            .is_some_and(|live| live.bridge.stopped_unexpectedly())
    }

    pub(super) fn current_auth(&self) -> AuthorizationState {
        if let Some(live) = self.live.as_ref() {
            super::connection_state::auth_for_bridge(
                live.bridge.stopped_unexpectedly(),
                live.driver.session.auth.clone(),
            )
        } else if let Some(session) = self.demo_session.as_ref() {
            session.auth.clone()
        } else {
            self.auth_ui.demo_state.clone()
        }
    }

    pub(crate) fn session(&self) -> Option<&Session> {
        self.live
            .as_ref()
            .map(|live| &live.driver.session)
            .or(self.demo_session.as_deref())
    }

    pub(crate) fn session_mut(&mut self) -> Option<&mut Session> {
        self.live
            .as_mut()
            .map(|live| &mut live.driver.session)
            .or(self.demo_session.as_deref_mut())
    }

    pub(super) fn media_display_roots(&self) -> std::rc::Rc<[PathBuf]> {
        if let Some(roots) = self.frame.media_roots_frame.borrow().as_ref() {
            return roots.clone();
        }
        let roots: std::rc::Rc<[PathBuf]> = self.compute_media_display_roots().into();
        *self.frame.media_roots_frame.borrow_mut() = Some(roots.clone());
        roots
    }

    fn compute_media_display_roots(&self) -> Vec<PathBuf> {
        let primary = if let Some(live) = self.live.as_ref() {
            live.driver.tdlib_media_roots()
        } else if self.demo_session.is_some() {
            let mut roots = vec![demo_media_allowlist()];
            roots.extend(super::demo::demo_stress_avatar_dir());
            roots.extend(super::demo::demo_stress_photo_dir());
            roots
        } else {
            Vec::new()
        };
        if primary.is_empty() {
            primary
        } else {
            quill::voice::with_capture_root(quill::animation::with_gif_frame_cache(primary))
        }
    }

    pub(super) fn pane_mode(&self) -> PaneMode {
        if self
            .session()
            .is_some_and(|session| matches!(session.auth, AuthorizationState::Ready))
        {
            return PaneMode::Ready;
        }
        match self.connection.status {
            ConnectUiStatus::NeedCredentials
                if self.live.is_none() && !self.auth_ui.demo_inputs =>
            {
                PaneMode::Synthetic
            }
            _ => PaneMode::Connecting,
        }
    }
}

impl Drop for QuillApp {
    fn drop(&mut self) {
        self.stop_animation_playback();
        self.stop_sticker_playback();
        self.close_clients_on_drop();
    }
}
