//! Work that finishes later, in the window that started it.
//!
//! A timer, a file picker or a background job started while a chat
//! window's state was in place (`slot`) finishes after that scope was left,
//! usually with the main window's state in place. Run as-is, its update
//! would read and write the main window's composer, viewer or history
//! instead of its own. Started through [`QuillApp::spawn_scoped`] (or
//! [`QuillApp::spawn_scoped_in`], [`QuillApp::defer_scoped`]) the task
//! gets a [`ScopedApp`] in place of the app's weak handle: every update
//! puts the starting window's state back in place first, and none runs
//! once that window has closed. Work that belongs to no window (the poll
//! loop, background accounts) uses [`QuillApp::spawn_main`], which
//! always runs as the main window. `scripts/check-window-scope.py` flags
//! async work in `src/ui` that touches window state without one of these.

use super::super::app::QuillApp;
use gpui_kit::*;
use quill::ids::ChatId;

/// The task's app or window is gone: the update did not run.
#[derive(Debug)]
pub(in crate::ui) struct Gone;

/// The app, as a task started in one window sees it.
#[derive(Clone)]
pub(in crate::ui) struct ScopedApp {
    app: WeakEntity<QuillApp>,
    /// The window the task belongs to (`None`: the main window).
    window: Option<ChatId>,
}

impl ScopedApp {
    /// Update the app with the task's window state in place. [`Gone`] once
    /// the app or that window is.
    pub(in crate::ui) fn update<C: AppContext, R>(
        &self,
        cx: &mut C,
        f: impl FnOnce(&mut QuillApp, &mut Context<QuillApp>) -> R,
    ) -> Result<R, Gone> {
        let window = self.window;
        self.app
            .update(cx, |app, cx| app.in_window_scope(window, cx, f))
            .ok()
            .flatten()
            .ok_or(Gone)
    }

    /// [`Self::update`] with the GPUI window the task was spawned in.
    pub(in crate::ui) fn update_in<C: AppContext, R>(
        &self,
        cx: &mut C,
        f: impl FnOnce(&mut QuillApp, &mut Window, &mut Context<QuillApp>) -> R,
    ) -> Result<R, Gone> {
        let scope = self.window;
        self.app
            .update_in(cx, |app, window, cx| {
                app.in_window_scope(scope, cx, |app, cx| f(app, window, cx))
            })
            .ok()
            .flatten()
            .ok_or(Gone)
    }
}

impl QuillApp {
    /// The handle a task started now carries: the window whose state is in
    /// place.
    fn scoped_app(&self, cx: &Context<Self>) -> ScopedApp {
        ScopedApp {
            app: cx.weak_entity(),
            window: self.windows.scope,
        }
    }

    /// `cx.spawn` for work that belongs to the window whose state is in
    /// place now.
    pub(in crate::ui) fn spawn_scoped<AsyncFn, R>(&self, cx: &Context<Self>, f: AsyncFn) -> Task<R>
    where
        AsyncFn: AsyncFnOnce(ScopedApp, &mut AsyncApp) -> R + 'static,
        R: 'static,
    {
        let app = self.scoped_app(cx);
        cx.spawn(async move |_, cx| f(app, cx).await)
    }

    /// `cx.spawn_in` for work that belongs to the window whose state is in
    /// place now.
    pub(in crate::ui) fn spawn_scoped_in<AsyncFn, R>(
        &self,
        window: &Window,
        cx: &Context<Self>,
        f: AsyncFn,
    ) -> Task<R>
    where
        AsyncFn: AsyncFnOnce(ScopedApp, &mut AsyncWindowContext) -> R + 'static,
        R: 'static,
    {
        let app = self.scoped_app(cx);
        cx.spawn_in(window, async move |_, cx| f(app, cx).await)
    }

    /// `cx.spawn` for work that belongs to no chat window: its updates run
    /// as the main window.
    pub(in crate::ui) fn spawn_main<AsyncFn, R>(cx: &Context<Self>, f: AsyncFn) -> Task<R>
    where
        AsyncFn: AsyncFnOnce(ScopedApp, &mut AsyncApp) -> R + 'static,
        R: 'static,
    {
        let app = ScopedApp {
            app: cx.weak_entity(),
            window: None,
        };
        cx.spawn(async move |_, cx| f(app, cx).await)
    }

    /// `cx.spawn_in` for work that belongs to no chat window: its updates
    /// run as the main window.
    pub(in crate::ui) fn spawn_main_in<AsyncFn, R>(
        window: &Window,
        cx: &Context<Self>,
        f: AsyncFn,
    ) -> Task<R>
    where
        AsyncFn: AsyncFnOnce(ScopedApp, &mut AsyncWindowContext) -> R + 'static,
        R: 'static,
    {
        let app = ScopedApp {
            app: cx.weak_entity(),
            window: None,
        };
        cx.spawn_in(window, async move |_, cx| f(app, cx).await)
    }

    /// `cx.defer_in` for work that belongs to the window whose state is in
    /// place now.
    pub(in crate::ui) fn defer_scoped(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) {
        let scope = self.windows.scope;
        cx.defer_in(window, move |app, window, cx| {
            app.in_window_scope(scope, cx, |app, cx| f(app, window, cx));
        });
    }
}
