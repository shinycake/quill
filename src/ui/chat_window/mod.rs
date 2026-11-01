//! A chat in a window of its own (`parity:platform-new-window`).
//!
//! Telegram Desktop (`Core::App().ensureSeparateWindowFor(id)`,
//! core/application.cpp) opens a `Window::Controller` per
//! `SeparateId`: the same account, its own active chat, header, history and
//! compose field, no chat list; a forum opens with its topic list. Opening
//! the chat that is active in the main window moves it there
//! (`SessionController::showInNewWindow` clears the main window's section
//! stack first); asking again for a chat that has a window activates that
//! window; picking the chat in the main window's list or clicking its
//! notification also activates the window
//! (`MainWidget::showHistoryInDifferentWindow`,
//! `Application::windowForShowingHistory`); a chat opened from inside a
//! chat window shows in the main window. Closing the window closes it,
//! nothing else.
//!
//! Quill keeps one `QuillApp` (one `Session`, one TDLib client, the
//! notifications, playback, dialogs and preferences). Each chat window is a
//! [`view::ChatWindowView`] whose [`slot::WindowSlot`] owns what is per
//! window: the history view state, the compose field and its state, the
//! message menu and selection, the pickers, the media viewer, and the
//! session's view of the chat. `QuillApp::set_window_scope` swaps a slot
//! in while its window is drawn or handles input, so the app's own
//! conversation, composer and overlay code serves every window; work that
//! finishes later runs in the window it started in (`scoped`); kit
//! dialogs open in the window whose input asked for them
//! (`QuillShell::sync_kit_dialogs_in`). On the session side the chat is
//! one of `Session::window_chats`.
#![deny(clippy::indexing_slicing)]

use crate::ui::status_note::StatusNote;
use crate::ui::user_error::user_error_for;
mod body;
mod cover;
mod recording;
mod saved;
mod scoped;
mod slot;
#[cfg(test)]
mod tests;
mod view;

use super::app::QuillApp;
use cover::WindowPlace;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::state::ChatSummary;
pub(in crate::ui) use scoped::ScopedApp;
pub(in crate::ui) use slot::scope_probe;
use view::ChatWindowView;
pub(in crate::ui) use view::WindowScope;

/// The open chat windows and the scope bookkeeping (a `QuillApp` field,
/// `windows`).
pub(crate) struct ChatWindows {
    open: Vec<OpenChatWindow>,
    /// The chat whose window's state is in place (`None`: the main
    /// window's). See `slot`.
    pub(super) scope: Option<ChatId>,
    /// The window the last input event happened in (`None`: the main
    /// window): a dialog it asks for opens there.
    pub(super) last_input: Option<ChatId>,
    /// The window showing the kit dialog (`Some(None)`: the main window).
    dialog_host: Option<Option<ChatId>>,
    /// The main window, as last rendered.
    pub(super) main: Option<AnyWindowHandle>,
    /// Where the main window stood at its last render (`cover`).
    main_place: Option<WindowPlace>,
    /// New windows cascade from the main window; this counts them.
    cascade: u32,
    /// A write of the open windows is scheduled (`saved`).
    save_pending: bool,
    /// Windows the user closed, newest last, for "Reopen closed window"
    /// (`SavedWindows::_closed`, at most `kMaxClosedWindows`).
    closed: Vec<(ChatId, Bounds<Pixels>)>,
    /// The windows from last time (`saved`).
    restore: saved::Restore,
    /// Keys go to the window that received them: the interceptor runs
    /// before any handler and puts that window's state in place.
    _key_scope: Subscription,
}

struct OpenChatWindow {
    chat_id: ChatId,
    handle: AnyWindowHandle,
    view: WeakEntity<ChatWindowView>,
    slot: slot::SharedSlot,
}

impl ChatWindows {
    pub(super) fn new(cx: &mut Context<QuillApp>) -> Self {
        let app = cx.weak_entity();
        let key_scope = cx.intercept_keystrokes(move |_, window, cx| {
            let id = window.window_handle().window_id();
            let _ = app.update(cx, |app, _| {
                let chat = app.windows.chat_for_window(id);
                app.note_window_input(chat);
            });
        });
        Self {
            open: Vec::new(),
            scope: None,
            last_input: None,
            dialog_host: None,
            main: None,
            main_place: None,
            cascade: 0,
            save_pending: false,
            closed: Vec::new(),
            restore: saved::Restore::default(),
            _key_scope: key_scope,
        }
    }

    /// `kMaxClosedWindows` in `window_saved_windows.cpp`.
    const MAX_CLOSED: usize = 16;

    /// Remember a window the user closed.
    fn note_closed(&mut self, chat_id: ChatId, bounds: Bounds<Pixels>) {
        while self.closed.len() >= Self::MAX_CLOSED {
            self.closed.remove(0);
        }
        self.closed.push((chat_id, bounds));
    }

    /// Every open chat window, as its chat and window.
    fn open_windows(&self) -> impl Iterator<Item = (ChatId, AnyWindowHandle)> + '_ {
        self.open.iter().map(|open| (open.chat_id, open.handle))
    }

    fn window_for(&self, chat_id: ChatId) -> Option<&OpenChatWindow> {
        self.open.iter().find(|open| open.chat_id == chat_id)
    }

    /// The chat shown by the window with this id, if it is a chat window.
    fn chat_for_window(&self, id: WindowId) -> Option<ChatId> {
        self.open
            .iter()
            .find(|open| open.handle.window_id() == id)
            .map(|open| open.chat_id)
    }

    /// Each open chat window's image cache, and whether the window is
    /// active (for the idle trim).
    pub(super) fn image_caches(
        &self,
        cx: &App,
    ) -> Vec<(Entity<super::image_budget::BoundedImageCache>, bool)> {
        self.open
            .iter()
            .filter_map(|open| {
                let images = open.view.upgrade()?.read(cx).images.clone();
                Some((images, open.slot.borrow().active))
            })
            .collect()
    }

    /// The image cache of the chat window with this id.
    pub(super) fn image_cache_for(
        &self,
        id: WindowId,
        cx: &App,
    ) -> Option<Entity<super::image_budget::BoundedImageCache>> {
        let open = self
            .open
            .iter()
            .find(|open| open.handle.window_id() == id)?;
        Some(open.view.upgrade()?.read(cx).images.clone())
    }

    fn slot_for(&self, chat_id: ChatId) -> Option<slot::SharedSlot> {
        self.window_for(chat_id).map(|open| open.slot.clone())
    }

    /// Whether `chat_id` has a window of its own.
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn has(&self, chat_id: ChatId) -> bool {
        self.window_for(chat_id).is_some()
    }

    /// The view of the window showing `chat_id`.
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn view_for(&self, chat_id: ChatId) -> Option<Entity<ChatWindowView>> {
        self.window_for(chat_id)
            .and_then(|open| open.view.upgrade())
    }

    pub(super) fn view_ids(&self) -> Vec<EntityId> {
        self.open.iter().map(|open| open.view.entity_id()).collect()
    }

    pub(super) fn view_id_for(&self, chat_id: ChatId) -> Option<EntityId> {
        self.window_for(chat_id).map(|open| open.view.entity_id())
    }

    /// Whether a chat window's state is in place.
    pub(super) fn in_window(&self) -> bool {
        self.scope.is_some()
    }

    /// A chat window is the active one (the frame clock runs for it).
    pub(super) fn any_active(&self) -> bool {
        self.open.iter().any(|open| open.slot.borrow().active)
    }

    pub(super) fn any_open(&self) -> bool {
        !self.open.is_empty()
    }

    /// Whether the window `host` (`None`: the main window) may open the
    /// kit dialog the app flags ask for: the one already showing a dialog
    /// keeps it; otherwise the window the last input happened in.
    pub(super) fn dialog_may_open_in(&self, host: Option<ChatId>) -> bool {
        match self.dialog_host {
            Some(current) => current == host,
            None => self.last_input == host,
        }
    }

    pub(super) fn dialog_opened_in(&mut self, host: Option<ChatId>) {
        self.dialog_host = Some(host);
    }

    pub(super) fn dialog_closed_in(&mut self, host: Option<ChatId>) {
        if self.dialog_host == Some(host) {
            self.dialog_host = None;
        }
    }

    /// Bring the window showing `chat_id` to the front, with its compose
    /// field focused (Telegram Desktop `Window::Controller::activate`).
    /// Returns whether there was one.
    fn activate(&self, chat_id: ChatId, cx: &mut App) -> bool {
        let Some(open) = self.window_for(chat_id) else {
            return false;
        };
        cx.activate(true);
        let composer = open.slot.borrow().composer.clone();
        let _ = open.handle.update(cx, |_, window, cx| {
            window.activate_window();
            composer.update(cx, |input, cx| input.focus(window, cx));
        });
        true
    }

    /// Where the next window opens: a 3/4-sized window stepping down and
    /// right from the main window, as Telegram Desktop's child windows
    /// do (`MainWindow::countInitialGeometry`, 3/4 of the default size,
    /// each new child offset from the last).
    fn next_bounds(&mut self, main: Bounds<Pixels>) -> Bounds<Pixels> {
        self.cascade = self.cascade % 6 + 1;
        let step = px(36.) * self.cascade as f32;
        let size = size(
            (main.size.width * 0.75).max(px(480.)),
            (main.size.height * 0.75).max(px(420.)),
        );
        Bounds {
            origin: main.origin + point(step, step),
            size,
        }
    }
}

impl QuillApp {
    /// The chat the content being built now shows: the chat window's
    /// while a chat window is built, laid out or painted, else the main
    /// window's open chat.
    pub(super) fn viewed_chat(&self) -> Option<ChatId> {
        self.windows.scope.or_else(|| self.open_chat_id())
    }

    /// The chat whose window `window` is (`None`: the main window), for
    /// work a window's own callback defers.
    pub(in crate::ui) fn window_scope_of(&self, window: &Window) -> Option<ChatId> {
        self.windows
            .chat_for_window(window.window_handle().window_id())
    }

    /// The open forum topic of the content being built now.
    pub(super) fn viewed_topic(&self) -> Option<i32> {
        self.session().and_then(|session| session.open_topic)
    }

    /// Whether the menus offer "Open in new window" for `chat`. Telegram
    /// Desktop shows it for every peer (`Filler::addNewWindow`); Quill
    /// offers it for every supported chat while unlocked
    /// (`SeparateWindowLocked`).
    pub(super) fn can_open_chat_window(&self, chat: &ChatSummary) -> bool {
        chat.supported() && !self.account.passcode.locked
    }

    /// "Open in new window" (Telegram Desktop `showInNewWindow`): activate
    /// the chat's window when it has one, else open one. The chat active in
    /// the main window moves there (the main window shows no chat then).
    pub(super) fn open_chat_in_new_window(
        &mut self,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_chat_window_at(chat_id, None, window, cx);
    }

    /// [`Self::open_chat_in_new_window`] at `bounds` (a window brought
    /// back from last time), else cascaded from the main window.
    pub(super) fn open_chat_window_at(
        &mut self,
        chat_id: ChatId,
        bounds: Option<Bounds<Pixels>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.windows.activate(chat_id, cx) {
            return;
        }
        let Some(chat) = self
            .session()
            .and_then(|session| session.chats.get(&chat_id.0))
            .cloned()
        else {
            return;
        };
        if !self.can_open_chat_window(&chat) {
            self.connection
                .set_note(StatusNote::error(tr(keys::Q_MISC_THIS_CHAT_CANT_OPEN_IN)));
            cx.notify();
            return;
        }
        // Asked from inside a chat window: the main window's view is the
        // one that lets go of the chat.
        let outer = self.windows.scope;
        self.set_window_scope(None);
        // The session side first: the chat is a window chat before the main
        // window lets go of it, so TDLib keeps it open.
        if let Some(live) = self.live.as_mut() {
            if let Err(err) = live.driver.open_chat_window(chat_id) {
                self.connection.set_note(StatusNote::error(user_error_for(
                    keys::Q_NOTES_FAILED_CHAT_WINDOW,
                    &err,
                )));
                self.set_window_scope(outer);
                cx.notify();
                return;
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.open_window_chat(chat_id);
        }
        if self.open_chat_id() == Some(chat_id) {
            self.leave_open_chat(window, cx);
        }
        self.set_window_scope(outer);
        // Cascade from the main window, wherever the request came from.
        let bounds = bounds.unwrap_or_else(|| {
            let main = self
                .windows
                .main
                .and_then(|main| main.update(cx, |_, window, _| window.bounds()).ok())
                .unwrap_or_else(|| window.bounds());
            self.windows.next_bounds(main)
        });
        let owner = cx.entity();
        // window-scope: window bookkeeping by chat id; the draft goes in with
        // the new window's state in place (`in_window_scope` below).
        cx.defer(move |cx| {
            let opened = view::open_chat_window(owner.clone(), chat_id, bounds, cx);
            let (handle, view, slot) = match opened {
                Ok(opened) => opened,
                Err(_) => {
                    owner.update(cx, |app, cx| {
                        app.connection.set_note(StatusNote::error(tr(
                            keys::Q_MISC_COULDNT_OPEN_THE_CHAT_WINDOW,
                        )));
                        app.chat_window_closed(chat_id, None, cx);
                    });
                    return;
                }
            };
            owner.update(cx, |app, cx| {
                app.windows.open.push(OpenChatWindow {
                    chat_id,
                    handle,
                    view,
                    slot,
                });
                app.schedule_chat_windows_save(cx);
                cx.notify();
            });
            // The chat's draft goes into the window's compose field, which
            // takes the keyboard.
            let _ = handle.update(cx, |_, window, cx| {
                owner.update(cx, |app, cx| {
                    app.in_window_scope(Some(chat_id), cx, |app, cx| {
                        app.restore_open_draft(window, cx);
                        app.composer.update(cx, |input, cx| input.focus(window, cx));
                    });
                });
            });
        });
        cx.notify();
    }

    /// Picking a chat that has its own window shows that window instead
    /// of the main window switching to it (Telegram Desktop
    /// `MainWidget::showHistoryInDifferentWindow`). Returns whether the
    /// request was taken over.
    pub(super) fn routed_to_chat_window(&self, chat_id: ChatId, cx: &mut App) -> bool {
        self.windows.activate(chat_id, cx)
    }

    /// A chat chosen from inside a chat window (a link, a search hit, a
    /// mention) opens in the main window, as Telegram Desktop hands a
    /// non-primary window's navigation to the primary one. Returns
    /// whether the request was taken over (`false` in the main window).
    pub(super) fn routed_to_main_window(
        &mut self,
        chat_id: ChatId,
        thread_switch: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(from) = self.windows.scope else {
            return false;
        };
        if from == chat_id {
            // The window shows it already.
            return true;
        }
        let Some(main) = self.windows.main else {
            return false;
        };
        let owner = cx.entity();
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, cx| {
                owner.update(cx, |app, cx| {
                    app.in_window_scope(None, cx, |app, cx| {
                        app.select_chat_with(chat_id, thread_switch, window, cx);
                    });
                });
                cx.activate(true);
                window.activate_window();
            });
        });
        true
    }

    /// "Reopen closed window" (`SavedWindows::reopenLastClosed`): the chat
    /// window closed last opens again where it stood. `false` when none was
    /// closed.
    pub(in crate::ui) fn reopen_last_closed_chat_window(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((chat_id, bounds)) = self.windows.closed.pop() else {
            return false;
        };
        self.open_chat_window_at(chat_id, Some(bounds), window, cx);
        true
    }

    /// The window showing `chat_id` is gone (closed by the user, or by the
    /// app because the chat or the session went away).
    ///
    /// `reopen_at` is where a window the user closed stood: it can come
    /// back with "Reopen closed window".
    pub(super) fn chat_window_closed(
        &mut self,
        chat_id: ChatId,
        reopen_at: Option<Bounds<Pixels>>,
        cx: &mut Context<Self>,
    ) {
        if let Some(bounds) = reopen_at {
            self.windows.note_closed(chat_id, bounds);
        }
        self.forget_chat_window(chat_id, cx);
        self.schedule_chat_windows_save(cx);
        cx.notify();
    }

    /// Drop the window's bookkeeping, end what it was recording and close
    /// its chat in TDLib.
    fn forget_chat_window(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        // Its state must not stay in the app's place.
        if self.windows.scope == Some(chat_id) {
            self.set_window_scope(None);
        }
        self.end_recording_of_window(chat_id, cx);
        self.windows.open.retain(|open| open.chat_id != chat_id);
        // Its activation goes with it: GPUI drops the window before the
        // platform reports it inactive, so its observer never runs, and a
        // main window that draws nothing would leave the gate on. Only the
        // active chat window covers the main one.
        let covered = self.frame.main_covered.get() && self.windows.any_active();
        self.frame.main_covered.set(covered);
        self.follow_windows(cx);
        self.drop_play_intent_in_window(chat_id);
        if self.windows.last_input == Some(chat_id) {
            self.windows.last_input = None;
        }
        self.windows.dialog_closed_in(Some(chat_id));
        self.set_window_chat_active(chat_id, false);
        self.playback.userpic_videos.forget_window(chat_id);
        self.playback.inline_videos.forget_window(chat_id, cx);
        self.playback.stickers.window_closed(chat_id);
        self.leave_sticker_chat(
            Some(chat_id),
            Some(chat_id),
            super::sticker_player::Leaving::Window,
            cx,
        );
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.close_chat_window(chat_id);
        } else if let Some(session) = self.demo_session.as_mut() {
            session.close_window_chat(chat_id);
        }
    }

    /// Whether a window other than `window` (`None`: the main window)
    /// shows `chat`. A chat can show in two: the main window opens a
    /// comment thread in its discussion group while the group has a window
    /// of its own (`select_chat_with` with `thread_switch`).
    pub(in crate::ui) fn chat_shown_elsewhere(
        &mut self,
        chat: ChatId,
        window: Option<ChatId>,
        cx: &mut Context<Self>,
    ) -> bool {
        let in_chat_window = window != Some(chat) && self.windows.window_for(chat).is_some();
        let in_main = window.is_some()
            && self.in_window_scope(None, cx, |app, _| app.open_chat_id()) == Some(Some(chat));
        in_chat_window || in_main
    }

    /// Whether `chat_id`'s window is the active one, for the session's
    /// notification decision (a message shown in an active window is read
    /// on arrival, so it does not notify; Telegram Desktop reads it when
    /// the active window shows it).
    pub(super) fn set_window_chat_active(&mut self, chat_id: ChatId, active: bool) {
        let Some(session) = self
            .live
            .as_mut()
            .map(|live| &mut live.driver.session)
            .or(self.demo_session.as_deref_mut())
        else {
            return;
        };
        if active && session.window_chats.contains(&chat_id) {
            session.active_window_chats.insert(chat_id);
        } else {
            session.active_window_chats.remove(&chat_id);
        }
    }

    /// The main window shows no chat any more (Telegram Desktop
    /// `clearSectionStack` when the active chat moves to its own window):
    /// the composer's text becomes the chat's draft, the per-chat composer
    /// and message state goes, and the pane shows "Select a chat".
    pub(super) fn leave_open_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.flush_leaving_draft(cx);
        self.composer_ui.set_reply(None);
        self.composer_ui.pending_edit = None;
        self.composer_ui.saved_edit_draft.clear();
        self.composer_ui.saved_edit_reply = None;
        self.message_ui.pending_delete = None;
        self.message_ui.pending_stop_poll = None;
        self.notify.ttl_picker_open = false;
        if self.recording_here() {
            self.cancel_recording(cx);
        }
        self.stop_animation_playback();
        self.playback.autoplayed_gifs.clear();
        self.leave_sticker_chat(
            None,
            self.open_chat_id(),
            super::sticker_player::Leaving::Chat,
            cx,
        );
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.deselect_chat();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.leave_open_chat();
        }
        self.set_composer_markup("", window, cx);
        self.composer_ui.prev_text.clear();
        self.history.rows_key = None;
        if self
            .session()
            .is_some_and(|session| session.users_state.open_info_panel.is_some())
        {
            self.close_info_panel(cx);
        }
        cx.notify();
    }

    /// The native title of `chat_id`'s window: the chat name, with its
    /// unread count and the total like the main window's (Telegram Desktop
    /// `MainWindow::updateTitle` runs the same way for every window).
    pub(super) fn chat_window_title(&self, chat_id: ChatId) -> String {
        use quill::window_title::{TitleChat, window_title};
        let Some(session) = self.session() else {
            return window_title(None, 0, None);
        };
        if self.account.passcode.locked {
            return window_title(None, 0, None);
        }
        // Every account counts, like tdesktop's `Core::App().unreadBadge()`.
        let total = quill::tray::badge_count(session, &session.settings.badge_prefs)
            .saturating_add(quill::notify_accounts::other_accounts_unread().count);
        let chat = session.chats.get(&chat_id.0).map(|chat| TitleChat {
            name: &chat.title,
            unread: chat.unread_count.max(0) as u32,
        });
        window_title(chat, total, self.title_account_name().as_deref())
    }
}
