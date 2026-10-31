//! The chat window's view: the window itself, its slot, the scope around
//! its body's layout and paint, and its kit dialog layer.

use super::super::app::QuillApp;
use super::super::command_shortcuts::CommandActions;
use super::super::frame_clock::follow_app;
use super::super::image_budget::{BoundedImageCache, CacheScope};
use super::super::shell::QuillShell;
use super::cover::WindowPlace;
use super::slot::{SharedSlot, WindowSlot, scope_probe};
use gpui_kit::component::{Root, StyledExt, TitleBar};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::ChatId;
use std::cell::RefCell;
use std::rc::Rc;

/// One chat window's view. `QuillApp` builds the body with this window's
/// slot swapped in; see `slot` and [`WindowScope`].
pub(crate) struct ChatWindowView {
    owner: WeakEntity<QuillApp>,
    chat_id: ChatId,
    pub(super) slot: SharedSlot,
    /// Hosts this window's kit dialog (the same registry as the main
    /// window's shell; the dialog opens where the input asked for it).
    pub(super) shell: Entity<QuillShell>,
    /// The native title last handed to the platform.
    title_shown: String,
    /// The geometry at the last render; a change is saved for next time.
    bounds_seen: Option<Bounds<Pixels>>,
    /// The window's bookkeeping ran (the app knows the window is gone).
    closed: bool,
    /// The pictures this window shows (`image_budget`), counted by its own
    /// frames; they go with the window.
    pub(super) images: Entity<BoundedImageCache>,
    _observe: Subscription,
    /// The window's activation, bounds and display (`cover`).
    _placed: [Subscription; 3],
}

impl ChatWindowView {
    fn new(
        owner: Entity<QuillApp>,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let slot = owner.update(cx, |app, cx| WindowSlot::new(app, window, cx));
        let shell = cx.new(|_| QuillShell::new(owner.clone()));
        let observe = follow_app(&owner, cx);
        let placed = [
            cx.observe_window_activation(window, Self::placed),
            cx.observe_window_bounds(window, Self::placed),
            cx.observe_window_display(window, Self::placed),
        ];
        Self {
            owner: owner.downgrade(),
            chat_id,
            slot: Rc::new(RefCell::new(slot)),
            shell,
            title_shown: String::new(),
            bounds_seen: None,
            closed: false,
            images: cx.new(|_| BoundedImageCache::for_window()),
            _observe: observe,
            _placed: placed,
        }
    }

    /// The window was activated or deactivated, moved, resized or went to
    /// another display: the app learns it at once, whether or not the
    /// window draws (a minimized one may not).
    fn placed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (chat_id, active) = (self.chat_id, window.is_window_active());
        let place = WindowPlace::of(window, window.display(cx).map(|display| display.id()));
        let _ = self.owner.update(cx, |app, cx| {
            app.chat_window_placed(chat_id, active, place, cx);
        });
    }

    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(crate) fn chat_id(&self) -> ChatId {
        self.chat_id
    }

    /// The window is going away: flush the draft and tell the app.
    fn closing(&mut self, reopenable: bool, cx: &mut Context<Self>) {
        if self.closed {
            return;
        }
        self.closed = true;
        let chat_id = self.chat_id;
        // Where the window stood, for "Reopen closed window".
        let bounds = self.bounds_seen.filter(|_| reopenable);
        let _ = self.owner.update(cx, |app, cx| {
            app.in_window_scope(Some(chat_id), cx, |app, cx| app.flush_leaving_draft(cx));
            app.chat_window_closed(chat_id, bounds, cx);
        });
    }

    /// Close the window from inside it (title bar close, Cmd/Ctrl+W).
    pub(crate) fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.closing(true, cx);
        window.remove_window();
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
        // closes when its peer becomes unavailable). So does a window the
        // session keeps no view for: it could only show another window's.
        let (alive, title) = app.read_with(cx, |app, _| {
            let alive = app.pane_mode() == super::super::app::PaneMode::Ready
                && app.session().is_some_and(|session| {
                    session.chats.contains_key(&chat_id.0)
                        && session.window_chats.contains(&chat_id)
                });
            (alive, app.chat_window_title(chat_id))
        });
        if !alive {
            self.closing(false, cx);
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
        let active = window.is_window_active();
        let bounds = window.bounds();
        let moved = self.bounds_seen.replace(bounds) != Some(bounds);
        let place = WindowPlace::of(window, window.display(cx).map(|display| display.id()));
        // A window may draw in the middle of another window's input: the
        // scope in place then is restored afterwards.
        let outer = app.update(cx, |app, cx| {
            app.windows.note_place(chat_id, active, place);
            app.set_window_chat_active(chat_id, active);
            if moved {
                app.schedule_chat_windows_save(cx);
            }
            app.sticker_frame_starts(Some(chat_id), cx);
            let outer = app.windows.scope;
            app.set_window_scope(Some(chat_id));
            // Its body builds the history every frame (no cached slice):
            // a frame without a history pass shows none of its clips.
            app.inline_videos_frame_start(true, window, cx);
            outer
        });
        // The kit dialog this window hosts, built with its state in place.
        self.shell.update(cx, |shell, cx| {
            shell.sync_kit_dialogs_in(Some(chat_id), window, cx)
        });
        let dialog_open = self.shell.read(cx).dialog_open();
        self.images
            .update(cx, |images, cx| images.begin_window_frame(window, cx));
        let body = app.update(cx, |app, cx| {
            app.release_viewer_key_probe(window, cx);
            app.viewer_follow_window_fullscreen(window);
            app.seed_history_width(f32::from(window.viewport_size().width));
            app.adopt_remote_draft(window, cx);
            app.schedule_idle_image_trim(cx);
            app.chat_window_body(chat_id, window, cx)
        });
        app.update(cx, |app, _| app.set_window_scope(outer));
        let view = cx.weak_entity();
        super::super::window_frame::sync(window, app.read(cx).appearance.native_window_frame);
        let title_bar = super::super::window_frame::FrameTitleBar::new()
            .on_close_window({
                let view = view.clone();
                move |_, window, cx| {
                    let _ = view.update(cx, |this, cx| this.close(window, cx));
                }
            })
            .caption(
                div()
                    .font_semibold()
                    .text_sm()
                    .truncate()
                    .child(self.title_shown.clone()),
            );
        div()
            .id("chat-window")
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .on_action({
                let view = view.clone();
                move |_: &super::super::actions::CloseWindow, window, cx| {
                    let _ = view.update(cx, |this, cx| this.close(window, cx));
                }
            })
            .chat_window_command_actions(self.owner.clone(), chat_id)
            // The viewer's Space release (a tap or the end of a hold), as on
            // the main root; key-downs reach it through the interceptor.
            .capture_key_up({
                let app = self.owner.clone();
                move |event: &KeyUpEvent, _, cx| {
                    let handled = app
                        .update(cx, |app, cx| {
                            app.note_window_input(Some(chat_id));
                            app.viewer_key_up(&event.keystroke.key, cx)
                        })
                        .unwrap_or(false);
                    if handled {
                        cx.stop_propagation();
                    }
                }
            })
            // Window-wide text selection, as in the main window.
            .child(gpui_kit::base::TextSelectionLayer)
            .child(scope_probe(self.owner.clone(), Some(chat_id)))
            .child(title_bar)
            .child(WindowScope {
                app: self.owner.clone(),
                target: Some(chat_id),
                child: CacheScope::new(self.images.clone(), body).into_any_element(),
            })
            .when(dialog_open, |this| {
                // Kit dialog contents paint at priority 10; occlude the
                // body right below them (as the main shell does).
                this.child(
                    deferred(
                        anchored().position(point(px(0.), px(0.))).child(
                            div()
                                .id("chat-window-dialog-barrier")
                                .w(window.viewport_size().width)
                                .h(window.viewport_size().height)
                                .occlude(),
                        ),
                    )
                    .with_priority(9),
                )
            })
            .into_any_element()
    }
}

/// Marks a window body's layout and paint as `target`'s window's
/// (`None`: the main window's): rows the message scroller builds lazily,
/// and everything else that renders during layout or paint, read that
/// window's state. The scope in place before each phase is restored after
/// it, since a window may draw in the middle of another window's input.
pub(in crate::ui) struct WindowScope {
    app: WeakEntity<QuillApp>,
    target: Option<ChatId>,
    child: AnyElement,
}

impl WindowScope {
    /// Wrap a window's content.
    pub(in crate::ui) fn new(
        app: WeakEntity<QuillApp>,
        target: Option<ChatId>,
        child: AnyElement,
    ) -> Self {
        Self { app, target, child }
    }

    fn scoped<R>(&self, cx: &mut App, f: impl FnOnce(&mut App) -> R) -> R {
        let target = self.target;
        let outer = self
            .app
            .update(cx, |app, _| {
                let outer = app.windows.scope;
                app.set_window_scope(target);
                outer
            })
            .unwrap_or(None);
        let result = f(cx);
        let _ = self.app.update(cx, |app, _| app.set_window_scope(outer));
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
/// (kit `TitleBar`, like the main window's). Returns the window, its view
/// and its slot; the caller registers them before the window first draws.
pub(super) fn open_chat_window(
    owner: Entity<QuillApp>,
    chat_id: ChatId,
    bounds: Bounds<Pixels>,
    cx: &mut App,
) -> Result<(AnyWindowHandle, WeakEntity<ChatWindowView>, SharedSlot), String> {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(size(px(380.), px(400.))),
        titlebar: Some(TitlebarOptions {
            title: Some("Quill".into()),
            appears_transparent: super::super::window_frame::titlebar_hidden(),
            ..TitleBar::title_bar_options()
        }),
        app_owns_titlebar_drag: true,
        window_decorations: super::super::window_frame::window_decorations(),
        app_id: Some("org.shinycake.quill".into()),
        focus: true,
        show: true,
        inactive_frame_interval: Some(super::super::window_frame::INACTIVE_FRAME_INTERVAL),
        ..Default::default()
    };
    let built: Rc<RefCell<Option<(WeakEntity<ChatWindowView>, SharedSlot)>>> = Rc::default();
    let built_out = built.clone();
    let handle = cx.open_window(options, move |window, cx| {
        let view = cx.new(|cx| ChatWindowView::new(owner, chat_id, window, cx));
        *built_out.borrow_mut() = Some((view.downgrade(), view.read(cx).slot.clone()));
        let weak = view.downgrade();
        window.on_window_should_close(cx, move |_, cx| {
            let _ = weak.update(cx, |this, cx| this.closing(true, cx));
            true
        });
        window.activate_window();
        cx.new(|cx| Root::new(view, window, cx))
    });
    let handle = handle.map_err(|err| err.to_string())?;
    let (view, slot) = built
        .borrow_mut()
        .take()
        .ok_or_else(|| "the window built no view".to_string())?;
    Ok((handle.into(), view, slot))
}
