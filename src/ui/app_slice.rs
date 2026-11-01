//! Cached child views of the one big `QuillApp` view ("slices").
//!
//! `QuillApp` renders as a single GPUI view, so any `cx.notify()` used to
//! lay out and paint the whole window again — including the frame clock's
//! 30 fps tick for one animated custom emoji. A slice is a tiny entity
//! whose render borrows `QuillApp` and draws one part of it (the chat list,
//! the conversation); `QuillApp` embeds it with `Entity::cached`, so GPUI
//! replays the slice's previous frame unless the slice itself (or something
//! rendered inside it) was notified.
//!
//! A chat window hosts the same history widget, so it has its own
//! conversation and composer slices and their layers (`WindowSlices`,
//! which a window's slot holds), and the same code serves both.
//!
//! Invalidation contract:
//! - Every `QuillApp` notify notifies every slice of every window but a
//!   held one (`observe_self`), so a `cx.notify()` refreshes everything
//!   that can show.
//! - Animated content asks the frame clock for a tick from inside a slice;
//!   the tick notifies only that slice (`SliceScope` records which one is
//!   being laid out or painted), and the other slices replay.
//! - Views rendered inside a slice (inputs, images loading, hover state)
//!   dirty the slice through GPUI's ancestor walk; `window.refresh()`
//!   bypasses every cache.
//!
//! GPUI re-renders every ancestor of a dirty view, and a re-rendered cached
//! view re-renders its whole subtree, so a slice per chat row would not
//! help: one animated emoji would still rebuild the list. Each slice
//! therefore has an animation layer (`anim_layer`), drawn right after it:
//! animated emoji, stickers, inline videos, spoiler specks and typing dots
//! only report where they sit, and a tick redraws the layer alone
//! (`Entity::live`): `QuillApp` and every slice stay as the last whole frame
//! drew them.
#![deny(clippy::indexing_slicing)]

use super::anim_layer::{AnimationLayer, Layer};
use super::app::QuillApp;
use super::conversation::ConversationPart;
use gpui_kit::*;
use quill::ids::ChatId;
use std::cell::Cell;
use std::rc::Rc;

mod window_slices;
pub(super) use window_slices::WindowSlices;

/// Which part of `QuillApp` a slice draws.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum SliceKind {
    /// The chat list column (ready mode).
    Sidebar,
    /// Header and history of the open chat (with the composer when there
    /// is no composer slice).
    Conversation,
    /// The open chat's composer, its popups, or the note / channel footer
    /// in its place: typing and the caret's blink redraw only this, not
    /// the history above it.
    Composer,
    /// The media viewer's media box (thumbnail or photo, zoom, pan, its
    /// menu), under the playing picture (`media_viewer::layer`), so a new
    /// video frame replays it.
    Viewer,
    /// The media viewer's chrome over the playing picture: top bar,
    /// arrows, caption and controls.
    ViewerChrome,
    /// The story viewer overlay: its 100 ms playback tick redraws only
    /// this.
    Story,
}

pub(super) struct AppSlice {
    app: WeakEntity<QuillApp>,
    kind: SliceKind,
    /// The chat window it belongs to (`None`: the main window).
    window: Option<ChatId>,
}

impl AppSlice {
    fn new(app: WeakEntity<QuillApp>, kind: SliceKind, window: Option<ChatId>) -> Self {
        Self { app, kind, window }
    }
}

/// Draws nothing: a notify of this view re-renders `QuillApp`, its parent,
/// and no slice.
pub(super) struct AppOnly;

impl Render for AppOnly {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

/// A slice, its animation layer's content, and the view that draws it.
struct SliceParts {
    slice: Entity<AppSlice>,
    layer: Layer,
    painter: Entity<AnimationLayer>,
}

/// `QuillApp`'s slices and the frame-clock bookkeeping that targets them.
#[derive(Default)]
pub(super) struct Slices {
    sidebar: Option<SliceParts>,
    /// The slices of the window whose state is in place.
    pub(super) window: WindowSlices,
    /// The media viewer's media box and its chrome, while the viewer is
    /// open.
    viewer: Option<Entity<AppSlice>>,
    viewer_chrome: Option<Entity<AppSlice>>,
    /// The story viewer overlay, while it is open.
    story: Option<Entity<AppSlice>>,
    app_only: Option<Entity<AppOnly>>,
    /// The main window's views, whichever window's state is in place.
    main_views: Vec<EntityId>,
    /// The window's bounded path-image cache (`image_budget`).
    image_cache: Option<Entity<super::image_budget::BoundedImageCache>>,
    /// Waits for the image cache's idle trim (`image_budget::IDLE_AFTER`).
    idle_trim: Option<Task<()>>,
    /// The slice whose content is being rendered, laid out or painted, in
    /// any window (`None`: the window's own view).
    scope: Rc<Cell<Option<EntityId>>>,
    /// A screen reader (or another accessibility client) reads the window:
    /// history rows report the accessibility focus.
    pub(super) a11y_active: Cell<bool>,
}

impl Slices {
    /// The composer's height as last laid out.
    pub(super) fn composer_height(&self) -> Pixels {
        self.window.composer_height.get()
    }

    /// The slice whose content is being drawn now, if any.
    pub(super) fn current(&self) -> Option<EntityId> {
        self.scope.get()
    }

    /// The conversation slice and its animation layer's view, for tests.
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn conversation_ids(&self) -> Option<(EntityId, EntityId)> {
        self.window
            .conversation
            .as_ref()
            .map(|parts| (parts.slice.entity_id(), parts.painter.entity_id()))
    }

    /// What covers the conversation's animations, for tests.
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn conversation_occluders(&self) -> Vec<Bounds<Pixels>> {
        self.window
            .conversation
            .as_ref()
            .map(|parts| parts.layer.occluders())
            .unwrap_or_default()
    }

    /// Whether content being built now belongs to the chat list.
    pub(super) fn in_sidebar(&self) -> bool {
        self.in_slice(self.sidebar.as_ref())
    }

    /// Whether content being built now belongs to the conversation.
    pub(super) fn in_conversation(&self) -> bool {
        self.in_slice(self.window.conversation.as_ref())
    }

    fn in_slice(&self, parts: Option<&SliceParts>) -> bool {
        self.scope.get().is_some() && self.scope.get() == parts.map(|p| p.slice.entity_id())
    }

    pub(super) fn image_cache(&self) -> Option<Entity<super::image_budget::BoundedImageCache>> {
        self.image_cache.clone()
    }

    /// The cell naming the slice being drawn, which every window's image
    /// cache reads.
    pub(super) fn scope(&self) -> Rc<Cell<Option<EntityId>>> {
        self.scope.clone()
    }

    /// A layer painter's content, drawn on its own on a tick: what it paints
    /// counts as the render of `painter`, which replaces it at every frame.
    pub(super) fn painter_scope(
        &self,
        painter: EntityId,
        cache: Option<Entity<super::image_budget::BoundedImageCache>>,
        content: impl IntoElement,
    ) -> AnyElement {
        let content = SliceScope {
            id: Some(painter),
            scope: self.scope.clone(),
            layer: None,
            frames: None,
            child: content.into_any_element(),
        };
        match cache {
            Some(cache) => super::image_budget::CacheScope::new(cache, content).into_any_element(),
            None => content.into_any_element(),
        }
    }

    fn panel_layer(&self) -> Option<Layer> {
        self.window.panel.as_ref().map(|parts| parts.layer.clone())
    }

    pub(super) fn app_only(&self) -> Option<Entity<AppOnly>> {
        self.app_only.clone()
    }

    pub(super) fn app_only_id(&self) -> Option<EntityId> {
        self.app_only.as_ref().map(Entity::entity_id)
    }

    /// Whether `id` is a view of the main window: a slice, a layer's
    /// painter, or [`AppOnly`].
    pub(super) fn owns(&self, id: EntityId) -> bool {
        self.main_views.contains(&id)
    }
}

impl QuillApp {
    /// Create the long-lived slices; every `QuillApp` notify re-renders
    /// them all.
    pub(super) fn init_slices(&mut self, cx: &mut Context<Self>) {
        let app = cx.entity().downgrade();
        let layer = Layer::default();
        let sidebar = SliceParts {
            slice: cx.new(|_| AppSlice::new(app.clone(), SliceKind::Sidebar, None)),
            painter: cx.new(|_| AnimationLayer::new(layer.clone(), app.clone(), None)),
            layer,
        };
        self.slices.window = WindowSlices::new(None, cx);
        self.slices.viewer = Some(cx.new(|_| AppSlice::new(app.clone(), SliceKind::Viewer, None)));
        self.slices.viewer_chrome =
            Some(cx.new(|_| AppSlice::new(app.clone(), SliceKind::ViewerChrome, None)));
        self.slices.story = Some(cx.new(|_| AppSlice::new(app.clone(), SliceKind::Story, None)));
        self.slices.app_only = Some(cx.new(|_| AppOnly));
        self.slices.main_views = [sidebar.slice.entity_id(), sidebar.painter.entity_id()]
            .into_iter()
            .chain(self.slices.window.view_ids())
            .chain(self.slices.viewer.as_ref().map(Entity::entity_id))
            .chain(self.slices.viewer_chrome.as_ref().map(Entity::entity_id))
            .chain(self.slices.story.as_ref().map(Entity::entity_id))
            .chain(self.slices.app_only_id())
            .collect();
        self.slices.sidebar = Some(sidebar);
        let scope = self.slices.scope.clone();
        self.slices.image_cache =
            Some(cx.new(|_| super::image_budget::BoundedImageCache::new(scope)));
        cx.observe_self(|this, cx| this.notify_slices(cx)).detach();
    }

    fn notify_slices(&self, cx: &mut Context<Self>) {
        super::frame_clock::trace_slice_render("(app notified)");
        super::frame_clock::trace_notify_origin();
        let ids: Vec<EntityId> = self
            .slices
            .sidebar
            .iter()
            .map(|parts| parts.slice.entity_id())
            .chain(self.conversation_slices(cx))
            .chain(self.slices.viewer.iter().map(Entity::entity_id))
            .chain(self.slices.viewer_chrome.iter().map(Entity::entity_id))
            .chain(self.slices.story.iter().map(Entity::entity_id))
            .collect();
        let app: &mut App = cx;
        for id in ids {
            app.notify(id);
        }
    }

    /// Every window's conversation and composer slices: those in place,
    /// and those in the chat windows' slots, which hold the main window's
    /// while a chat window's are in place. A held chat window's stay out:
    /// it redraws them when it shows again (`follow_windows`).
    fn conversation_slices(&self, cx: &App) -> Vec<EntityId> {
        self.slices
            .window
            .slice_ids()
            .chain(self.windows.slice_ids())
            .filter(|id| !self.windows.holds_view(*id, cx))
            .collect()
    }

    /// Redraw what every chat window shows in its slices, for a tick of
    /// the whole app that leaves the main window out (`tick_app`).
    pub(super) fn notify_chat_window_slices(&self, cx: &mut Context<Self>) {
        let main = &self.slices.main_views;
        let ids = self.conversation_slices(cx);
        let app: &mut App = cx;
        for id in ids.into_iter().filter(|id| !main.contains(id)) {
            app.notify(id);
        }
    }

    /// Once nothing new has been shown for `image_budget::IDLE_AFTER`,
    /// trim the image cache to its idle budget, drop the wallpaper
    /// pictures no area shows and sweep the emoji glyphs off screen
    /// (`glyph_sweep`). One waiting task at most; it sleeps past
    /// every newer activity.
    pub(super) fn schedule_idle_image_trim(&mut self, cx: &mut Context<Self>) {
        if self.slices.idle_trim.is_some() || super::image_budget::idle_trim_due().is_none() {
            return;
        }
        let Some(cache) = self.slices.image_cache.clone() else {
            return;
        };
        self.slices.idle_trim = Some(cx.spawn(async move |this, cx| {
            while let Some(due) = super::image_budget::idle_trim_due() {
                let now = std::time::Instant::now();
                if due <= now {
                    let (main, background, windows, sweeping) = this
                        .read_with(cx, |this, _| {
                            (
                                this.windows.main,
                                !this.frame.window_active.get(),
                                this.windows.image_caches(),
                                this.windows
                                    .main
                                    .into_iter()
                                    .chain(this.windows.open_windows().map(|(_, handle)| handle))
                                    .collect::<Vec<_>>(),
                            )
                        })
                        .unwrap_or_default();
                    cache.update(cx, |cache, cx| {
                        cache.idle_trim(background, cx);
                        super::spoiler_fx::release_media_tile(None, cx);
                        if let Some(main) = main {
                            super::wallpaper_cache::release_unshown(main.window_id(), cx);
                        }
                    });
                    for (images, active) in windows {
                        images.update(cx, |images, cx| images.idle_trim(!active, cx));
                    }
                    cx.update(|cx| super::glyph_sweep::begin_for(&sweeping, cx));
                    break;
                }
                cx.background_executor().timer(due - now).await;
            }
            let _ = this.update(cx, |this, _| this.slices.idle_trim = None);
        }));
    }

    /// Redraw only the chat list (pinned drag follows the pointer).
    pub(super) fn notify_sidebar(&self, cx: &mut Context<Self>) {
        match &self.slices.sidebar {
            Some(slice) => {
                let id = slice.slice.entity_id();
                let app: &mut App = cx;
                app.notify(id);
            }
            None => cx.notify(),
        }
    }

    /// Redraw for TDLib updates that only change chat-list rows: the chat
    /// list slice, and `QuillApp` itself as its ancestor (the chrome,
    /// side panels and render-time drains), while the conversation and
    /// composer replay their last frame. Outside the ready layout the
    /// chat list is not a slice, so everything redraws.
    pub(super) fn notify_chat_list(&self, cx: &mut Context<Self>) {
        if self.pane_mode() == super::app::PaneMode::Ready {
            self.notify_sidebar(cx);
            // The Touch Bar's pinned chats show these rows' badges and
            // online dots, and it mirrors by itself only on an app notify.
            #[cfg(target_os = "macos")]
            super::touch_bar::request_sync(cx);
        } else {
            cx.notify();
        }
    }

    /// Redraw only the conversation (state that nothing else shows
    /// changed); the whole app before the slices exist.
    pub(super) fn notify_conversation(&self, cx: &mut Context<Self>) {
        let slice = self.slices.window.conversation.as_ref();
        self.redraw_view(slice.map(|parts| parts.slice.entity_id()), cx);
    }

    /// Redraw only the composer (state that only it shows changed, such
    /// as suggestions following the typed text); the whole app before the
    /// slices exist.
    pub(super) fn notify_composer(&self, cx: &mut Context<Self>) {
        let slice = self.slices.window.composer.as_ref();
        self.redraw_view(slice.map(Entity::entity_id), cx);
    }

    /// Redraw `view` of the window in place, unless that window is held:
    /// the whole app with no such view.
    fn redraw_view(&self, view: Option<EntityId>, cx: &mut Context<Self>) {
        match view {
            Some(view) if self.windows.holds_view(view, cx) => {}
            Some(view) => {
                let app: &mut App = cx;
                app.notify(view);
            }
            None => cx.notify(),
        }
    }

    /// Redraw what a timer advanced (a countdown) where it is shown: the
    /// `kind` slice of the window whose state is in place, while everything
    /// else replays. That whole window redraws while the conversation is
    /// not on screen as slices.
    pub(super) fn notify_tick(&self, kind: SliceKind, cx: &mut Context<Self>) {
        let shown = self.slices.window.conversation_shown();
        match kind {
            SliceKind::Conversation if shown => self.notify_conversation(cx),
            SliceKind::Composer if shown => self.notify_composer(cx),
            SliceKind::Story if shown && !self.windows.in_window() => self.notify_story(cx),
            _ => self.redraw_view(self.windows.view_in_place(), cx),
        }
    }

    /// Redraw only the story viewer overlay (its playback tick); the
    /// whole app before the slices exist.
    pub(super) fn notify_story(&self, cx: &mut Context<Self>) {
        match &self.slices.story {
            Some(slice) => {
                let id = slice.entity_id();
                let app: &mut App = cx;
                app.notify(id);
            }
            None => cx.notify(),
        }
    }

    /// Redraw the open chat in every window (a playing track's position):
    /// each window's conversation and composer slices.
    pub(super) fn notify_conversations(&self, cx: &mut Context<Self>) {
        if !self.slices.window.conversation_shown() || !self.windows.all_conversations_shown() {
            cx.notify();
            return;
        }
        let ids = self.conversation_slices(cx);
        let app: &mut App = cx;
        for id in ids {
            app.notify(id);
        }
    }

    /// The layer that plays the media panel's custom emoji, where the main
    /// window hosts it (as the composer's popover or as the selector).
    pub(super) fn panel_layer(&self) -> Option<Layer> {
        let hosted = self.pickers.media_panel.selector_anchor().is_some()
            || self.media_panel_over_composer();
        if !hosted {
            return None;
        }
        self.slices.panel_layer()
    }

    /// The view that draws [`Self::panel_layer`], embedded right after what
    /// hosts the panel.
    pub(super) fn panel_painter(&self) -> Option<AnyElement> {
        self.panel_layer()?;
        let parts = self.slices.window.panel.as_ref()?;
        Some(parts.painter.clone().live().into_any_element())
    }

    /// The selector's panel. No slice holds it, so its frame is the layer's
    /// frame.
    pub(super) fn selector_panel(&self, panel: AnyElement) -> AnyElement {
        SliceScope {
            id: self.slices.current(),
            scope: self.slices.scope.clone(),
            layer: None,
            frames: self.slices.panel_layer(),
            child: panel,
        }
        .into_any_element()
    }

    /// Keep a layer's content playing: a tick redraws the layer while its
    /// slice replays, or the slice itself for content something covers
    /// (drawn inline). The layer (`painter`) calls this after it painted,
    /// with its window's state in place. The content grid is the main
    /// window's alone.
    pub(super) fn tick_animation_layer(&self, painter: EntityId, cx: &mut Context<Self>) {
        let Some(parts) = [
            self.slices.sidebar.as_ref(),
            self.slices.window.conversation.as_ref(),
            self.slices.window.panel.as_ref(),
        ]
        .into_iter()
        .flatten()
        .find(|parts| parts.painter.entity_id() == painter) else {
            return;
        };
        let (layer, inline) = parts.layer.demand();
        let slice = parts.slice.entity_id();
        let main = !self.windows.in_window();
        if main {
            self.note_layer_drawn(painter);
        }
        if layer > 0 && (!main || self.layer_wants_tick(painter, layer)) {
            self.request_animation_tick_for(Some(painter), layer, cx);
        }
        if inline > 0 {
            self.request_animation_tick_for(Some(slice), inline, cx);
        }
    }

    /// The chat list, as a cached slice filling its laid-out column,
    /// with its layer painting the animated content just above it.
    pub(super) fn sidebar_slot(&self) -> AnyElement {
        let column = div()
            .id("sidebar-slot")
            .role(gpui_kit::Role::Group)
            .a11y_tag(quill::agent::ui_driver::redact::REGION_CHAT_LIST)
            .relative()
            .w(self.list_width() + px(self.folder_rail_width()))
            .flex_none()
            .h_full();
        match &self.slices.sidebar {
            Some(parts) => {
                super::image_budget::slice_shown(parts.slice.entity_id(), None);
                column
                    .child(parts.slice.clone().cached(full()))
                    .child(parts.painter.clone().live())
                    .into_any_element()
            }
            None => column.into_any_element(),
        }
    }

    /// The conversation, as a cached slice filling the remaining width,
    /// with its layer painting the history's animations just above it,
    /// and the composer slice below (painted after the layer, so its
    /// popups cover the history's animations).
    pub(super) fn conversation_slot(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let drop_state = self.visible_drop_state(cx);
        let drop_overlay = drop_state.map(|state| self.drop_zone_overlay(state, cx));
        let emoji_effects = self.emoji_effect_overlay(self.effects_window_active(), cx);
        let send_fly = self.send_fly_overlay(cx);
        let slot = div()
            .id("conversation-slot")
            .role(gpui_kit::Role::Group)
            .a11y_tag(quill::agent::ui_driver::redact::REGION_CHAT)
            .debug_selector(|| "conversation-slot".into())
            .relative()
            .flex()
            .flex_1()
            .min_w_0()
            .min_h_0()
            // Files dragged over the chat: classify them so the zones can
            // say what each one does (`drop_zones`).
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<ExternalPaths>, _, cx| {
                    let paths = event.drag(cx).paths().to_vec();
                    this.note_file_drag(&paths, cx);
                }),
            )
            // A drop outside any zone, or while the zones are off, attaches
            // the files the usual way.
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.drop_files_plain(paths.paths(), cx);
            }));
        // A chat the pull to the next channel opened slides in from below.
        let slide = self.pull_slide_offset(cx);
        let slot = if slide > 0. {
            slot.top(relative(slide))
        } else {
            slot
        };
        self.conversation_column(slot, cx)
            .children(drop_overlay)
            .children(emoji_effects)
            .children(send_fly)
            .into_any_element()
    }

    /// `column` filled with the conversation of the window whose state is
    /// in place: its cached slice with the layer painting the history's
    /// animations just above it, then the composer slice (painted after
    /// the layer, so its popups cover the history's animations).
    pub(super) fn conversation_column(
        &self,
        column: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let slices = &self.slices.window;
        let (Some(parts), Some(composer)) = (&slices.conversation, &slices.composer) else {
            return column;
        };
        slices.conversation_shown.set(true);
        let window = self.windows.scope;
        super::image_budget::slice_shown(parts.slice.entity_id(), window);
        super::image_budget::slice_shown(composer.entity_id(), window);
        let height = slices.composer_height.get();
        let hint = self.pull_hint_overlay(cx).map(|hint| {
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .h(height)
                .child(hint)
        });
        column
            .flex_col()
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .child(parts.slice.clone().cached(full()))
                    .child(parts.painter.clone().live()),
            )
            .child(
                composer
                    .clone()
                    .cached(StyleRefinement::default().w_full().h(height).flex_none()),
            )
            .children(
                self.media_panel_over_composer()
                    .then(|| self.panel_painter())
                    .flatten(),
            )
            .children(hint)
    }

    /// The media viewer's media box and chrome as cached slices filling
    /// the overlay (`None` before the slices exist).
    pub(super) fn viewer_slots(&self) -> Option<(AnyElement, AnyElement)> {
        let media = self.slices.viewer.as_ref()?;
        let chrome = self.slices.viewer_chrome.as_ref()?;
        super::image_budget::slice_shown(media.entity_id(), None);
        super::image_budget::slice_shown(chrome.entity_id(), None);
        // Stacked, not side by side in the overlay's row.
        let layer = || {
            StyleRefinement::default()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
        };
        Some((
            media.clone().cached(layer()).into_any_element(),
            chrome.clone().cached(layer()).into_any_element(),
        ))
    }

    /// The story viewer overlay as a cached slice filling the window
    /// (`None` before the slices exist).
    pub(super) fn story_slot(&self) -> Option<AnyElement> {
        let story = self.slices.story.as_ref()?;
        super::image_budget::slice_shown(story.entity_id(), None);
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .child(story.clone().cached(full()))
                .into_any_element(),
        )
    }

    fn slice_layer(&self, kind: SliceKind) -> Option<Layer> {
        match kind {
            SliceKind::Sidebar => self.slices.sidebar.as_ref(),
            SliceKind::Conversation => self.slices.window.conversation.as_ref(),
            SliceKind::Composer
            | SliceKind::Viewer
            | SliceKind::ViewerChrome
            | SliceKind::Story => None,
        }
        .map(|parts| parts.layer.clone())
    }

    fn render_slice(
        &mut self,
        kind: SliceKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match kind {
            SliceKind::Viewer | SliceKind::ViewerChrome => {
                if self.viewer.state.is_open() {
                    let part = if kind == SliceKind::Viewer {
                        super::media_viewer::ViewerPart::Media
                    } else {
                        super::media_viewer::ViewerPart::Chrome
                    };
                    self.media_viewer_panel(part, window, cx)
                } else {
                    Empty.into_any_element()
                }
            }
            SliceKind::Story => {
                if self.stories.viewer.is_open() {
                    let width = f32::from(window.viewport_size().width);
                    div()
                        .size_full()
                        .relative()
                        .child(self.story_viewer_overlay(width, cx))
                        .into_any_element()
                } else {
                    Empty.into_any_element()
                }
            }
            SliceKind::Sidebar => {
                if quill::startup_trace::enabled()
                    && self
                        .session()
                        .is_some_and(|session| !session.main_order.is_empty())
                {
                    quill::startup_trace::mark("chat_list");
                }
                let auth_state = self.current_auth();
                let auth = quill::auth::view_for(&auth_state);
                self.sidebar(&auth, false, false, false, false, window, cx)
            }
            SliceKind::Conversation => {
                self.slices.window.conversation_rendered.set(true);
                // Position ticks notify only the slices, so the knob and the
                // a11y value follow playback from here, not the root render.
                self.sync_seek_slider(window, cx);
                div()
                    .size_full()
                    .flex()
                    .child(self.conversation(ConversationPart::Top, cx))
                    .into_any_element()
            }
            SliceKind::Composer => {
                // Laid out at the bottom of its box: while the box still
                // has last frame's height, a taller composer grows over
                // the history (as its popups do) for that one frame.
                let redraw = self.windows.view_in_place().or(self.slices.app_only_id());
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .justify_end()
                    .child(MeasureHeight {
                        height: self.slices.window.composer_height.clone(),
                        redraw,
                        child: div()
                            .flex_none()
                            .flex()
                            .flex_col()
                            .child(self.conversation(ConversationPart::Bottom, cx))
                            .into_any_element(),
                    })
                    .into_any_element()
            }
        }
    }
}

fn full() -> StyleRefinement {
    StyleRefinement::default().size_full()
}

impl Render for AppSlice {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let id = cx.entity_id();
        let (kind, slice_window) = (self.kind, self.window);
        super::frame_clock::trace_slice_render(match (kind, slice_window) {
            (SliceKind::Sidebar, _) => "sidebar",
            (SliceKind::Conversation, None) => "conversation",
            (SliceKind::Conversation, Some(_)) => "chat window conversation",
            (SliceKind::Composer, None) => "composer",
            (SliceKind::Composer, Some(_)) => "chat window composer",
            (SliceKind::Viewer, _) => "viewer",
            (SliceKind::ViewerChrome, _) => "viewer-chrome",
            (SliceKind::Story, _) => "story",
        });
        let Some(app) = self.app.upgrade() else {
            return Empty.into_any_element();
        };
        app.update(cx, |app, cx| {
            if app.windows.scope != slice_window {
                return Empty.into_any_element();
            }
            super::image_budget::slice_rendered(id, slice_window);
            let scope = app.slices.scope.clone();
            let layer = app.slice_layer(kind);
            let outer = scope.replace(Some(id));
            let child = super::anim_layer::with_layer(layer.as_ref(), || {
                app.render_slice(kind, window, cx)
            });
            scope.set(outer);
            let frames = match kind {
                SliceKind::Composer => app.slices.panel_layer(),
                _ => layer.clone(),
            };
            SliceScope {
                id: Some(id),
                scope,
                layer,
                frames,
                child,
            }
            .into_any_element()
        })
    }
}

/// Marks a slice's subtree while it is laid out and painted, so animated
/// content built lazily there (virtual list rows) asks the frame clock on
/// behalf of the right slice, and reports to the slice's layer.
struct SliceScope {
    id: Option<EntityId>,
    scope: Rc<Cell<Option<EntityId>>>,
    layer: Option<Layer>,
    /// The layer whose frame this is: the slice's, or the media panel's,
    /// which only the panel's cells report to.
    frames: Option<Layer>,
    child: AnyElement,
}

impl SliceScope {
    fn enter<R>(&self, f: impl FnOnce() -> R) -> R {
        let outer = self.scope.replace(self.id);
        let result = super::anim_layer::with_layer(self.layer.as_ref(), f);
        self.scope.set(outer);
        result
    }
}

impl IntoElement for SliceScope {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SliceScope {
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
        let layout_id = self.enter(|| child.request_layout(window, cx));
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
        // The slice really draws: its layer gets this frame's content.
        if let Some(layer) = &self.frames {
            layer.begin_frame();
        }
        let mut child = std::mem::replace(&mut self.child, Empty.into_any_element());
        self.enter(|| child.prepaint(window, cx));
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
        self.enter(|| child.paint(window, cx));
        self.child = child;
        if let Some(layer) = &self.frames {
            layer.end_frame();
        }
    }
}

/// Records its child's laid-out height; when it changed, the slot that
/// sizes the composer slice is redrawn (`redraw`: [`AppOnly`], or the chat
/// window).
struct MeasureHeight {
    height: Rc<Cell<Pixels>>,
    redraw: Option<EntityId>,
    child: AnyElement,
}

impl IntoElement for MeasureHeight {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for MeasureHeight {
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
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let height = bounds.size.height.ceil();
        if self.height.replace(height) != height
            && let Some(id) = self.redraw
        {
            cx.notify(id);
        }
        self.child.prepaint(window, cx);
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
        self.child.paint(window, cx);
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "scene_replay_tests.rs"]
mod scene_replay_tests;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "drag_redraw_tests.rs"]
mod drag_redraw_tests;
