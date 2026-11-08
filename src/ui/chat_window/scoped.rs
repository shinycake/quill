//! Work that finishes later, for the screen and the window that started it.
//!
//! A timer, a file picker or a background job started while a chat
//! window's state was in place (`slot`) finishes after that scope was left,
//! usually with the main window's state in place. Run as-is, its update
//! would read and write the main window's composer, viewer or history
//! instead of its own. The same holds one level up: work started while one
//! account was on screen finishes after a switch, and run as-is would act
//! through the client of the account switched to, with the first account's
//! chat and message ids (docs/decisions/account-scoped-ui-state.md).
//!
//! Started through one of the helpers below the task gets a [`ScopedApp`]
//! in place of the app's weak handle. Every update checks the screen it
//! belongs to ([`ScreenEpoch`]) and puts the starting window's state back
//! in place first; none runs once that screen or window is gone.
//!
//! | Helper | Screen | Window |
//! | --- | --- | --- |
//! | [`QuillApp::spawn_scoped`] / `_in` | the one on screen now | the one whose state is in place |
//! | [`QuillApp::spawn_screen`] / `_in` | the one on screen now | the main window |
//! | [`QuillApp::spawn_app`] / `_in` | none (the app's own work) | the main window |
//! | [`QuillApp::defer_screen`] / `_in` | the one on screen now | the main window |
//! | [`QuillApp::in_screen_scope`] | the given one | the main window |
//!
//! `scripts/check-window-scope.py` flags async work in `src/ui` that touches
//! window or screen state without one of these.

use super::super::app::QuillApp;
pub(in crate::ui) use super::super::on_screen::ScreenEpoch;
use gpui_kit::*;
use quill::ids::ChatId;

/// The task's app, screen or window is gone: the update did not run.
#[derive(Debug)]
pub(in crate::ui) struct Gone;

/// The app, as a task started on one screen and in one window sees it.
#[derive(Clone)]
pub(in crate::ui) struct ScopedApp {
    app: WeakEntity<QuillApp>,
    /// The screen the task belongs to (`None`: the app's own work, which
    /// runs whichever account is on screen).
    screen: Option<ScreenEpoch>,
    /// The window the task belongs to (`None`: the main window).
    window: Option<ChatId>,
}

impl ScopedApp {
    /// Update the app with the task's screen current and its window state
    /// in place. [`Gone`] once the app, that screen or that window is.
    pub(in crate::ui) fn update<C: AppContext, R>(
        &self,
        cx: &mut C,
        f: impl FnOnce(&mut QuillApp, &mut Context<QuillApp>) -> R,
    ) -> Result<R, Gone> {
        let screen = self.screen;
        let window = self.window;
        self.app
            .update(cx, |app, cx| {
                if screen.is_some_and(|screen| app.screen_epoch() != screen) {
                    return None;
                }
                app.in_window_scope(window, cx, f)
            })
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
        let screen = self.screen;
        let scope = self.window;
        self.app
            .update_in(cx, |app, window, cx| {
                if screen.is_some_and(|screen| app.screen_epoch() != screen) {
                    return None;
                }
                app.in_window_scope(scope, cx, |app, cx| f(app, window, cx))
            })
            .ok()
            .flatten()
            .ok_or(Gone)
    }

    /// The same task as the main window sees it, on the same screen, for
    /// what it still owes once its own window has closed (a note that its
    /// result was dropped).
    pub(in crate::ui) fn main(&self) -> ScopedApp {
        ScopedApp {
            app: self.app.clone(),
            screen: self.screen,
            window: None,
        }
    }

    /// The same task as the app sees it, on whichever screen is current,
    /// for what it owes once its screen is gone (a file to remove, a note
    /// that nothing was sent).
    pub(in crate::ui) fn app(&self) -> ScopedApp {
        ScopedApp {
            app: self.app.clone(),
            screen: None,
            window: None,
        }
    }
}

impl QuillApp {
    /// Run `f` with the state of the window `window` is in place: a chat
    /// window's, else the main window's (also for the app's other
    /// windows). GPUI calls a subscription or an observer made with
    /// `subscribe_in` / `observe_*` with the window it was made in, but
    /// whatever state is in place at that moment: an edit made inside
    /// another window's `in_window_scope` emits after that scope is gone.
    /// A closed window's subscriptions do not run at all.
    pub(in crate::ui) fn in_window_of<R>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) -> R,
    ) -> Option<R> {
        let scope = self.window_scope_of(window);
        self.in_window_scope(scope, cx, |app, cx| f(app, window, cx))
    }

    /// Run `f` as the main window of the screen `screen`, for a `cx.defer`
    /// body or a subscription made on another entity that continues work
    /// started on that screen. `None`, without running `f`, once another
    /// account is on screen.
    pub(in crate::ui) fn in_screen_scope<R>(
        &mut self,
        screen: ScreenEpoch,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Self, &mut Context<Self>) -> R,
    ) -> Option<R> {
        if self.screen_epoch() != screen {
            return None;
        }
        self.in_window_scope(None, cx, f)
    }

    /// `cx.defer` for work that continues the account on screen now: the
    /// body runs as the main window of this screen, or not at all.
    /// For a surface only the main window draws (the story viewer, the
    /// profile modal), opened by input in a chat window: run `f` at the end
    /// of this update as the main window, with its `Window`, and bring that
    /// window forward, so the surface shows where it is drawn and what it
    /// builds is the main window's. `false`, without running `f`, when the
    /// main window's state is in place already (the caller goes on there).
    pub(in crate::ui) fn hand_to_main_window(
        &mut self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> bool {
        if self.windows.scope.is_none() {
            return false;
        }
        let Some(main) = self.main_window.handle else {
            return true;
        };
        let screen = self.screen_epoch();
        let app = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, cx| {
                crate::ui::link_inbox::raise_main_window(window, cx);
                let _ = app.update(cx, |this, cx| {
                    this.in_screen_scope(screen, cx, |this, cx| {
                        this.in_window_scope(None, cx, |this, cx| f(this, window, cx));
                    });
                });
            });
        });
        true
    }

    /// Work aimed at `chat` when a chat window shows it: run `f` at the end
    /// of this update in that window, with its `Window` and its state in
    /// place, and bring it forward. `false`, without running `f`, when no
    /// chat window shows `chat`.
    pub(in crate::ui) fn hand_to_chat_window(
        &mut self,
        chat: ChatId,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> bool {
        let Some(handle) = self
            .windows
            .open_windows()
            .find_map(|(open, handle)| (open == chat).then_some(handle))
        else {
            return false;
        };
        let screen = self.screen_epoch();
        let app = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                window.activate_window();
                let _ = app.update(cx, |this, cx| {
                    this.in_screen_scope(screen, cx, |this, cx| {
                        this.in_window_scope(Some(chat), cx, |this, cx| f(this, window, cx));
                    });
                });
            });
        });
        true
    }

    pub(in crate::ui) fn defer_screen(
        &self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
    ) {
        let screen = self.screen_epoch();
        let app = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = app.update(cx, |this, cx| {
                this.in_screen_scope(screen, cx, f);
            });
        });
    }

    /// `cx.defer_in` for work that continues the account on screen now.
    pub(in crate::ui) fn defer_screen_in(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) {
        let screen = self.screen_epoch();
        cx.defer_in(window, move |this, window, cx| {
            this.in_screen_scope(screen, cx, |this, cx| f(this, window, cx));
        });
    }

    /// The handle a task started now carries: the screen on screen and the
    /// window whose state is in place.
    fn scoped_app(&self, cx: &Context<Self>) -> ScopedApp {
        ScopedApp {
            app: cx.weak_entity(),
            screen: Some(self.screen_epoch()),
            window: self.windows.scope,
        }
    }

    /// The handle for work that belongs to the account on screen now, in
    /// the main window.
    fn screen_app(&self, cx: &Context<Self>) -> ScopedApp {
        ScopedApp {
            app: cx.weak_entity(),
            screen: Some(self.screen_epoch()),
            window: None,
        }
    }

    /// The handle for the app's own work: it runs whichever account is on
    /// screen, as the main window.
    fn app_app(cx: &Context<Self>) -> ScopedApp {
        ScopedApp {
            app: cx.weak_entity(),
            screen: None,
            window: None,
        }
    }

    /// `cx.spawn` for work that belongs to the window whose state is in
    /// place now, on the screen on screen now.
    pub(in crate::ui) fn spawn_scoped<AsyncFn, R>(&self, cx: &Context<Self>, f: AsyncFn) -> Task<R>
    where
        AsyncFn: AsyncFnOnce(ScopedApp, &mut AsyncApp) -> R + 'static,
        R: 'static,
    {
        let app = self.scoped_app(cx);
        cx.spawn(async move |_, cx| f(app, cx).await)
    }

    /// `cx.spawn_in` for work that belongs to the window whose state is in
    /// place now, on the screen on screen now.
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

    /// `cx.spawn` for work that belongs to the account on screen now and
    /// to no chat window: its updates run as the main window, and not at
    /// all once another account is on screen.
    pub(in crate::ui) fn spawn_screen<AsyncFn, R>(&self, cx: &Context<Self>, f: AsyncFn) -> Task<R>
    where
        AsyncFn: AsyncFnOnce(ScopedApp, &mut AsyncApp) -> R + 'static,
        R: 'static,
    {
        let app = self.screen_app(cx);
        cx.spawn(async move |_, cx| f(app, cx).await)
    }

    /// `cx.spawn_in` for work that belongs to the account on screen now
    /// and to no chat window.
    pub(in crate::ui) fn spawn_screen_in<AsyncFn, R>(
        &self,
        window: &Window,
        cx: &Context<Self>,
        f: AsyncFn,
    ) -> Task<R>
    where
        AsyncFn: AsyncFnOnce(ScopedApp, &mut AsyncWindowContext) -> R + 'static,
        R: 'static,
    {
        let app = self.screen_app(cx);
        cx.spawn_in(window, async move |_, cx| f(app, cx).await)
    }

    /// `cx.spawn` for the app's own work (the poll loop, background
    /// accounts, the passcode, the updater): it runs whichever account is
    /// on screen, as the main window. Its body must not touch the screen's
    /// state; the lint checks.
    pub(in crate::ui) fn spawn_app<AsyncFn, R>(cx: &Context<Self>, f: AsyncFn) -> Task<R>
    where
        AsyncFn: AsyncFnOnce(ScopedApp, &mut AsyncApp) -> R + 'static,
        R: 'static,
    {
        let app = Self::app_app(cx);
        cx.spawn(async move |_, cx| f(app, cx).await)
    }

    /// `cx.spawn_in` for the app's own work.
    pub(in crate::ui) fn spawn_app_in<AsyncFn, R>(
        window: &Window,
        cx: &Context<Self>,
        f: AsyncFn,
    ) -> Task<R>
    where
        AsyncFn: AsyncFnOnce(ScopedApp, &mut AsyncWindowContext) -> R + 'static,
        R: 'static,
    {
        let app = Self::app_app(cx);
        cx.spawn_in(window, async move |_, cx| f(app, cx).await)
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "scoped_tests.rs"]
mod tests;
