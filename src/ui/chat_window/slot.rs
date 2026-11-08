//! A chat window's slot: everything that is per window, and the swap that
//! puts it in place while that window is drawn or handles input.
//!
//! `QuillApp` keeps one copy of the view state its conversation, composer
//! and overlay code reads (`history`, `composer`, `composer_ui`,
//! `chat_ui`, `message_ui`, `pickers`, `viewer`, and the slices that draw them,
//! `slices.window`) and the `Session` keeps one view
//! (`open_chat`, `open_topic`, the thread, Saved Messages and search
//! views, the gallery). A chat window owns its own copy of the app's part
//! in a [`WindowSlot`] and the session keeps its view by chat
//! (`Session::window_chats`); [`QuillApp::set_window_scope`] exchanges the
//! window's copies with the ones in place, so the same code then reads and
//! writes the window's state. The scope is entered when the window is
//! built, laid out and painted (`view::WindowScope`) and on every input
//! event in it ([`scope_probe`] for the pointer, the keystroke interceptor
//! in `ChatWindows::new` for keys), and left at the main window's render,
//! its input, and the poll loop. Work that finishes later carries the
//! window it started in (`scoped`); TDLib answers carry it on their
//! request (`Session::enter_view`).

use super::super::app::QuillApp;
use super::super::app_slice::WindowSlices;
use super::super::chat_ui_state::ChatUi;
use super::super::composer_state::ComposerUi;
use super::super::glyph_sweep;
use super::super::history_state::HistoryUi;
use super::super::image_budget::{BoundedImageCache, WindowDraw};
use super::super::media_viewer::PipLink;
use super::super::message_state::MessageUi;
use super::super::passcode::LockField;
use super::super::pickers_state::PickerUi;
use super::super::share_state::ShareUi;
use super::super::sticker_player;
use super::super::viewer_state::ViewerUi;
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// What one field of a chat window's state says when the window closes,
/// run with that state in place (`QuillApp::farewell_window`): the viewer
/// closes its Picture-in-Picture window. Most fields say nothing. The
/// window's own leave (`forget_chat_window`: the recording, the TDLib
/// chat close) is the window's, not a field's.
pub(in crate::ui) trait SlotFarewell {
    fn farewell(_app: &mut QuillApp, _cx: &mut Context<QuillApp>) {}
}

impl<T: SlotFarewell> SlotFarewell for Box<T> {
    fn farewell(app: &mut QuillApp, cx: &mut Context<QuillApp>) {
        T::farewell(app, cx);
    }
}

impl SlotFarewell for HistoryUi {}
impl SlotFarewell for Entity<TextareaState> {}
impl SlotFarewell for ComposerUi {}
impl SlotFarewell for ChatUi {}
impl SlotFarewell for MessageUi {}
impl SlotFarewell for PickerUi {}
impl SlotFarewell for ShareUi {}
impl SlotFarewell for LockField {}

/// The Picture-in-Picture window opened from this window's viewer plays
/// the window's clip, with its link in this slot: it closes with it.
impl SlotFarewell for ViewerUi {
    fn farewell(app: &mut QuillApp, cx: &mut Context<QuillApp>) {
        app.close_video_pip(cx);
    }
}

super::super::on_screen::on_screen_fields! {
    /// One chat window's view state.
    pub(super) struct WindowSlot via SlotFarewell;
    /// What the window's state says as the window closes, with the
    /// window's state in place.
    fn farewell_window(&mut self, cx: &mut Context<Self>) => (self, cx);
    pub(super) slices: WindowSlices,
    /// The pictures the window shows (`image_budget`); they go with it.
    /// Stays here while the window's state is in place.
    pub(super) images: Entity<BoundedImageCache>,
    /// The frame `QuillApp::chat_frame_starts` last ran for.
    pub(super) frame_started: Cell<u64>,
    /// The window was the active one at its last render or activation
    /// change.
    pub(super) active: bool,
    /// Where the window stood then (`cover`).
    pub(super) place: Option<super::cover::WindowPlace>,
    ;
    history: HistoryUi,
    composer: Entity<TextareaState>,
    composer_ui: Box<ComposerUi>,
    /// What the window has open for its chat (`chat_ui_state`).
    chat_ui: Box<ChatUi>,
    message_ui: MessageUi,
    pickers: PickerUi,
    viewer: ViewerUi,
    /// The window's own "Search in chat" field (`SearchUi::chat_input`).
    chat_search_input: Entity<TextareaState>,
    /// The window's own share box, forward selection and picker field.
    share: ShareUi,
    /// The window's own passcode field on the lock screen
    /// (`PasscodeUi::lock`).
    lock_field: LockField,
}

pub(super) type SharedSlot = Rc<RefCell<WindowSlot>>;

impl WindowSlot {
    /// A fresh slot, with a compose field wired like the main window's and
    /// its own picker search fields.
    pub(super) fn new(
        app: &QuillApp,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<QuillApp>,
    ) -> Self {
        let composer = QuillApp::new_composer_field(app.chat_prefs.send_key_mode, window, cx);
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
        for input in [&emoji_search_input, &reaction_search_input] {
            cx.subscribe(input, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            })
            .detach();
        }
        let forward_search_input = QuillApp::new_forward_search_field(window, cx);
        Self {
            history: HistoryUi::new(cx),
            composer,
            composer_ui: Box::new(ComposerUi::new()),
            chat_ui: Box::new(ChatUi::for_chat(Some(chat_id))),
            message_ui: MessageUi::new(window, cx),
            pickers: PickerUi::new(window, cx, emoji_search_input, reaction_search_input),
            viewer: ViewerUi::new(),
            chat_search_input: QuillApp::new_chat_search_field(window, cx),
            share: ShareUi::new(window, cx, forward_search_input),
            lock_field: LockField::new(window, cx, app.account.passcode.locked),
            slices: WindowSlices::new(Some(chat_id), cx)
                .with_composer_height(app.slices.composer_height()),
            images: cx.new(|_| BoundedImageCache::for_window(chat_id, app.slices.scope())),
            frame_started: Cell::new(0),
            active: false,
            place: None,
        }
    }
}

impl QuillApp {
    /// Put `target`'s window state in place (`None`: the main window's).
    /// A no-op when it already is. A chat with no window, or a window
    /// whose session view is gone (the session records a diagnostic, and
    /// the window closes at its next render), puts the main window's state
    /// in place: a window never shows its own slot over another view.
    /// Returns whether `target`'s state is in place.
    pub(in crate::ui) fn set_window_scope(&mut self, target: Option<ChatId>) -> bool {
        if self.windows.scope == target {
            return true;
        }
        if let Some(current) = self.windows.scope.take()
            && let Some(slot) = self.windows.slot_for(current)
        {
            self.swap_slot(&slot);
        }
        let slot = target.and_then(|chat_id| Some((chat_id, self.windows.slot_for(chat_id)?)));
        let view = slot.as_ref().map(|(chat_id, _)| *chat_id);
        if self.enter_session_view(view)
            && let Some((chat_id, slot)) = slot
        {
            self.swap_slot(&slot);
            self.windows.scope = Some(chat_id);
        }
        self.windows.scope == target
    }

    /// Put `target`'s view in the session's place. `false` when the
    /// session has no view for it (the main window's is in place then).
    fn enter_session_view(&mut self, target: Option<ChatId>) -> bool {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.enter_view(target)
        } else if let Some(session) = self.demo_session.as_mut() {
            session.enter_view(target)
        } else {
            true
        }
    }

    /// Exchange the app's view state with the slot's (symmetric).
    fn swap_slot(&mut self, slot: &SharedSlot) {
        let mut slot = slot.borrow_mut();
        std::mem::swap(&mut self.history, &mut slot.history);
        std::mem::swap(&mut self.composer, &mut slot.composer);
        std::mem::swap(&mut self.composer_ui, &mut slot.composer_ui);
        std::mem::swap(&mut self.screen.chat_ui, &mut slot.chat_ui);
        std::mem::swap(&mut self.message_ui, &mut slot.message_ui);
        std::mem::swap(&mut self.pickers, &mut slot.pickers);
        std::mem::swap(&mut self.viewer, &mut slot.viewer);
        std::mem::swap(&mut self.search_ui.chat_input, &mut slot.chat_search_input);
        std::mem::swap(&mut self.share, &mut slot.share);
        std::mem::swap(&mut self.account.passcode.lock, &mut slot.lock_field);
        std::mem::swap(&mut self.slices.window, &mut slot.slices);
    }

    /// Run `f` as `window_chat`'s window (`None`: the main window), for
    /// work deferred from a window's render or finishing later, then
    /// restore the scope that was in place. `None`, without running `f`,
    /// when that window has closed (or lost its session view): its work
    /// must not land in the state of whichever window is in place instead.
    pub(in crate::ui) fn in_window_scope<R>(
        &mut self,
        window_chat: Option<ChatId>,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Self, &mut Context<Self>) -> R,
    ) -> Option<R> {
        let outer = self.windows.scope;
        self.set_window_scope(window_chat);
        if self.windows.scope != window_chat {
            self.set_window_scope(outer);
            return None;
        }
        let result = f(self, cx);
        self.set_window_scope(outer);
        Some(result)
    }

    /// Run `f` once for every window with that window's state in place:
    /// the main window's, then each chat window's. The scope in place
    /// before is put back. For work that no frame or input of the window
    /// runs (a batch of session updates, a new spelling engine) and that
    /// each window's state needs.
    pub(in crate::ui) fn in_every_window(
        &mut self,
        cx: &mut Context<Self>,
        mut f: impl FnMut(&mut Self, &mut Context<Self>),
    ) {
        let chats: Vec<ChatId> = self.windows.open_windows().map(|(chat, _)| chat).collect();
        for window in std::iter::once(None).chain(chats.into_iter().map(Some)) {
            self.in_window_scope(window, cx, &mut f);
        }
    }

    /// The lock started: every window's passcode field, the one in place
    /// and each chat window slot's, empties and takes its window's focus
    /// at that window's next frame.
    pub(in crate::ui) fn reset_lock_fields(&mut self) {
        self.account.passcode.lock.reset();
        for slot in self.windows.slots() {
            if let Ok(mut slot) = slot.try_borrow_mut() {
                slot.lock_field.reset();
            }
        }
    }

    /// Take the Picture-in-Picture links `pick` chooses from every
    /// window's viewer: the one in place and each chat window slot's. A
    /// PiP opened from a chat window keeps its link in that window's
    /// slot, so the app's checks (the lock, a deletion, one sound at a
    /// time, a logout) look in every slot, whichever window is in place.
    pub(in crate::ui) fn take_video_pips(
        &mut self,
        cx: &App,
        pick: impl Fn(&Self, &PipLink, &App) -> bool,
    ) -> Vec<PipLink> {
        let mut taken = Vec::new();
        if self
            .viewer
            .pip
            .as_ref()
            .is_some_and(|link| pick(self, link, cx))
        {
            taken.extend(self.viewer.pip.take());
        }
        let slots: Vec<SharedSlot> = self.windows.slots().collect();
        for slot in slots {
            let picked = slot.try_borrow().is_ok_and(|slot| {
                slot.viewer
                    .pip
                    .as_ref()
                    .is_some_and(|link| pick(self, link, cx))
            });
            if picked && let Ok(mut slot) = slot.try_borrow_mut() {
                taken.extend(slot.viewer.pip.take());
            }
        }
        taken
    }

    /// What each frame of `chat`'s window starts with, whether its view
    /// renders or only its animation layers redraw, as `main_frame_starts`
    /// for the main window. The first call of a frame counts.
    pub(in crate::ui) fn chat_frame_starts(
        &mut self,
        chat: ChatId,
        draw: WindowDraw,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(slot) = self.windows.slot_for(chat) else {
            return;
        };
        let frame = window.frame_number();
        let (started, images) = {
            let slot = slot.borrow();
            (slot.frame_started.replace(frame), slot.images.clone())
        };
        if started == frame {
            return;
        }
        let display = window.display(cx).map(|display| u64::from(display.id()));
        sticker_player::note_display(Some(chat), display, window, cx);
        self.sticker_frame_starts(cx);
        images.update(cx, |images, cx| {
            images.begin_window_frame(draw, window, cx);
        });
        glyph_sweep::start_frame(window, cx);
        self.schedule_idle_image_trim(cx);
    }

    /// Clear the pending chrome notify on `chat_id`'s slot without putting
    /// the window in scope (it cannot be, or its state is not in place).
    pub(in crate::ui) fn clear_slot_chrome_notify(&self, chat_id: ChatId) {
        if let Some(slot) = self.windows.slot_for(chat_id)
            && let Ok(slot) = slot.try_borrow()
        {
            slot.history.chrome_notify_pending.set(false);
        }
    }

    /// An input event started in `chat`'s window (`None`: the main
    /// window): its state goes in place, and dialogs opened by what the
    /// event does belong to that window.
    pub(super) fn note_window_input(&mut self, chat: Option<ChatId>) {
        self.windows.last_input = chat;
        self.connection.note_window = chat;
        self.set_window_scope(chat);
    }
}

/// Painted first in a window's root: its capture-phase pointer listeners
/// run before any handler in the window, so the window's state is in
/// place for them (`QuillApp::note_window_input`). Draws nothing.
pub(in crate::ui) fn scope_probe(
    app: WeakEntity<QuillApp>,
    chat: Option<ChatId>,
) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |_, _, window, _| {
            let note = {
                let app = app.clone();
                move |cx: &mut App| {
                    let _ = app.update(cx, |app, _| app.note_window_input(chat));
                }
            };
            let on = note.clone();
            window.on_mouse_event(move |_: &MouseDownEvent, phase, _, cx| {
                if phase == DispatchPhase::Capture {
                    on(cx);
                }
            });
            let on = note.clone();
            window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                if phase == DispatchPhase::Capture {
                    on(cx);
                }
            });
            let on = note.clone();
            window.on_mouse_event(move |_: &MouseMoveEvent, phase, _, cx| {
                if phase == DispatchPhase::Capture {
                    on(cx);
                }
            });
            window.on_mouse_event(move |_: &ScrollEvent, phase, _, cx| {
                if phase == DispatchPhase::Capture {
                    note(cx);
                }
            });
        },
    )
    .absolute()
    .size_0()
}
