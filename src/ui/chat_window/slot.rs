//! A chat window's slot: everything that is per window, and the swap that
//! puts it in place while that window is drawn or handles input.
//!
//! `QuillApp` keeps one copy of the view state its conversation, composer
//! and overlay code reads (`history`, `composer`, `composer_ui`,
//! `message_ui`, `pickers`, `viewer`) and the `Session` keeps one view
//! (`open_chat`, `open_topic`, the thread, Saved Messages and search
//! views, the gallery). A chat window owns its own copy of each in a
//! [`WindowSlot`]; [`QuillApp::set_window_scope`] exchanges the window's
//! copy with the app's, so the same code then reads and writes the
//! window's state. The scope is entered when the window is built, laid
//! out and painted (`view::WindowScope`) and on every input event in it
//! ([`scope_probe`] for the pointer, the keystroke interceptor in
//! `ChatWindows::new` for keys), and left at the main window's render,
//! its input, and the poll loop.

use super::super::app::QuillApp;
use super::super::composer_state::ComposerUi;
use super::super::history_state::HistoryUi;
use super::super::message_state::MessageUi;
use super::super::pickers_state::PickerUi;
use super::super::share_state::ShareUi;
use super::super::viewer_state::ViewerUi;
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::*;
use quill::ids::ChatId;
use quill::state::SessionView;
use std::cell::RefCell;
use std::rc::Rc;

/// One chat window's view state.
pub(super) struct WindowSlot {
    pub(super) history: HistoryUi,
    pub(super) composer: Entity<TextareaState>,
    pub(super) composer_ui: ComposerUi,
    pub(super) message_ui: MessageUi,
    pub(super) pickers: PickerUi,
    pub(super) viewer: ViewerUi,
    /// The window's own "Search in chat" field (`SearchUi::chat_input`).
    pub(super) chat_search_input: Entity<TextareaState>,
    /// The window's own share box, forward selection and picker field.
    pub(super) share: ShareUi,
    pub(super) session: SessionView,
    /// The window was the active one at its last render.
    pub(super) active: bool,
}

pub(super) type SharedSlot = Rc<RefCell<WindowSlot>>;

impl WindowSlot {
    /// A fresh slot for `chat_id`, with a compose field wired like the
    /// main window's and its own picker search fields.
    pub(super) fn new(
        app: &QuillApp,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<QuillApp>,
    ) -> Self {
        let composer = QuillApp::new_composer_field(app.chat_prefs.send_key_mode, window, cx);
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
            composer_ui: ComposerUi::new(Vec::new()),
            message_ui: MessageUi::new(window, cx),
            pickers: PickerUi::new(window, cx, emoji_search_input, reaction_search_input),
            viewer: ViewerUi::new(&app.playback.audio.output()),
            chat_search_input: QuillApp::new_chat_search_field(window, cx),
            share: ShareUi::new(window, cx, forward_search_input),
            session: SessionView::of_chat(chat_id),
            active: false,
        }
    }
}

impl QuillApp {
    /// Put `target`'s window state in place (`None`: the main window's).
    /// A no-op when it already is.
    pub(in crate::ui) fn set_window_scope(&mut self, target: Option<ChatId>) {
        if self.windows.scope == target {
            return;
        }
        if let Some(current) = self.windows.scope.take()
            && let Some(slot) = self.windows.slot_for(current)
        {
            self.swap_slot(&slot);
        }
        if let Some(chat_id) = target
            && let Some(slot) = self.windows.slot_for(chat_id)
        {
            self.swap_slot(&slot);
            self.windows.scope = Some(chat_id);
        }
    }

    /// Exchange the app's view state with the slot's (symmetric).
    fn swap_slot(&mut self, slot: &SharedSlot) {
        let mut slot = slot.borrow_mut();
        std::mem::swap(&mut self.history, &mut slot.history);
        std::mem::swap(&mut self.composer, &mut slot.composer);
        std::mem::swap(&mut self.composer_ui, &mut slot.composer_ui);
        std::mem::swap(&mut self.message_ui, &mut slot.message_ui);
        std::mem::swap(&mut self.pickers, &mut slot.pickers);
        std::mem::swap(&mut self.viewer, &mut slot.viewer);
        std::mem::swap(&mut self.search_ui.chat_input, &mut slot.chat_search_input);
        std::mem::swap(&mut self.share, &mut slot.share);
        if let Some(live) = self.live.as_mut() {
            live.driver.session.swap_view(&mut slot.session);
        } else if let Some(session) = self.demo_session.as_mut() {
            session.swap_view(&mut slot.session);
        }
    }

    /// Run `f` as `window_chat`'s window (`None`: the main window), for
    /// work deferred from a window's render, then restore the scope that
    /// was in place.
    pub(in crate::ui) fn in_window_scope<R>(
        &mut self,
        window_chat: Option<ChatId>,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Self, &mut Context<Self>) -> R,
    ) -> R {
        let outer = self.windows.scope;
        self.set_window_scope(window_chat);
        let result = f(self, cx);
        self.set_window_scope(outer);
        result
    }

    /// An input event started in `chat`'s window (`None`: the main
    /// window): its state goes in place, and dialogs opened by what the
    /// event does belong to that window.
    pub(super) fn note_window_input(&mut self, chat: Option<ChatId>) {
        self.windows.last_input = chat;
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
            window.on_mouse_event(move |_: &ScrollWheelEvent, phase, _, cx| {
                if phase == DispatchPhase::Capture {
                    note(cx);
                }
            });
        },
    )
    .absolute()
    .size_0()
}
