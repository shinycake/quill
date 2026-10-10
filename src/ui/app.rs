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
    /// Stories UI state: the strip, viewer, composer, story page and their boxes.
    pub(super) stories: super::stories_state::StoryUi,
    /// Message history: rows, window, scroll probes, highlights and pinned bar (`ui/history_state.rs`).
    pub(super) history: super::history_state::HistoryUi,
    /// Window and per-frame bookkeeping: animation demand, focus restore, quit guard (`ui/frame_state.rs`).
    pub(super) frame: super::frame_state::FrameUi,
    /// Composer state: menus, attachments, send options, reply and edit drafts (`ui/composer_state.rs`).
    pub(super) composer_ui: super::composer_state::ComposerUi,
    /// Emoji, sticker and GIF panel and their search fields (`ui/pickers_state.rs`).
    pub(super) pickers: super::pickers_state::PickerUi,
    /// Payments and the gift marketplace (`ui/payments_state.rs`).
    pub(super) payments: super::payments_state::PaymentUi,
    /// Sign-in: inputs, registration, terms, recovery and the QR code (`ui/auth_state.rs`).
    pub(super) auth_ui: super::auth_state::AuthUi,
    /// Search fields: global search and search in chat (`ui/search_state.rs`).
    pub(super) search_ui: super::search_state::SearchUi,
    /// Forwarding and the share box (`ui/share_state.rs`).
    pub(super) share: super::share_state::ShareUi,
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
    pub(super) demo_session: Option<Session>,
    /// Screenshot-demo stand-ins for devices, call video and the log sink (`ui/demo_state.rs`).
    pub(super) demo_ui: super::demo_state::DemoUi,
    /// One-to-one call UI: the call window, tick, sounds, images and notifications.
    pub(super) calls: super::calls_state::CallUi,
    /// Notifications: OS clicks, sounds, mute and auto-delete menus, defaults (`ui/notify_state.rs`).
    pub(super) notify: super::notify_state::NotifyUi,
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
    /// Voice and round video recording (`ui/recording_state.rs`).
    pub(super) recording: super::recording_state::RecordingUi,
    /// Group and channel management boxes: invite links, admins, members, permissions (`ui/admin_state.rs`).
    pub(super) admin: super::admin_state::AdminUi,
    /// Deep links and bot link confirmations (`ui/links_state.rs`).
    pub(super) links: super::links_state::LinkUi,
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

    pub(super) fn current_auth(&self) -> AuthorizationState {
        if self.connection.lost {
            // The receive bridge died: treat it like an unexpected Closed.
            AuthorizationState::Closed
        } else if let Some(live) = self.live.as_ref() {
            live.driver.session.auth.clone()
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
            .or(self.demo_session.as_ref())
    }

    pub(super) fn media_display_roots(&self) -> Vec<PathBuf> {
        if let Some(roots) = self.frame.media_roots_frame.borrow().as_ref() {
            return roots.clone();
        }
        let roots = self.compute_media_display_roots();
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
            quill::voice::with_capture_root(quill::video::with_viewer_frame_cache(
                quill::video::with_video_frame_cache(quill::animation::with_gif_frame_cache(
                    primary,
                )),
            ))
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
    }
}
