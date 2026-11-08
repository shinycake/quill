//! The UI of the account on screen, as one value.
//!
//! `QuillApp` draws for one account at a time. Everything that account has
//! open (its chat list, history, composer, viewer, boxes, menus, the mini
//! app) sits in [`OnScreen`], and an account switch replaces that value
//! whole ([`QuillApp::take_off_screen`]): nothing a box or a viewer held for
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
}

impl OnScreen {
    /// A fresh screen for one account: its text fields, scroll handles and
    /// the subscriptions that drive them. Everything here is dropped with
    /// the value, the subscriptions included (they belong to the fields'
    /// entities).
    pub(super) fn new(
        epoch: ScreenEpoch,
        chat_prefs: &ChatPrefs,
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
        self.screen = OnScreen::new(epoch, &self.chat_prefs, window, cx);
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
        let Some(handle) = self.windows.main else {
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
