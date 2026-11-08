//! The UI of the account on screen, as one value.
//!
//! `QuillApp` draws for one account at a time. Everything that account has
//! open (its chat list, history, composer, viewer, boxes, menus, the mini
//! app) sits in [`OnScreen`], and an account switch replaces that value
//! whole ([`QuillApp::replace_screen`]): nothing a box or a viewer held for
//! the old account can act for the new one, and no list of fields to clear
//! has to be kept. Telegram Desktop does the same with one
//! `SessionController` per account on screen. Design and the state of the
//! migration: docs/decisions/account-scoped-ui-state.md.
//!
//! A feature struct (`ui/<feature>_state.rs`) is a field here unless it is
//! the process's (then it is a field of `QuillApp`, listed in
//! `scripts/app-wide-state.txt`). Every field type implements [`Farewell`]:
//! what that feature tells TDLib, disk or the OS through the *old* client
//! before its state is dropped. Most say nothing.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::*;
use quill::composer::should_send_on_enter;
use quill::connect::LiveConnect;
use quill::lang::{keys, tr};
use quill::settings::ChatPrefs;

/// One account's turn on screen. Monotonic per process: A → B → A gives A a
/// new epoch, so a continuation from its first turn (a box that is gone)
/// does not run in its second. A key would match again.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub(crate) struct ScreenEpoch(u64);

impl ScreenEpoch {
    /// The epoch after this one.
    pub(super) fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

/// What one feature's UI says, through the client it was opened on, when
/// the account on screen is about to change: the mini app's `closeWebApp`,
/// the story viewer's `closeStory`, the open chat's draft flush and leave,
/// a recording's cancel. Most features say nothing and keep the default.
///
/// It takes the app, not the feature struct alone, because the goodbyes
/// are the app's closers (the chat leave reads the composer, the history,
/// the session and the recording at once); the forcing function is the
/// `impl`, one per field type of [`OnScreen`].
pub(crate) trait Farewell {
    fn farewell(_app: &mut QuillApp, _window: &mut Window, _cx: &mut Context<QuillApp>) {}
}

impl<T: Farewell> Farewell for Box<T> {
    fn farewell(app: &mut QuillApp, window: &mut Window, cx: &mut Context<QuillApp>) {
        T::farewell(app, window, cx);
    }
}

/// The composer field itself: its text goes with the chat leave
/// (`HistoryUi`'s farewell).
impl Farewell for Entity<TextareaState> {}

/// Declares [`OnScreen`] and `QuillApp::farewell_screen`, which runs every
/// field's [`Farewell`] in field order. One list: a field cannot be
/// declared without its goodbye, and the order of goodbyes is the order
/// of fields (the mini app before the chat leave).
macro_rules! on_screen_fields {
    ($(
        $(#[$meta:meta])*
        $field:ident : $ty:ty
    ),+ $(,)?) => {
        pub struct OnScreen {
            /// Changes with every replacement; work started under an older
            /// epoch does not run (`chat_window::scoped::ScopedApp`).
            pub(super) epoch: ScreenEpoch,
            /// A client was put on this screen (`QuillApp::put_on_screen`):
            /// it must be replaced before that client leaves.
            client_filled: bool,
            $(
                $(#[$meta])*
                pub(super) $field: $ty,
            )+
        }

        impl OnScreen {
            /// The declared fields, in order: a test checks a flag's struct
            /// against this list instead of naming it by hand.
            #[cfg(test)]
            pub(super) const FIELD_NAMES: &'static [&'static str] = &[$(stringify!($field)),+];
        }

        impl QuillApp {
            /// What the account leaving the screen says, through its own
            /// client (still `self.live` here), before its UI is dropped.
            pub(super) fn farewell_screen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
                $( <$ty as Farewell>::farewell(self, window, cx); )+
            }
        }
    };
}

on_screen_fields! {
    /// Mini apps: the helper window, its launch and the open box
    /// (`ui/web_app_ui.rs`, `DialogKind::WebAppConfirm`). First: its
    /// `closeWebApp` goes before the chat leave.
    mini_apps: super::web_app_ui::MiniApps,
    /// Message history: rows, window, scroll probes, highlights and pinned bar (`ui/history_state.rs`).
    /// Its farewell is the chat leave: the draft is saved through the old client.
    history: super::history_state::HistoryUi,
    /// Voice and round video recording (`ui/recording_state.rs`).
    recording: super::recording_state::RecordingUi,
    /// What plays for the account on screen: the track, the GIF, seek state (`ui/playback_state.rs`).
    playback: super::playback_state::PlaybackUi,
    /// Media viewer: zoom, video, controls and the photo editor (`ui/viewer_state.rs`).
    viewer: super::viewer_state::ViewerUi,
    /// Stories UI state: the strip, viewer, composer, story page and their boxes.
    stories: super::stories_state::StoryUi,
    /// Chat list: rows, filter, menus, pinning, preview and side tabs (`ui/chat_list_state.rs`).
    chat_list: super::chat_list_state::ChatListUi,
    /// The main window's compose field.
    composer: Entity<TextareaState>,
    /// Composer state: menus, attachments, send options, reply and edit drafts (`ui/composer_state.rs`).
    composer_ui: Box<super::composer_state::ComposerUi>,
    /// Emoji, sticker and GIF panel and their search fields (`ui/pickers_state.rs`).
    pickers: super::pickers_state::PickerUi,
    /// Search fields: global search and search in chat (`ui/search_state.rs`).
    search_ui: super::search_state::SearchUi,
    /// The sticker set box's editing flows (`ui/sticker_set_box/state.rs`).
    sticker_box: super::sticker_set_box::state::StickerSetBoxUi,
    /// Forwarding and the share box (`ui/share_state.rs`).
    share: super::share_state::ShareUi,
    /// Message menu, selection, links and per-message toggles in the history (`ui/message_state.rs`).
    message_ui: super::message_state::MessageUi,
    /// Mute, sound and auto-delete menus and the notification boxes (`ui/notify_state.rs`).
    notify: super::notify_state::NotifyUi,
    /// Chat folders UI: the tab strip, editor, manager, share and invite boxes.
    folders: super::folders_state::FolderUi,
    /// Standalone boxes: profiles, contacts, chat look, export, tags and call rating (`ui/dialogs_state.rs`).
    dialogs: super::dialogs_state::DialogUi,
    /// Group and channel management boxes: invite links, admins, members, permissions (`ui/admin_state.rs`).
    admin: super::admin_state::AdminUi,
    /// Payments and the gift marketplace (`ui/payments_state.rs`).
    payments: super::payments_state::PaymentUi,
    /// Revenue pages, withdrawals and affiliate programs (`ui/earn_state.rs`).
    earn: super::earn_state::EarnUi,
    /// Gift resale, auctions and crafting boxes (`ui/gift_market_state.rs`).
    gift_market: super::gift_market_state::GiftMarketUi,
    /// The giveaway form (`ui/giveaway_state.rs`).
    giveaway: super::giveaway_state::GiveawayUi,
    /// Paid messages: price, confirmation, Remove Fee (`ui/paid_messages_state.rs`).
    paid_messages: super::paid_messages_state::PaidMessagesUi,
    /// The Star reaction box (`ui/paid_reaction_state.rs`).
    paid_reaction: super::paid_reaction_state::PaidReactionUi,
    /// Suggested posts: price, time and decision boxes (`ui/suggested_posts_state.rs`).
    suggested_posts: super::suggested_posts_state::SuggestedPostsUi,
    /// Channel direct messages: the admin's and the writer's boxes (`ui/direct_messages_state.rs`).
    direct_messages: super::direct_messages_state::DirectMessagesUi,
    /// Privacy and security screens: rules, blocked users, sessions and websites (`ui/privacy_state.rs`).
    privacy: super::privacy_state::PrivacyState,
    /// Two-step verification box: its view, inputs and pending confirmation.
    twofa: super::twofa_state::TwoStepUi,
    /// Deep links and bot link confirmations (`ui/links_state.rs`).
    links: super::links_state::LinkUi,
    /// The QR code box (`ui/peer_qr`).
    peer_qr: super::peer_qr::PeerQrUi,
    /// The profile cover's rating and last seen boxes (`ui/profile_cover`).
    profile_cover: super::profile_cover::ProfileCoverUi,
    /// The info cover's video profile photo (`ui/video_avatar`).
    video_avatar: super::video_avatar::VideoAvatarUi,
    /// The members block of the group info panel (`ui/inline_members_state.rs`).
    inline_members: super::inline_members_state::InlineMembersUi,
    /// The search field above a profile's groups in common.
    common_groups: super::profile_panels::common_groups::CommonGroupsUi,
    /// The gallery's search field (`ui/shared_media_search_state.rs`).
    media_search: super::shared_media_search_state::SharedMediaSearchUi,
    /// Scroll positions of the shared-media grid tabs (`ui/shared_media_grid_state.rs`).
    shared_media_grid: super::shared_media_grid_state::SharedMediaGridUi,
    /// The Premium feature preview's clip player (`ui/premium_preview_state.rs`).
    premium_preview: super::premium_preview_state::PremiumPreviewUi,
    /// Send Location's map gesture, search field and timers (`ui/location_picker/state.rs`).
    location_ui: super::location_picker::state::LocationPickerUi,
    /// The AI compose box, its style editor and preview (`ui/ai_compose_state.rs`).
    ai_compose: super::ai_compose_state::AiComposeUi,
    /// The settings window: its pages, search and the boxes they open (`ui/settings_state.rs`).
    settings: super::settings_state::SettingsUi,
    /// The boxes around 1:1 calls: confirm, dialing, the microphone test, Add People (`ui/calls_state.rs`).
    call_boxes: super::calls_state::CallBoxes,
    /// The boxes around a video chat: start, invite, share link (`ui/group_call_state.rs`).
    group_call_boxes: super::group_call_state::GroupCallBoxes,
    /// Sign-in: inputs, registration, terms, recovery and the QR code (`ui/auth_state.rs`).
    auth_ui: super::auth_state::AuthUi,
    /// Agent access: the account Settings edits, the consent box and the page (`ui/agent_state.rs`).
    agent_ui: super::agent_state::AgentUi,
    /// Connection status, the status note and its toasts, presence sync (`ui/connection_state.rs`).
    connection: super::connection_state::ConnectionUi,
    /// Chats open in windows of their own and the frame scope that names the one being drawn (`ui/chat_window`).
    /// Last: its windows close after everything they showed has said goodbye.
    windows: super::chat_window::ChatWindows,
}

impl OnScreen {
    /// A fresh screen for one account: its text fields, scroll handles and
    /// the subscriptions that drive them. Everything here is dropped with
    /// the value, the subscriptions included (they belong to the fields'
    /// entities).
    pub(super) fn new(
        epoch: ScreenEpoch,
        chat_prefs: &ChatPrefs,
        connect_status: super::connect_ui::ConnectUiStatus,
        status_note: String,
        window: &mut Window,
        cx: &mut Context<QuillApp>,
    ) -> Self {
        let composer = QuillApp::new_composer_field(chat_prefs.send_key_mode, window, cx);
        let emoji_search_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_MISC_SEARCH_EMOJI))
                .auto_grow(1, 1)
        });
        let reaction_search_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_MISC_SEARCH_EMOJI))
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
        let search_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_MISC_SEARCH))
                .auto_grow(1, 1)
                .submit_on_enter(true)
        });
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
                        // Not a chat composer: the send-key setting does not apply.
                        quill::composer::SendKeyMode::Enter,
                    ) {
                        this.activate_first_search_result(window, cx);
                    }
                }
            },
        )
        .detach();
        let chat_search_input = QuillApp::new_chat_search_field(window, cx);
        let forward_search_input = QuillApp::new_forward_search_field(window, cx);
        let history = super::history_state::HistoryUi::new(cx);
        // The scroller notifies on every scroll: drives the floating date.
        cx.observe(&history.scroller, |this, _, cx| {
            this.note_history_scroll(cx);
        })
        .detach();
        let message_ui = super::message_state::MessageUi::new(window, cx);
        super::history_a11y::install(&message_ui.history_focus, window, cx);
        Self {
            epoch,
            client_filled: false,
            mini_apps: Default::default(),
            history,
            recording: super::recording_state::RecordingUi::new(),
            playback: super::playback_state::PlaybackUi::new(),
            viewer: super::viewer_state::ViewerUi::new(),
            stories: super::stories_state::StoryUi::new(window, cx),
            chat_list: super::chat_list_state::ChatListUi::new(window, cx),
            composer,
            composer_ui: Box::new(super::composer_state::ComposerUi::new()),
            pickers: super::pickers_state::PickerUi::new(
                window,
                cx,
                emoji_search_input,
                reaction_search_input,
            ),
            search_ui: super::search_state::SearchUi::new(search_input, chat_search_input),
            sticker_box: super::sticker_set_box::state::StickerSetBoxUi::new(window, cx),
            share: super::share_state::ShareUi::new(window, cx, forward_search_input),
            message_ui,
            notify: super::notify_state::NotifyUi::new(),
            folders: super::folders_state::FolderUi::new(),
            dialogs: super::dialogs_state::DialogUi::new(window, cx),
            admin: super::admin_state::AdminUi::new(),
            payments: super::payments_state::PaymentUi::new(window, cx),
            earn: super::earn_state::EarnUi::new(window, cx),
            gift_market: super::gift_market_state::GiftMarketUi::new(),
            giveaway: super::giveaway_state::GiveawayUi::new(),
            paid_messages: super::paid_messages_state::PaidMessagesUi::new(),
            paid_reaction: super::paid_reaction_state::PaidReactionUi::new(),
            suggested_posts: super::suggested_posts_state::SuggestedPostsUi::new(),
            direct_messages: super::direct_messages_state::DirectMessagesUi::new(),
            privacy: super::privacy_state::PrivacyState::new(window, cx),
            twofa: super::twofa_state::TwoStepUi::new(window, cx),
            links: super::links_state::LinkUi::new(),
            peer_qr: super::peer_qr::PeerQrUi::new(cx),
            profile_cover: Default::default(),
            video_avatar: Default::default(),
            inline_members: super::inline_members_state::InlineMembersUi::new(),
            common_groups: Default::default(),
            media_search: super::shared_media_search_state::SharedMediaSearchUi::new(window, cx),
            shared_media_grid: super::shared_media_grid_state::SharedMediaGridUi::new(),
            premium_preview: Default::default(),
            location_ui: Default::default(),
            ai_compose: super::ai_compose_state::AiComposeUi::new(window, cx),
            settings: super::settings_state::SettingsUi::new(cx),
            call_boxes: super::calls_state::CallBoxes::new(),
            group_call_boxes: super::group_call_state::GroupCallBoxes::new(),
            auth_ui: super::auth_state::AuthUi::new(window, cx),
            agent_ui: super::agent_state::AgentUi::new(),
            connection: super::connection_state::ConnectionUi::new(connect_status, status_note),
            windows: super::chat_window::ChatWindows::new(),
        }
    }
}

/// `self.chat_list`, `this.viewer`, `app.stories` keep reading as before
/// the move: the fields of the account on screen are reached through the
/// app. Not a smart pointer in disguise: `OnScreen` *is* the state the app
/// renders, and the app is its one owner. A `&mut` reached this way borrows
/// the whole app, so an expression that also needs a disjoint app field
/// spells `self.screen.<field>`.
impl std::ops::Deref for QuillApp {
    type Target = OnScreen;

    fn deref(&self) -> &OnScreen {
        &self.screen
    }
}

impl std::ops::DerefMut for QuillApp {
    fn deref_mut(&mut self) -> &mut OnScreen {
        &mut self.screen
    }
}

impl QuillApp {
    /// The screen the account on screen draws on now.
    pub(in crate::ui) fn screen_epoch(&self) -> ScreenEpoch {
        self.screen.epoch
    }

    /// The account on screen is leaving: its UI, goodbyes said
    /// (`end_account_surfaces`), is replaced whole, and the epoch moves, so
    /// work started before does not run after. The chat windows' state is
    /// swapped back first (`set_window_scope`), so nothing of the old
    /// screen stays in a slot. The client itself leaves right after, through
    /// [`Self::take_live`]; a client filling an empty screen keeps the
    /// screen ([`Self::put_on_screen`]), because nothing on it was any
    /// account's while there was no client.
    pub(in crate::ui) fn replace_screen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_window_scope(None);
        let epoch = self.screen.epoch.next();
        // The status is the connection's, which the switch sets again right
        // after (`adopt_live`, `start_connection`); a fresh default would
        // show the synthetic pane for a frame.
        let status = self.connection.status.clone();
        self.screen = OnScreen::new(epoch, &self.chat_prefs, status, String::new(), window, cx);
        cx.notify();
    }

    /// The client on screen leaves with no window at hand (the poll loop's
    /// logout, a full disk, a connection that replaces it). The epoch moves
    /// now, so nothing of the old screen's work runs; the goodbyes and the
    /// replacement run on the main window at the end of this update, with
    /// the client already gone, as these paths always did (a logged-out
    /// client has nothing to say). Without a main window the UI stays, as
    /// it did before, under the new epoch.
    pub(in crate::ui) fn screen_leaves_later(&mut self, cx: &mut Context<Self>) {
        self.screen.epoch = self.screen.epoch.next();
        self.screen.client_filled = false;
        let Some(handle) = self.main_window.handle else {
            return;
        };
        let app = cx.entity().downgrade();
        // screen-scope: the goodbyes of the screen that just left; its
        // epoch has already moved, and no client is on screen yet.
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                let _ = app.update(cx, |this, cx| this.end_account_surfaces(window, cx));
            });
        });
    }

    /// The client leaves the screen, for the background or the closing set.
    /// Only after [`Self::replace_screen`] or [`Self::screen_leaves_later`]:
    /// a client never leaves with the UI it drew for still in place. (Quit
    /// takes `live` directly: the app is going away with everything on it.)
    pub(in crate::ui) fn take_live(&mut self) -> Option<LiveConnect> {
        debug_assert!(
            !self.screen.client_filled,
            "the client leaves the screen before its UI is replaced"
        );
        self.live.take()
    }

    /// A client fills the empty screen: the chat list, folders and counters
    /// are its from the next frame. The only place `self.live` is filled.
    pub(in crate::ui) fn put_on_screen(&mut self, live: LiveConnect) {
        debug_assert!(self.live.is_none(), "a client is already on screen");
        self.screen.client_filled = true;
        self.live = Some(live);
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "on_screen_tests.rs"]
mod tests;
