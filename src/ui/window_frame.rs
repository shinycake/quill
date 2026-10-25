//! "Use system window frame" (tdesktop Settings > Advanced, Linux and
//! Windows only): the operating system draws the window's title bar and
//! caption buttons instead of Quill's own bar.
//!
//! Off (tdesktop's default) is Quill's frame: the kit `TitleBar` with its
//! own drag area and, on Linux, its own minimize/maximize/close buttons under
//! client-side decorations. On, the window asks for server-side decorations
//! (Linux) or shows the system title bar (Windows), and the bar below the
//! frame keeps only the action buttons: the OS already shows the window
//! title and the caption buttons. The change applies at once to every open
//! window, like tdesktop's `nativeWindowFrameChanges`. macOS always has its
//! own frame and shows no switch.
#![deny(clippy::indexing_slicing)]

use gpui_kit::component::{ActiveTheme, TITLE_BAR_HEIGHT, TitleBar};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use smallvec::SmallVec;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Whether the platform lets the user choose (tdesktop's
/// `Ui::Platform::NativeWindowFrameSupported`: Linux and Windows).
pub(crate) const fn supported() -> bool {
    cfg!(any(target_os = "linux", target_os = "windows"))
}

/// The saved choice, mirrored here so every window's title bar can read it
/// without the app lease.
static NATIVE_FRAME: AtomicBool = AtomicBool::new(false);

/// Whether windows are drawn with the system frame right now. Always false
/// where the platform has no choice.
pub(crate) fn use_native() -> bool {
    supported() && NATIVE_FRAME.load(Ordering::Relaxed)
}

/// Record the saved choice. Returns whether it changed.
pub(crate) fn set_native(native: bool) -> bool {
    NATIVE_FRAME.swap(native, Ordering::Relaxed) != native
}

thread_local! {
    /// The frame each open window was last asked for, so a frame that is
    /// already right (or a backend that cannot honor the request, such as X11
    /// without a compositor) is not asked again on every draw.
    static APPLIED: RefCell<HashMap<WindowId, bool>> = RefCell::new(HashMap::new());
}

/// Make `window` follow the saved choice. Cheap when nothing changed, so
/// every window calls it from its render.
pub(crate) fn sync(window: &mut Window, native: bool) {
    if !supported() {
        return;
    }
    set_native(native);
    let id = window.window_handle().window_id();
    let changed = APPLIED.with(|applied| {
        let mut applied = applied.borrow_mut();
        applied.insert(id, native) != Some(native)
    });
    if changed {
        platform::apply(window, native);
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;

    /// X11 and Wayland both take the request live (`_MOTIF_WM_HINTS`, or an
    /// xdg-decoration mode); Wayland falls back to client-side decorations
    /// when the compositor has no server-side ones.
    pub(super) fn apply(window: &mut Window, native: bool) {
        window.request_decorations(if native {
            WindowDecorations::Server
        } else {
            WindowDecorations::Client
        });
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    /// `gpui_windows::native_frame` (vendored): flips the title-bar hiding
    /// and sends `SWP_FRAMECHANGED`.
    pub(super) fn apply(window: &mut Window, native: bool) {
        if let Ok(RawWindowHandle::Win32(handle)) =
            HasWindowHandle::window_handle(window).map(|handle| handle.as_raw())
        {
            gpui_windows::native_frame::set_native_frame(handle.hwnd.get(), native);
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
mod platform {
    use super::*;

    pub(super) fn apply(_window: &mut Window, _native: bool) {}
}

/// The frame a new window is created with, from the saved choice (set with
/// [`set_native`] before the first window opens). Linux defaults to
/// client-side decorations: Quill's frame is the default, as in tdesktop, and
/// the kit `TitleBar` draws its caption buttons only then.
pub(crate) fn window_decorations() -> Option<WindowDecorations> {
    cfg!(target_os = "linux").then(|| {
        if use_native() {
            WindowDecorations::Server
        } else {
            WindowDecorations::Client
        }
    })
}

/// kit Phase 7: window options compatible with kit's `TitleBar`: the title
/// bar owns dragging (double-click zoom included), so the platform must not
/// also treat it as a system move region. The frame follows the saved choice.
/// These are the main window's options (and its screenshot demos'), so they
/// carry its minimum size; interface zoom scales it when the window opens.
pub(crate) fn window_options(title: &str) -> WindowOptions {
    use crate::ui::shell_layout::{WINDOW_MIN_HEIGHT, WINDOW_MIN_WIDTH};
    WindowOptions {
        window_min_size: Some(size(px(WINDOW_MIN_WIDTH), px(WINDOW_MIN_HEIGHT))),
        titlebar: Some(TitlebarOptions {
            title: Some(title.into()),
            appears_transparent: titlebar_hidden(),
            ..TitleBar::title_bar_options()
        }),
        app_owns_titlebar_drag: true,
        window_decorations: window_decorations(),
        ..Default::default()
    }
}

/// `TitlebarOptions::appears_transparent` for a new window: false asks
/// Windows for the system title bar. macOS and Quill's own frame hide it.
pub(crate) fn titlebar_hidden() -> bool {
    !use_native()
}

/// How far content drawn over the window's top edge (the media viewer's
/// top bar) must start from the left to clear the macOS traffic lights: the
/// kit `TitleBar`'s own left padding. Zero full screen, where macOS hides
/// them, and on Linux and Windows, whose caption buttons are drawn by the
/// title bar under that content.
pub(crate) fn traffic_light_inset(window: &Window) -> Pixels {
    if cfg!(target_os = "macos") && !window.is_fullscreen() {
        px(80.)
    } else {
        px(0.)
    }
}

/// The bar above the window content: the kit `TitleBar` with Quill's own
/// frame, or, with the system frame, a slim action row that drops the caption
/// and the caption buttons (the OS draws both) and the drag area.
///
/// An in-window menu bar (Linux and Windows) leads the same row under both
/// frames, in place of the caption, so the window has one bar, not two.
#[derive(IntoElement)]
pub(crate) struct FrameTitleBar {
    caption: Option<AnyElement>,
    menu: Option<AnyElement>,
    children: SmallVec<[AnyElement; 2]>,
    on_close_window: Option<Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
    /// Overrides [`use_native`] (tests draw both frames on any platform).
    native: Option<bool>,
}

impl FrameTitleBar {
    pub(crate) fn new() -> Self {
        Self {
            caption: None,
            menu: None,
            children: SmallVec::new(),
            on_close_window: None,
            native: None,
        }
    }

    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(crate) fn system_frame(mut self, native: bool) -> Self {
        self.native = Some(native);
        self
    }

    /// The window title, shown only in Quill's own bar.
    pub(crate) fn caption(mut self, caption: impl IntoElement) -> Self {
        self.caption = Some(caption.into_any_element());
        self
    }

    /// The in-window menu bar, leading the row under both frames. It takes
    /// the caption's place: the OS already names the window in its taskbar.
    pub(crate) fn menu(mut self, menu: impl IntoElement) -> Self {
        self.menu = Some(menu.into_any_element());
        self
    }

    /// The close button of Quill's own bar (Linux).
    pub(crate) fn on_close_window(
        mut self,
        f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_close_window = Some(Rc::new(f));
        self
    }
}

impl ParentElement for FrameTitleBar {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for FrameTitleBar {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        // The menu fills the free width so its trailing space still drags.
        let menu = self.menu.map(|menu| {
            div()
                .h_full()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .child(menu)
        });
        if self.native.unwrap_or_else(use_native) {
            if self.children.is_empty() && menu.is_none() {
                return div().into_any_element();
            }
            let theme = cx.theme();
            return div()
                .flex_shrink_0()
                .h(TITLE_BAR_HEIGHT)
                .px_3()
                .flex()
                .items_center()
                .justify_end()
                .gap_2()
                .bg(theme.title_bar)
                .border_b_1()
                .border_color(theme.title_bar_border)
                .children(menu)
                .children(self.children)
                .into_any_element();
        }
        let on_close = self.on_close_window;
        let lead = match menu {
            Some(menu) => Some(menu.into_any_element()),
            None => self.caption,
        };
        TitleBar::new()
            .when_some(on_close, |bar, close| {
                bar.on_close_window(move |event, window, cx| {
                    if !quill::agent::ui_driver::guard::refuse_hard_if_injecting(
                        quill::agent::ui_driver::guard::HardSurface::Window,
                    ) {
                        close(event, window, cx);
                    }
                })
            })
            .children(lead)
            .children(self.children)
            .into_any_element()
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "window_frame_tests.rs"]
mod tests;
