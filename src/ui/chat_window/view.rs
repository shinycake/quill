//! The chat window's view: what is per window (chat, history view state,
//! compose field), the swap that lets `QuillApp`'s conversation code draw
//! it, and the window itself.

use super::super::app::QuillApp;
use super::super::history_state::HistoryUi;
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::component::{Root, StyledExt, TitleBar};
use gpui_kit::*;
use quill::composer::{DraftTopic, enter_event_from_kit, send_text_on_enter, should_send_on_enter};
use quill::ids::ChatId;
use std::cell::RefCell;
use std::rc::Rc;

/// One chat window's view. `QuillApp` builds the body (header, history,
/// compose field) with this view's state swapped in; see [`WindowScope`].
pub(crate) struct ChatWindowView {
    owner: WeakEntity<QuillApp>,
    chat_id: ChatId,
    /// This window's history view state: swapped into `QuillApp::history`
    /// while the window is built, laid out or painted.
    history: Rc<RefCell<HistoryUi>>,
    pub(super) composer: Entity<TextareaState>,
    /// Why the last send did not go out, shown under the compose field.
    pub(super) note: Option<String>,
    /// The native title last handed to the platform.
    title_shown: String,
    /// The window's bookkeeping ran (the app knows the window is gone).
    closed: bool,
    _observe: Subscription,
}

impl ChatWindowView {
    fn new(
        owner: Entity<QuillApp>,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (send_key_mode, draft) = owner.read_with(cx, |app, _| {
            let draft = app.session().and_then(|session| {
                if !session.accepts_composer_draft(chat_id, DraftTopic::Chat) {
                    return None;
                }
                session
                    .draft_for(chat_id, DraftTopic::Chat)
                    .map(|draft| draft.text.clone())
            });
            (app.chat_prefs.send_key_mode, draft)
        });
        let submit_on_enter = send_key_mode == quill::composer::SendKeyMode::Enter;
        let composer = cx.new(|cx| {
            let mut state = TextareaState::new(window, cx)
                .placeholder("Write a message...")
                .auto_grow(1, 8)
                .submit_on_enter(submit_on_enter);
            state.set_span_styler(Some(super::super::composer_field::span_styler()), cx);
            if let Some(draft) = draft {
                let doc = quill::composer_doc::ComposerDoc::from_markup(&draft);
                state.set_value(super::super::composer_field::doc_content(&doc), window, cx);
            }
            state
        });
        cx.subscribe_in(
            &composer,
            window,
            move |this, state, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { secondary, shift } => {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if should_send_on_enter(
                        enter_event_from_kit(*shift, *secondary, marked),
                        send_key_mode,
                    ) {
                        this.send(window, cx);
                    }
                }
                InputEvent::Change => {
                    this.note = None;
                    this.save_draft(true, cx);
                    cx.notify();
                }
                _ => {}
            },
        )
        .detach();
        let history = Rc::new(RefCell::new(HistoryUi::new(cx)));
        let observe = cx.observe(&owner, |_, _, cx| cx.notify());
        Self {
            owner: owner.downgrade(),
            chat_id,
            history,
            composer,
            note: None,
            title_shown: String::new(),
            closed: false,
            _observe: observe,
        }
    }

    #[cfg(all(test, feature = "demo-capture"))]
    pub(crate) fn chat_id(&self) -> ChatId {
        self.chat_id
    }

    /// The compose field's content as markup (formatting kept).
    fn markup(&self, cx: &App) -> String {
        super::super::composer_field::field_doc(self.composer.read(cx)).to_markup()
    }

    /// Send what is typed to this window's chat and clear the field.
    pub(crate) fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let markup = send_text_on_enter(
            self.markup(cx),
            self.owner
                .read_with(cx, |app, _| app.chat_prefs.send_key_mode)
                .unwrap_or_default(),
        );
        if markup.trim().is_empty() {
            return;
        }
        let chat_id = self.chat_id;
        let sent = self
            .owner
            .update(cx, |app, cx| app.send_text_to_chat(chat_id, markup, cx))
            .unwrap_or_else(|_| Err("The app is gone.".into()));
        match sent {
            Ok(()) => {
                self.composer
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.note = None;
            }
            Err(note) => self.note = Some(note),
        }
        cx.notify();
    }

    /// The field's text as the chat's draft (`delayed`: tdesktop's 1 s /
    /// 5 s save timer; `false` flushes, on close).
    fn save_draft(&mut self, delayed: bool, cx: &mut Context<Self>) {
        let markup = self.markup(cx);
        let chat_id = self.chat_id;
        let _ = self.owner.update(cx, |app, cx| {
            app.save_chat_draft(chat_id, DraftTopic::Chat, &markup, None, delayed, cx);
        });
    }

    /// The window is going away: flush the draft and tell the app.
    fn closing(&mut self, cx: &mut Context<Self>) {
        if self.closed {
            return;
        }
        self.closed = true;
        self.save_draft(false, cx);
        let chat_id = self.chat_id;
        let _ = self
            .owner
            .update(cx, |app, cx| app.chat_window_closed(chat_id, cx));
    }

    /// Close the window from inside it (title bar close, Cmd/Ctrl+W).
    pub(crate) fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.closing(cx);
        window.remove_window();
    }

    /// Swap this window's history view state into the app and name the
    /// chat as the one being viewed. Paired with [`Self::exit_scope`];
    /// nothing may hold the app between the two.
    fn enter_scope(
        app: &WeakEntity<QuillApp>,
        chat_id: ChatId,
        history: &Rc<RefCell<HistoryUi>>,
        cx: &mut App,
    ) {
        let _ = app.update(cx, |app, _| {
            std::mem::swap(&mut app.history, &mut *history.borrow_mut());
            app.windows.scope = Some(chat_id);
        });
    }

    fn exit_scope(app: &WeakEntity<QuillApp>, history: &Rc<RefCell<HistoryUi>>, cx: &mut App) {
        let _ = app.update(cx, |app, _| {
            std::mem::swap(&mut app.history, &mut *history.borrow_mut());
            app.windows.scope = None;
        });
    }
}

impl Render for ChatWindowView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(app) = self.owner.upgrade() else {
            window.remove_window();
            return div().into_any_element();
        };
        let chat_id = self.chat_id;
        // The chat or the session went away: the window goes with it
        // (Telegram Desktop `closeChatFromWindows`, and a separate window
        // closes when its peer becomes unavailable).
        let (alive, title) = app.read_with(cx, |app, _| {
            let alive = app.pane_mode() == super::super::app::PaneMode::Ready
                && app
                    .session()
                    .is_some_and(|session| session.chats.contains_key(&chat_id.0));
            (alive, app.chat_window_title(chat_id))
        });
        if !alive {
            self.closing(cx);
            let handle = window.window_handle();
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            });
            return div().size_full().into_any_element();
        }
        if self.title_shown != title {
            window.set_window_title(&title);
            self.title_shown = title;
        }
        let composer = self.composer.clone();
        let note = self.note.clone();
        let view = cx.weak_entity();
        let weak_app = self.owner.clone();
        Self::enter_scope(&weak_app, chat_id, &self.history, cx);
        let body = app.update(cx, |app, cx| {
            app.chat_window_body(chat_id, &composer, view.clone(), note, window, cx)
        });
        Self::exit_scope(&weak_app, &self.history, cx);
        let title_bar = TitleBar::new()
            .on_close_window({
                let view = view.clone();
                move |_, window, cx| {
                    let _ = view.update(cx, |this, cx| this.close(window, cx));
                }
            })
            .child(
                div()
                    .font_semibold()
                    .text_sm()
                    .truncate()
                    .child(self.title_shown.clone()),
            );
        div()
            .id("chat-window")
            .size_full()
            .flex()
            .flex_col()
            .on_action({
                let view = view.clone();
                move |_: &super::super::actions::CloseWindow, window, cx| {
                    let _ = view.update(cx, |this, cx| this.close(window, cx));
                }
            })
            .child(title_bar)
            .child(WindowScope {
                app: weak_app,
                chat_id,
                history: self.history.clone(),
                child: body,
            })
            .into_any_element()
    }
}

/// Marks the window body's layout and paint as the chat window's: the
/// history rows built lazily inside the message scroller read the window's
/// `HistoryUi` and `QuillApp::viewed_chat` resolves to the window's chat.
/// The same idea as `app_slice::SliceScope`, with the view state swapped
/// as well as the scope named.
struct WindowScope {
    app: WeakEntity<QuillApp>,
    chat_id: ChatId,
    history: Rc<RefCell<HistoryUi>>,
    child: AnyElement,
}

impl WindowScope {
    fn scoped<R>(&self, cx: &mut App, f: impl FnOnce(&mut App) -> R) -> R {
        ChatWindowView::enter_scope(&self.app, self.chat_id, &self.history, cx);
        let result = f(cx);
        ChatWindowView::exit_scope(&self.app, &self.history, cx);
        result
    }
}

impl IntoElement for WindowScope {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for WindowScope {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut child = std::mem::replace(&mut self.child, Empty.into_any_element());
        let layout_id = self.scoped(cx, |cx| child.request_layout(window, cx));
        self.child = child;
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let mut child = std::mem::replace(&mut self.child, Empty.into_any_element());
        self.scoped(cx, |cx| child.prepaint(window, cx));
        self.child = child;
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let mut child = std::mem::replace(&mut self.child, Empty.into_any_element());
        self.scoped(cx, |cx| child.paint(window, cx));
        self.child = child;
    }
}

/// Open the window for `chat_id` at `bounds`. The kit `Root` hosts the
/// window's own dialog and notification layers; the title bar is Quill's
/// (kit `TitleBar`, like the main window's).
pub(super) fn open_chat_window(
    owner: Entity<QuillApp>,
    chat_id: ChatId,
    bounds: Bounds<Pixels>,
    cx: &mut App,
) -> Result<(AnyWindowHandle, WeakEntity<ChatWindowView>), String> {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(size(px(380.), px(400.))),
        titlebar: Some(TitlebarOptions {
            title: Some("Quill".into()),
            ..TitleBar::title_bar_options()
        }),
        app_owns_titlebar_drag: true,
        app_id: Some("org.shinycake.quill".into()),
        focus: true,
        show: true,
        ..Default::default()
    };
    let view_slot: Rc<RefCell<Option<WeakEntity<ChatWindowView>>>> = Rc::default();
    let view_out = view_slot.clone();
    let handle = cx.open_window(options, move |window, cx| {
        let view = cx.new(|cx| ChatWindowView::new(owner, chat_id, window, cx));
        *view_out.borrow_mut() = Some(view.downgrade());
        let composer = view.read(cx).composer.clone();
        composer.update(cx, |input, cx| input.focus(window, cx));
        let weak = view.downgrade();
        window.on_window_should_close(cx, move |_, cx| {
            let _ = weak.update(cx, |this, cx| this.closing(cx));
            true
        });
        window.activate_window();
        cx.new(|cx| Root::new(view, window, cx))
    });
    let handle = handle.map_err(|err| err.to_string())?;
    let view = view_slot
        .borrow_mut()
        .take()
        .ok_or_else(|| "the window built no view".to_string())?;
    Ok((handle.into(), view))
}
