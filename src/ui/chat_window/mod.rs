//! A chat in a window of its own (`parity:platform-new-window`).
//!
//! Telegram Desktop (`Core::App().ensureSeparateWindowFor(id)`,
//! core/application.cpp) opens a `Window::Controller` per
//! `SeparateId`: the same account, its own active chat, header, history and
//! compose field, no chat list. Opening the chat that is active in the main
//! window moves it there (`SessionController::showInNewWindow` clears the
//! main window's section stack first); asking again for a chat that has a
//! window activates that window; picking the chat in the main window's list
//! or clicking its notification also activates the window
//! (`MainWidget::showHistoryInDifferentWindow`,
//! `Application::windowForShowingHistory`). Closing the window closes it,
//! nothing else.
//!
//! Quill keeps one `QuillApp` (one `Session`, one TDLib client, the
//! notifications, playback, dialogs and preferences). Each chat window is a
//! [`view::ChatWindowView`] that owns what is per window: the chat it
//! shows, its history view state (`HistoryUi`: scroller, row cache,
//! probes) and its compose field. The window's body is built by `QuillApp`'s
//! own header and history code; while a chat window is built, laid out or
//! painted, its `HistoryUi` is swapped into `QuillApp::history` and
//! `ChatWindows::scope` names its chat, so `QuillApp::viewed_chat` resolves
//! to it instead of the main window's `open_chat` (see `view::WindowScope`).
//! On the session side the chat is one of `Session::window_chats`.

mod body;
#[cfg(test)]
mod tests;
mod view;

use super::app::QuillApp;
use gpui_kit::*;
use quill::composer::{ComposerSnapshot, DraftTopic};
use quill::ids::ChatId;
use quill::state::ChatSummary;
pub(super) use view::ChatWindowView;

/// The open chat windows and the frame-scope bookkeeping (a `QuillApp`
/// field, `windows`).
#[derive(Default)]
pub(crate) struct ChatWindows {
    open: Vec<OpenChatWindow>,
    /// The chat whose window is being built, laid out or painted (`None`:
    /// the main window). Set by `view::WindowScope`.
    pub(super) scope: Option<ChatId>,
    /// New windows cascade from the main window; this counts them.
    cascade: u32,
}

struct OpenChatWindow {
    chat_id: ChatId,
    handle: AnyWindowHandle,
    view: WeakEntity<ChatWindowView>,
}

impl ChatWindows {
    /// Whether `chat_id` has a window of its own.
    #[cfg(all(test, feature = "demo-capture"))]
    pub(super) fn has(&self, chat_id: ChatId) -> bool {
        self.window_for(chat_id).is_some()
    }

    fn window_for(&self, chat_id: ChatId) -> Option<&OpenChatWindow> {
        self.open.iter().find(|open| open.chat_id == chat_id)
    }

    /// The view of the window showing `chat_id`.
    #[cfg(all(test, feature = "demo-capture"))]
    pub(super) fn view_for(&self, chat_id: ChatId) -> Option<Entity<ChatWindowView>> {
        self.window_for(chat_id)
            .and_then(|open| open.view.upgrade())
    }

    /// Bring the window showing `chat_id` to the front, with its compose
    /// field focused (Telegram Desktop `Window::Controller::activate`).
    /// Returns whether there was one.
    fn activate(&self, chat_id: ChatId, cx: &mut App) -> bool {
        let Some(open) = self.window_for(chat_id) else {
            return false;
        };
        cx.activate(true);
        let view = open.view.clone();
        let _ = open.handle.update(cx, |_, window, cx| {
            window.activate_window();
            if let Some(view) = view.upgrade() {
                view.read(cx)
                    .composer
                    .clone()
                    .update(cx, |input, cx| input.focus(window, cx));
            }
        });
        true
    }

    /// Whether a chat window's body is being built now.
    pub(super) fn in_window(&self) -> bool {
        self.scope.is_some()
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

    /// The open forum topic of the content being built now: a chat window
    /// shows its chat's plain history, so none there.
    pub(super) fn viewed_topic(&self) -> Option<i32> {
        if self.windows.in_window() {
            None
        } else {
            self.session().and_then(|session| session.open_topic)
        }
    }

    /// Whether the menus offer "Open in new window" for `chat`. Telegram
    /// Desktop shows it for every peer (`Filler::addNewWindow`, forums as a
    /// Forum window with the topic list, communities as a Community
    /// window); Quill's chat window shows a plain history, so forums (their
    /// topic list) wait for a later slice, as does the locked state
    /// (`SeparateWindowLocked`).
    pub(super) fn can_open_chat_window(&self, chat: &ChatSummary) -> bool {
        chat.supported() && !chat.is_forum_chat() && !self.account.passcode.locked
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
            self.connection.status_note = "This chat can't open in a new window yet.".into();
            cx.notify();
            return;
        }
        // The session side first: the chat is a window chat before the main
        // window lets go of it, so TDLib keeps it open.
        if let Some(live) = self.live.as_mut() {
            if let Err(err) = live.driver.open_chat_window(chat_id) {
                self.connection.status_note = format!("could not open the chat window: {err:?}");
                cx.notify();
                return;
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.open_window_chat(chat_id);
        }
        if self.open_chat_id() == Some(chat_id) {
            self.leave_open_chat(window, cx);
        }
        let bounds = self.windows.next_bounds(window.bounds());
        let owner = cx.entity();
        cx.defer(move |cx| {
            let result = view::open_chat_window(owner.clone(), chat_id, bounds, cx);
            owner.update(cx, |app, cx| {
                match result {
                    Ok((handle, view)) => app.windows.open.push(OpenChatWindow {
                        chat_id,
                        handle,
                        view,
                    }),
                    Err(_) => {
                        app.connection.status_note = "Couldn't open the chat window".into();
                        app.chat_window_closed(chat_id, cx);
                    }
                }
                cx.notify();
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

    /// The window showing `chat_id` is gone (closed by the user, or by the
    /// app because the chat or the session went away).
    pub(super) fn chat_window_closed(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        self.windows.open.retain(|open| open.chat_id != chat_id);
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.close_chat_window(chat_id);
        } else if let Some(session) = self.demo_session.as_mut() {
            session.close_window_chat(chat_id);
        }
        cx.notify();
    }

    /// The main window shows no chat any more (Telegram Desktop
    /// `clearSectionStack` when the active chat moves to its own window):
    /// the composer's text becomes the chat's draft, the per-chat composer
    /// and message state goes, and the pane shows "Select a chat".
    pub(super) fn leave_open_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.flush_leaving_draft(cx);
        self.composer_ui.pending_reply = None;
        self.composer_ui.pending_edit = None;
        self.composer_ui.saved_edit_draft.clear();
        self.composer_ui.saved_edit_reply = None;
        self.message_ui.pending_delete = None;
        self.message_ui.pending_stop_poll = None;
        self.notify.ttl_picker_open = false;
        if self.recording_active() {
            self.cancel_recording(cx);
        }
        self.stop_animation_playback();
        self.playback.autoplayed_gifs.clear();
        self.stop_sticker_playback();
        self.stop_video_playback();
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

    /// Send the text typed in `chat_id`'s window. Only text: the window's
    /// compose field has no attachments, replies or edits yet.
    pub(super) fn send_text_to_chat(
        &mut self,
        chat_id: ChatId,
        markup: String,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if markup.trim().is_empty() {
            return Ok(());
        }
        let Some(chat) = self
            .session()
            .and_then(|session| session.chats.get(&chat_id.0))
            .cloned()
        else {
            return Err("This chat is gone.".into());
        };
        if self.session().is_some_and(|session| session.is_frozen()) {
            return Err("Your account is frozen and can't send messages.".into());
        }
        if !chat.supported() {
            return Err("This chat type is not supported yet.".into());
        }
        if !chat.can_post() {
            return Err("You can't send messages here.".into());
        }
        let limit = self.text_length_limit();
        if quill::text_split::units_over_limit(&markup, limit) > 0 {
            return Err("This message is too long.".into());
        }
        self.remember_sent_hashtags(&markup);
        if let Some(live) = self.live.as_mut() {
            let generation = live.driver.session.view_generation;
            let snapshot = ComposerSnapshot::capture(chat_id, generation, markup);
            live.driver
                .send_snapshot(&snapshot)
                .map_err(|err| format!("Couldn't send the message ({err:?})."))?;
        } else if self.demo_session.is_some() {
            self.apply_demo_outgoing_to(chat_id, &markup, None, None);
        }
        // The chat's cloud draft was this text: clear it now.
        self.save_chat_draft(chat_id, DraftTopic::Chat, "", None, false, cx);
        cx.notify();
        Ok(())
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
        let total = quill::tray::badge_count(session, &session.settings.badge_prefs);
        let chat = session.chats.get(&chat_id.0).map(|chat| TitleChat {
            name: &chat.title,
            unread: chat.unread_count.max(0) as u32,
        });
        window_title(chat, total, None)
    }
}
